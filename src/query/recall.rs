use std::collections::{BTreeMap, HashMap};

use anyhow::{Result, bail};
use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use crate::{
    application::ProjectLocator,
    core::{
        Capability, ErrorClass, EvidenceKind, HistoryEvent, Operation, Outcome, Strategy, TaskKind,
    },
};

use super::task::TaskIntent;

pub const DEFAULT_RECALL_LIMIT: usize = 10;
pub const MAX_RECALL_LIMIT: usize = 20;
pub const DEFAULT_TOKEN_BUDGET: usize = 300;
pub const MAX_TOKEN_BUDGET: usize = 1_000;
const MIN_TOKEN_BUDGET: usize = 32;
const MAX_CANDIDATE_EVENTS_PER_KIND: usize = 1_000;

pub trait ProjectLookup {
    fn find_project(&self, locator: &ProjectLocator) -> Result<Option<Uuid>>;
}

pub trait ProjectEventSource {
    fn recent_project_events(
        &self,
        project_id: Uuid,
        evidence_kind: EvidenceKind,
        limit: usize,
    ) -> Result<Vec<HistoryEvent>>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecallOptions {
    pub operation: Option<Operation>,
    pub failures_only: bool,
    pub limit: usize,
    pub token_budget: usize,
}

impl Default for RecallOptions {
    fn default() -> Self {
        Self {
            operation: None,
            failures_only: false,
            limit: DEFAULT_RECALL_LIMIT,
            token_budget: DEFAULT_TOKEN_BUDGET,
        }
    }
}

impl RecallOptions {
    pub fn validate(&self) -> Result<()> {
        if self.limit == 0 || self.limit > MAX_RECALL_LIMIT {
            bail!("recall limit must be between 1 and {MAX_RECALL_LIMIT}");
        }
        if self.token_budget < MIN_TOKEN_BUDGET || self.token_budget > MAX_TOKEN_BUDGET {
            bail!("recall token budget must be between {MIN_TOKEN_BUDGET} and {MAX_TOKEN_BUDGET}");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RecallResult {
    pub observations: Vec<RecallObservation>,
    pub hook_summary: HookSummary,
    pub approximate_tokens: usize,
}

impl RecallResult {
    pub fn empty() -> Self {
        budgeted_result(Vec::new(), HookSummary::default(), DEFAULT_TOKEN_BUDGET)
    }
}

/// A bounded diagnostic sample of provider-reported tool execution outcomes.
/// These counts never participate in procedural guidance.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct HookSummary {
    pub sampled_executions: usize,
    pub reported_successes: usize,
    pub reported_failures: usize,
    pub unknown: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceConfidence {
    Weak,
    Moderate,
    Strong,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PracticeGuidance {
    Prefer,
    Avoid,
    Mixed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RecallObservation {
    pub task: TaskKind,
    pub capability: Capability,
    pub operation: Operation,
    pub strategy: Strategy,
    pub attempts: usize,
    pub successes: usize,
    pub failures: usize,
    pub unknown: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub success_rate_percent: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<EvidenceConfidence>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guidance: Option<PracticeGuidance>,
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

    pub fn recall(
        &self,
        locator: &ProjectLocator,
        options: RecallOptions,
        task_query: Option<&str>,
    ) -> Result<RecallResult> {
        options.validate()?;
        let Some(project_id) = self.history.find_project(locator)? else {
            return Ok(budgeted_result(
                Vec::new(),
                HookSummary::default(),
                options.token_budget,
            ));
        };
        let practice_events = self.history.recent_project_events(
            project_id,
            EvidenceKind::LearnedPractice,
            MAX_CANDIDATE_EVENTS_PER_KIND,
        )?;
        let hook_events = self.history.recent_project_events(
            project_id,
            EvidenceKind::HookExecution,
            MAX_CANDIDATE_EVENTS_PER_KIND,
        )?;
        let intent = task_query.map(TaskIntent::classify).unwrap_or_default();
        let (observations, hook_summary) = aggregate(
            practice_events,
            hook_events,
            options,
            &intent,
            task_query.is_some(),
        );
        Ok(budgeted_result(
            observations,
            hook_summary,
            options.token_budget,
        ))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct GroupKey {
    task: TaskKind,
    capability: Capability,
    operation: Operation,
    strategy: Strategy,
}

struct Group {
    attempts: usize,
    successes: usize,
    failures: usize,
    unknown: usize,
    errors: BTreeMap<ErrorClass, usize>,
    latest: DateTime<Utc>,
}

fn aggregate(
    practice_events: Vec<HistoryEvent>,
    hook_events: Vec<HistoryEvent>,
    options: RecallOptions,
    intent: &TaskIntent,
    query_present: bool,
) -> (Vec<RecallObservation>, HookSummary) {
    let mut hook_summary = HookSummary::default();
    for event in hook_events {
        if options
            .operation
            .is_some_and(|wanted| event.operation != wanted)
        {
            continue;
        }
        hook_summary.sampled_executions += 1;
        match event.outcome {
            Outcome::Success => hook_summary.reported_successes += 1,
            Outcome::Failure => hook_summary.reported_failures += 1,
            Outcome::Unknown => hook_summary.unknown += 1,
        }
    }

    let mut groups = HashMap::<GroupKey, Group>::new();
    for event in practice_events {
        if options
            .operation
            .is_some_and(|wanted| event.operation != wanted)
            || (options.failures_only && event.outcome != Outcome::Failure)
        {
            continue;
        }
        let (Some(task), Some(strategy)) = (event.task, event.strategy) else {
            continue;
        };
        if query_present && !intent.matches_task(task) {
            continue;
        }
        let key = GroupKey {
            task,
            capability: event.capability,
            operation: event.operation,
            strategy,
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
            let score = group.successes.saturating_mul(4) + group.failures.saturating_mul(3);
            let relevance = intent.relevance(key.capability, key.operation);
            let known_outcomes = group.successes + group.failures;
            let confidence = evidence_confidence(known_outcomes);
            let guidance = practice_guidance(group.successes, group.failures);
            (
                relevance,
                guidance_priority(guidance),
                confidence_priority(confidence),
                score,
                group.latest,
                key,
                RecallObservation {
                    task: key.task,
                    capability: key.capability,
                    operation: key.operation,
                    strategy: key.strategy,
                    attempts: group.attempts,
                    successes: group.successes,
                    failures: group.failures,
                    unknown: group.unknown,
                    success_rate_percent: (known_outcomes >= 2).then(|| {
                        (group.successes.saturating_mul(100) + known_outcomes / 2) / known_outcomes
                    }),
                    confidence,
                    guidance,
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
            .then_with(|| right.2.cmp(&left.2))
            .then_with(|| right.3.cmp(&left.3))
            .then_with(|| right.4.cmp(&left.4))
            .then_with(|| left.5.task.cmp(&right.5.task))
            .then_with(|| left.5.capability.cmp(&right.5.capability))
            .then_with(|| left.5.operation.cmp(&right.5.operation))
            .then_with(|| left.5.strategy.cmp(&right.5.strategy))
    });
    let observations = ranked
        .into_iter()
        .take(options.limit)
        .map(|(_, _, _, _, _, _, observation)| observation)
        .collect();
    (observations, hook_summary)
}

fn evidence_confidence(known_outcomes: usize) -> Option<EvidenceConfidence> {
    match known_outcomes {
        0..=1 => None,
        2 => Some(EvidenceConfidence::Weak),
        3..=7 => Some(EvidenceConfidence::Moderate),
        _ => Some(EvidenceConfidence::Strong),
    }
}

fn practice_guidance(successes: usize, failures: usize) -> Option<PracticeGuidance> {
    let known_outcomes = successes + failures;
    if known_outcomes < 2 {
        None
    } else if successes.saturating_mul(4) >= known_outcomes.saturating_mul(3) {
        Some(PracticeGuidance::Prefer)
    } else if failures.saturating_mul(4) >= known_outcomes.saturating_mul(3) {
        Some(PracticeGuidance::Avoid)
    } else {
        Some(PracticeGuidance::Mixed)
    }
}

fn guidance_priority(guidance: Option<PracticeGuidance>) -> usize {
    match guidance {
        Some(PracticeGuidance::Prefer | PracticeGuidance::Avoid) => 2,
        Some(PracticeGuidance::Mixed) => 1,
        None => 0,
    }
}

fn confidence_priority(confidence: Option<EvidenceConfidence>) -> usize {
    match confidence {
        None => 0,
        Some(EvidenceConfidence::Weak) => 1,
        Some(EvidenceConfidence::Moderate) => 2,
        Some(EvidenceConfidence::Strong) => 3,
    }
}

fn budgeted_result(
    mut observations: Vec<RecallObservation>,
    hook_summary: HookSummary,
    token_budget: usize,
) -> RecallResult {
    loop {
        let mut result = RecallResult {
            observations,
            hook_summary,
            approximate_tokens: 0,
        };
        loop {
            let estimate = estimate_tokens(&result);
            if estimate == result.approximate_tokens {
                break;
            }
            result.approximate_tokens = estimate;
        }
        if result.approximate_tokens <= token_budget || result.observations.is_empty() {
            return result;
        }
        observations = result.observations;
        observations.pop();
    }
}

fn estimate_tokens(value: &impl Serialize) -> usize {
    let bytes = serde_json::to_vec_pretty(value)
        .expect("fixed-schema recall output must remain serializable")
        .len()
        + 1; // CLI newline
    bytes.div_ceil(4)
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
            evidence_kind: EvidenceKind,
            limit: usize,
        ) -> Result<Vec<HistoryEvent>> {
            self.requested_limit.set(limit);
            Ok(self
                .events
                .iter()
                .filter(|event| event.evidence_kind == evidence_kind)
                .take(limit)
                .cloned()
                .collect())
        }
    }

    fn event(id: u128, operation: Operation, outcome: Outcome) -> HistoryEvent {
        HistoryEvent {
            id: Uuid::from_u128(id),
            timestamp: Utc
                .timestamp_millis_opt(1_776_254_400_000 + id as i64)
                .unwrap(),
            session_id: None,
            project_id: Some(Uuid::from_u128(7)),
            agent: None,
            evidence_kind: EvidenceKind::LearnedPractice,
            task: Some(TaskKind::Testing),
            capability: Capability::Test,
            operation,
            strategy: Some(Strategy::NativeTool),
            outcome,
            duration_ms: None,
            error_class: None,
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
                    token_budget: DEFAULT_TOKEN_BUDGET,
                },
                None,
            )
            .unwrap();

        assert_eq!(history.requested_limit.get(), MAX_CANDIDATE_EVENTS_PER_KIND);
        assert_eq!(result.observations.len(), 1);
        assert_eq!(result.observations[0].attempts, 2);
        assert_eq!(result.observations[0].successes, 1);
        assert_eq!(result.observations[0].failures, 1);
        assert_eq!(result.observations[0].success_rate_percent, Some(50));
        assert_eq!(
            result.observations[0].confidence,
            Some(EvidenceConfidence::Weak)
        );
        assert_eq!(
            result.observations[0].guidance,
            Some(PracticeGuidance::Mixed)
        );
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
    fn hook_activity_is_separate_from_verified_practice() {
        let mut events = Vec::new();
        for id in 1..=20 {
            let mut unknown = event(id, Operation::Command, Outcome::Unknown);
            unknown.evidence_kind = EvidenceKind::HookExecution;
            unknown.task = None;
            unknown.strategy = None;
            unknown.session_id = Some("PRIVATE_SESSION".to_owned());
            unknown.agent = Some(AgentKind::Codex);
            events.push(unknown);
        }
        for id in 21..=22 {
            let mut verified = event(id, Operation::Command, Outcome::Success);
            verified.strategy = Some(Strategy::TargetedVerification);
            events.push(verified);
        }
        let history = MemoryHistory {
            project_id: Some(Uuid::from_u128(7)),
            events,
            requested_limit: Cell::new(0),
        };

        let result = RecallService::new(&history)
            .recall(
                &ProjectLocator::new(PathBuf::from("/private/project")),
                RecallOptions::default(),
                Some("test verification"),
            )
            .unwrap();

        assert_eq!(
            result.observations[0].strategy,
            Strategy::TargetedVerification
        );
        assert_eq!(result.observations[0].success_rate_percent, Some(100));
        assert_eq!(
            result.observations[0].confidence,
            Some(EvidenceConfidence::Weak)
        );
        assert_eq!(
            result.observations[0].guidance,
            Some(PracticeGuidance::Prefer)
        );
        assert_eq!(result.observations.len(), 1);
        assert_eq!(result.hook_summary.sampled_executions, 20);
        assert_eq!(result.hook_summary.unknown, 20);
    }

    #[test]
    fn research_practice_is_recalled_as_analysis_evidence() {
        let events = (1..=2)
            .map(|id| {
                let mut event = event(id, Operation::Analyze, Outcome::Success);
                event.capability = Capability::Research;
                event.task = Some(TaskKind::Research);
                event.strategy = Some(Strategy::ReproduceThenCompare);
                event
            })
            .collect();
        let history = MemoryHistory {
            project_id: Some(Uuid::from_u128(7)),
            events,
            requested_limit: Cell::new(0),
        };

        let result = RecallService::new(&history)
            .recall(
                &ProjectLocator::new(PathBuf::from("/private/project")),
                RecallOptions::default(),
                Some("research reproduce compare"),
            )
            .unwrap();

        assert_eq!(result.observations[0].capability, Capability::Research);
        assert_eq!(result.observations[0].operation, Operation::Analyze);
        assert_eq!(
            result.observations[0].strategy,
            Strategy::ReproduceThenCompare
        );
        assert_eq!(
            result.observations[0].guidance,
            Some(PracticeGuidance::Prefer)
        );
        assert!(
            serde_json::to_string(&result)
                .unwrap()
                .contains("reproduce_then_compare")
        );
    }

    #[test]
    fn guidance_requires_repetition_and_uses_known_outcomes_only() {
        assert_eq!(practice_guidance(1, 0), None);
        assert_eq!(practice_guidance(2, 0), Some(PracticeGuidance::Prefer));
        assert_eq!(practice_guidance(0, 2), Some(PracticeGuidance::Avoid));
        assert_eq!(practice_guidance(2, 2), Some(PracticeGuidance::Mixed));
        assert_eq!(evidence_confidence(0), None);
        assert_eq!(evidence_confidence(3), Some(EvidenceConfidence::Moderate));
        assert_eq!(evidence_confidence(8), Some(EvidenceConfidence::Strong));
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
                None,
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
                None,
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
                    token_budget: MAX_TOKEN_BUDGET,
                    ..RecallOptions::default()
                },
                None,
            )
            .unwrap();
        assert!(result.observations.len() <= MAX_RECALL_LIMIT);
        assert!(result.approximate_tokens <= MAX_TOKEN_BUDGET);
    }

    #[test]
    fn task_relevance_survives_a_tight_token_budget_without_leaking_query_text() {
        let mut events = Vec::new();
        for id in 1..=10 {
            let mut item = event(id, Operation::Command, Outcome::Success);
            item.task = Some(TaskKind::Documentation);
            events.push(item);
        }
        let mut patch = event(20, Operation::ApplyPatch, Outcome::Success);
        patch.task = Some(TaskKind::FeatureImplementation);
        events.push(patch);
        let history = MemoryHistory {
            project_id: Some(Uuid::from_u128(7)),
            events,
            requested_limit: Cell::new(0),
        };
        let result = RecallService::new(&history)
            .recall(
                &ProjectLocator::new(PathBuf::from("/private/project")),
                RecallOptions {
                    limit: 1,
                    token_budget: 180,
                    ..RecallOptions::default()
                },
                Some("feature patch SUPER_SECRET_TASK_TOKEN"),
            )
            .unwrap();

        assert_eq!(result.observations.len(), 1);
        assert_eq!(result.observations[0].operation, Operation::ApplyPatch);
        assert!(result.approximate_tokens <= 180);
        assert!(
            !serde_json::to_string(&result)
                .unwrap()
                .contains("SUPER_SECRET_TASK_TOKEN")
        );
    }

    #[test]
    fn rejects_oversized_token_budgets_before_querying_storage() {
        let history = MemoryHistory {
            project_id: Some(Uuid::nil()),
            events: Vec::new(),
            requested_limit: Cell::new(0),
        };
        let error = RecallService::new(&history)
            .recall(
                &ProjectLocator::new(PathBuf::from("/private/project")),
                RecallOptions {
                    token_budget: MAX_TOKEN_BUDGET + 1,
                    ..RecallOptions::default()
                },
                None,
            )
            .unwrap_err();
        assert!(error.to_string().contains("token budget"));
        assert_eq!(history.requested_limit.get(), 0);
    }

    #[test]
    fn data_import_query_excludes_unrelated_and_unrecognized_practices() {
        let mut import = event(1, Operation::Analyze, Outcome::Success);
        import.task = Some(TaskKind::DataImport);
        import.capability = Capability::Research;
        import.strategy = Some(Strategy::PerSubjectStreaming);
        let mut documentation = event(2, Operation::ApplyPatch, Outcome::Success);
        documentation.task = Some(TaskKind::Documentation);
        documentation.strategy = Some(Strategy::StructuredPatch);
        let history = MemoryHistory {
            project_id: Some(Uuid::from_u128(7)),
            events: vec![import, documentation],
            requested_limit: Cell::new(0),
        };

        let relevant = RecallService::new(&history)
            .recall(
                &ProjectLocator::new(PathBuf::from("/private/project")),
                RecallOptions::default(),
                Some("data import"),
            )
            .unwrap();
        let nonsense = RecallService::new(&history)
            .recall(
                &ProjectLocator::new(PathBuf::from("/private/project")),
                RecallOptions::default(),
                Some("zzz nonsense unrelated"),
            )
            .unwrap();

        assert_eq!(relevant.observations.len(), 1);
        assert_eq!(relevant.observations[0].task, TaskKind::DataImport);
        assert_eq!(
            relevant.observations[0].strategy,
            Strategy::PerSubjectStreaming
        );
        assert!(nonsense.observations.is_empty());
    }

    #[test]
    fn failures_filter_semantic_practices_not_successful_tool_executions() {
        let mut hook = event(1, Operation::ApplyPatch, Outcome::Success);
        hook.evidence_kind = EvidenceKind::HookExecution;
        hook.task = None;
        hook.strategy = Some(Strategy::DirectTextMutation);
        let mut failed_practice = event(2, Operation::ApplyPatch, Outcome::Failure);
        failed_practice.task = Some(TaskKind::Documentation);
        failed_practice.strategy = Some(Strategy::DirectTextMutation);
        let history = MemoryHistory {
            project_id: Some(Uuid::from_u128(7)),
            events: vec![hook, failed_practice],
            requested_limit: Cell::new(0),
        };

        let result = RecallService::new(&history)
            .recall(
                &ProjectLocator::new(PathBuf::from("/private/project")),
                RecallOptions {
                    failures_only: true,
                    ..RecallOptions::default()
                },
                Some("documentation"),
            )
            .unwrap();

        assert_eq!(result.observations.len(), 1);
        assert_eq!(result.observations[0].failures, 1);
        assert_eq!(result.observations[0].common_error, None);
        assert_eq!(result.hook_summary.reported_successes, 1);
        assert_eq!(result.hook_summary.reported_failures, 0);
    }
}
