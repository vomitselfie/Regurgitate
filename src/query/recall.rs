use std::collections::{BTreeMap, HashMap};

use anyhow::{Result, bail};
use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use crate::{
    application::ProjectLocator,
    core::{Capability, ErrorClass, HistoryEvent, Operation, Outcome, Strategy},
};

pub const DEFAULT_RECALL_LIMIT: usize = 10;
pub const MAX_RECALL_LIMIT: usize = 20;
const MAX_CANDIDATE_EVENTS: usize = 1_000;

pub trait ProjectLookup {
    fn find_project(&self, locator: &ProjectLocator) -> Result<Option<Uuid>>;
}

pub trait ProjectEventSource {
    fn recent_project_events(&self, project_id: Uuid, limit: usize) -> Result<Vec<HistoryEvent>>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecallOptions {
    pub operation: Option<Operation>,
    pub failures_only: bool,
    pub limit: usize,
}

impl Default for RecallOptions {
    fn default() -> Self {
        Self {
            operation: None,
            failures_only: false,
            limit: DEFAULT_RECALL_LIMIT,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RecallResult {
    pub observations: Vec<RecallObservation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RecallObservation {
    pub capability: Capability,
    pub operation: Operation,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strategy: Option<Strategy>,
    pub attempts: usize,
    pub successes: usize,
    pub failures: usize,
    pub unknown: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub common_error: Option<ErrorClass>,
}

pub struct RecallService<'a, H> {
    history: &'a H,
}

impl<'a, H> RecallService<'a, H>
where
    H: ProjectLookup + ProjectEventSource,
{
    pub fn new(history: &'a H) -> Self {
        Self { history }
    }

    pub fn recall(&self, locator: &ProjectLocator, options: RecallOptions) -> Result<RecallResult> {
        if options.limit == 0 || options.limit > MAX_RECALL_LIMIT {
            bail!("recall limit must be between 1 and {MAX_RECALL_LIMIT}");
        }
        let Some(project_id) = self.history.find_project(locator)? else {
            return Ok(RecallResult {
                observations: Vec::new(),
            });
        };
        let events = self
            .history
            .recent_project_events(project_id, MAX_CANDIDATE_EVENTS)?;
        Ok(RecallResult {
            observations: aggregate(events, options),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct GroupKey {
    capability: Capability,
    operation: Operation,
    strategy: Option<Strategy>,
}

struct Group {
    attempts: usize,
    successes: usize,
    failures: usize,
    unknown: usize,
    errors: BTreeMap<ErrorClass, usize>,
    latest: DateTime<Utc>,
}

fn aggregate(events: Vec<HistoryEvent>, options: RecallOptions) -> Vec<RecallObservation> {
    let mut groups = HashMap::<GroupKey, Group>::new();
    for event in events {
        if options
            .operation
            .is_some_and(|wanted| event.operation != wanted)
            || (options.failures_only && event.outcome != Outcome::Failure)
        {
            continue;
        }
        let key = GroupKey {
            capability: event.capability,
            operation: event.operation,
            strategy: event.strategy,
        };
        let group = groups.entry(key).or_insert_with(|| Group {
            attempts: 0,
            successes: 0,
            failures: 0,
            unknown: 0,
            errors: BTreeMap::new(),
            latest: event.timestamp,
        });
        group.attempts += 1;
        match event.outcome {
            Outcome::Success => group.successes += 1,
            Outcome::Failure => group.failures += 1,
            Outcome::Unknown => group.unknown += 1,
        }
        if let Some(error) = event.error_class {
            *group.errors.entry(error).or_default() += 1;
        }
        group.latest = group.latest.max(event.timestamp);
    }

    let mut ranked: Vec<_> = groups
        .into_iter()
        .map(|(key, group)| {
            let common_error = group
                .errors
                .into_iter()
                .max_by_key(|(error, count)| (*count, std::cmp::Reverse(*error)))
                .map(|(error, _)| error);
            let score = group.successes.saturating_mul(4)
                + group.failures.saturating_mul(3)
                + group.unknown;
            (
                score,
                group.latest,
                key,
                RecallObservation {
                    capability: key.capability,
                    operation: key.operation,
                    strategy: key.strategy,
                    attempts: group.attempts,
                    successes: group.successes,
                    failures: group.failures,
                    unknown: group.unknown,
                    common_error,
                },
            )
        })
        .collect();
    ranked.sort_by(|left, right| {
        right
            .0
            .cmp(&left.0)
            .then_with(|| right.1.cmp(&left.1))
            .then_with(|| left.2.capability.cmp(&right.2.capability))
            .then_with(|| left.2.operation.cmp(&right.2.operation))
            .then_with(|| left.2.strategy.cmp(&right.2.strategy))
    });
    ranked
        .into_iter()
        .take(options.limit)
        .map(|(_, _, _, observation)| observation)
        .collect()
}

#[cfg(test)]
mod tests {
    use std::{cell::Cell, path::PathBuf};

    use chrono::TimeZone;

    use crate::core::{AgentKind, CURRENT_SCHEMA_VERSION};

    use super::*;

    struct MemoryHistory {
        project_id: Option<Uuid>,
        events: Vec<HistoryEvent>,
        requested_limit: Cell<usize>,
    }

    impl ProjectLookup for MemoryHistory {
        fn find_project(&self, _locator: &ProjectLocator) -> Result<Option<Uuid>> {
            Ok(self.project_id)
        }
    }

    impl ProjectEventSource for MemoryHistory {
        fn recent_project_events(
            &self,
            _project_id: Uuid,
            limit: usize,
        ) -> Result<Vec<HistoryEvent>> {
            self.requested_limit.set(limit);
            Ok(self.events.iter().take(limit).cloned().collect())
        }
    }

    fn event(id: u128, operation: Operation, outcome: Outcome) -> HistoryEvent {
        HistoryEvent {
            id: Uuid::from_u128(id),
            timestamp: Utc
                .timestamp_millis_opt(1_776_254_400_000 + id as i64)
                .unwrap(),
            session_id: Some("PRIVATE_SESSION".to_owned()),
            project_id: Some(Uuid::from_u128(7)),
            agent: Some(AgentKind::Codex),
            capability: Capability::Test,
            operation,
            strategy: Some(Strategy::NativeTool),
            outcome,
            duration_ms: None,
            error_class: (outcome == Outcome::Failure).then_some(ErrorClass::TestFailure),
            schema_version: CURRENT_SCHEMA_VERSION,
        }
    }

    #[test]
    fn returns_bounded_aggregate_observations_without_identifiers() {
        let history = MemoryHistory {
            project_id: Some(Uuid::from_u128(7)),
            events: vec![
                event(1, Operation::Command, Outcome::Success),
                event(2, Operation::Command, Outcome::Failure),
                event(3, Operation::ApplyPatch, Outcome::Success),
            ],
            requested_limit: Cell::new(0),
        };
        let result = RecallService::new(&history)
            .recall(
                &ProjectLocator::new(PathBuf::from("/private/project")),
                RecallOptions {
                    operation: Some(Operation::Command),
                    failures_only: false,
                    limit: 1,
                },
            )
            .unwrap();

        assert_eq!(history.requested_limit.get(), MAX_CANDIDATE_EVENTS);
        assert_eq!(result.observations.len(), 1);
        assert_eq!(result.observations[0].attempts, 2);
        assert_eq!(result.observations[0].successes, 1);
        assert_eq!(result.observations[0].failures, 1);
        let encoded = serde_json::to_string(&result).unwrap();
        for forbidden in [
            "PRIVATE_SESSION",
            "/private/project",
            &Uuid::from_u128(7).to_string(),
        ] {
            assert!(!encoded.contains(forbidden));
        }
    }

    #[test]
    fn rejects_enumeration_sized_limits_before_querying_storage() {
        let history = MemoryHistory {
            project_id: Some(Uuid::nil()),
            events: Vec::new(),
            requested_limit: Cell::new(0),
        };
        let error = RecallService::new(&history)
            .recall(
                &ProjectLocator::new(PathBuf::from("/private/project")),
                RecallOptions {
                    limit: MAX_RECALL_LIMIT + 1,
                    ..RecallOptions::default()
                },
            )
            .unwrap_err();
        assert!(error.to_string().contains("recall limit"));
        assert_eq!(history.requested_limit.get(), 0);
    }

    #[test]
    fn unknown_project_returns_no_observations() {
        let history = MemoryHistory {
            project_id: None,
            events: Vec::new(),
            requested_limit: Cell::new(0),
        };
        let result = RecallService::new(&history)
            .recall(
                &ProjectLocator::new(PathBuf::from("/private/project")),
                RecallOptions::default(),
            )
            .unwrap();
        assert!(result.observations.is_empty());
        assert_eq!(history.requested_limit.get(), 0);
    }

    #[test]
    fn hard_limit_survives_many_distinct_groups() {
        let operations = [
            Operation::Command,
            Operation::ContinueCommand,
            Operation::ApplyPatch,
            Operation::ReadFile,
            Operation::WriteFile,
            Operation::Search,
            Operation::WebRequest,
            Operation::InspectImage,
            Operation::UpdatePlan,
            Operation::Delegate,
            Operation::Wait,
            Operation::ToolCall,
        ];
        let mut events = Vec::new();
        for id in 0..24_u128 {
            let mut item = event(
                id + 1,
                operations[id as usize % operations.len()],
                Outcome::Success,
            );
            item.strategy = if id < 12 {
                Some(Strategy::NativeTool)
            } else {
                Some(Strategy::StructuredPatch)
            };
            events.push(item);
        }
        let history = MemoryHistory {
            project_id: Some(Uuid::from_u128(7)),
            events,
            requested_limit: Cell::new(0),
        };
        let result = RecallService::new(&history)
            .recall(
                &ProjectLocator::new(PathBuf::from("/private/project")),
                RecallOptions {
                    limit: MAX_RECALL_LIMIT,
                    ..RecallOptions::default()
                },
            )
            .unwrap();
        assert_eq!(result.observations.len(), MAX_RECALL_LIMIT);
    }
}
