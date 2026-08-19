use anyhow::{Context, Result};
use serde::Serialize;

use crate::core::HistoryEvent;

use super::{EventSink, ProjectLocator, ProjectResolver};

/// The complete value an agent hook may pass into application code. The event
/// is controlled and serializable; the project locator is intentionally not.
pub struct HookObservation {
    event: HistoryEvent,
    project: ProjectLocator,
}

impl HookObservation {
    pub fn new(event: HistoryEvent, project: ProjectLocator) -> Self {
        Self { event, project }
    }

    pub fn event(&self) -> &HistoryEvent {
        &self.event
    }

    pub fn project(&self) -> &ProjectLocator {
        &self.project
    }

    fn into_parts(self) -> (HistoryEvent, ProjectLocator) {
        (self.event, self.project)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct RecordingReport {
    pub inserted: usize,
    pub already_present: usize,
}

/// Records one already-sanitized native hook observation without loading or
/// advancing a transcript cursor.
pub struct RecordingService<H> {
    history: H,
}

impl<H> RecordingService<H>
where
    H: EventSink + ProjectResolver,
{
    pub fn new(history: H) -> Self {
        Self { history }
    }

    pub fn record(&self, observation: HookObservation) -> Result<RecordingReport> {
        let (mut event, project) = observation.into_parts();
        let project_id = self
            .history
            .resolve_project(&project)
            .context("could not resolve the encrypted project identity")?;
        event.project_id = Some(project_id);
        let inserted = self
            .history
            .append(&event)
            .context("could not persist a sanitized hook event")?;
        Ok(RecordingReport {
            inserted: usize::from(inserted),
            already_present: usize::from(!inserted),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, collections::HashMap, path::PathBuf};

    use anyhow::bail;
    use chrono::{TimeZone, Utc};
    use uuid::Uuid;

    use crate::core::{AgentKind, CURRENT_SCHEMA_VERSION, Capability, Operation, Outcome};

    use super::*;

    #[derive(Default)]
    struct MemoryHistory {
        events: RefCell<HashMap<Uuid, Option<Uuid>>>,
    }

    impl EventSink for MemoryHistory {
        fn append(&self, event: &HistoryEvent) -> Result<bool> {
            let mut events = self.events.borrow_mut();
            if events.contains_key(&event.id) {
                return Ok(false);
            }
            events.insert(event.id, event.project_id);
            Ok(true)
        }
    }

    impl ProjectResolver for MemoryHistory {
        fn resolve_project(&self, _locator: &ProjectLocator) -> Result<Uuid> {
            Ok(Uuid::from_u128(0x50524f4a454354))
        }
    }

    struct FailingHistory;

    impl EventSink for FailingHistory {
        fn append(&self, _event: &HistoryEvent) -> Result<bool> {
            bail!("sentinel storage failure")
        }
    }

    impl ProjectResolver for FailingHistory {
        fn resolve_project(&self, _locator: &ProjectLocator) -> Result<Uuid> {
            Ok(Uuid::nil())
        }
    }

    fn observation() -> HookObservation {
        HookObservation::new(
            HistoryEvent {
                id: Uuid::from_u128(7),
                timestamp: Utc.timestamp_millis_opt(1_776_254_400_123).unwrap(),
                session_id: Some("PRIVATE_SESSION".to_owned()),
                project_id: None,
                agent: Some(AgentKind::Claude),
                capability: Capability::Test,
                operation: Operation::Command,
                strategy: None,
                outcome: Outcome::Success,
                duration_ms: Some(12),
                error_class: None,
                schema_version: CURRENT_SCHEMA_VERSION,
            },
            ProjectLocator::new(PathBuf::from("/private/project")),
        )
    }

    #[test]
    fn records_one_event_without_exposing_identifiers() {
        let service = RecordingService::new(MemoryHistory::default());

        let first = service.record(observation()).unwrap();
        let second = service.record(observation()).unwrap();

        assert_eq!(
            first,
            RecordingReport {
                inserted: 1,
                already_present: 0
            }
        );
        assert_eq!(
            second,
            RecordingReport {
                inserted: 0,
                already_present: 1
            }
        );
        assert!(
            service
                .history
                .events
                .borrow()
                .values()
                .all(Option::is_some)
        );
        let encoded = serde_json::to_string(&first).unwrap();
        assert!(!encoded.contains("PRIVATE_SESSION"));
        assert!(!encoded.contains("private/project"));
    }

    #[test]
    fn adds_safe_context_to_recording_failures() {
        let error = RecordingService::new(FailingHistory)
            .record(observation())
            .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("could not persist a sanitized hook event")
        );
    }
}
