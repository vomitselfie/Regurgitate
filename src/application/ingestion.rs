use anyhow::{Context, Result};
use serde::Serialize;

use crate::core::HistoryEvent;

/// Loads already-sanitized events for one opaque source-session identifier.
/// Implementations own all knowledge of transcript and host formats.
pub trait SessionEventSource {
    fn events_for_session(&self, session_id: &str) -> Result<Vec<HistoryEvent>>;
}

/// Minimal persistence port needed by ingestion.
pub trait EventSink {
    /// Returns `true` when the event was newly inserted and `false` when its
    /// stable identifier was already present.
    fn append(&self, event: &HistoryEvent) -> Result<bool>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct IngestionReport {
    pub observed: usize,
    pub inserted: usize,
    pub already_present: usize,
}

/// Coordinates a source and sink without depending on AoE, Codex, SQLite, or
/// credential-store details.
pub struct IngestionService<S, H> {
    source: S,
    history: H,
}

impl<S, H> IngestionService<S, H>
where
    S: SessionEventSource,
    H: EventSink,
{
    pub fn new(source: S, history: H) -> Self {
        Self { source, history }
    }

    pub fn ingest_session(&self, session_id: &str) -> Result<IngestionReport> {
        let events = self
            .source
            .events_for_session(session_id)
            .context("could not load sanitized session events")?;
        let observed = events.len();
        let mut inserted = 0;

        for event in &events {
            if self
                .history
                .append(event)
                .context("could not persist a sanitized history event")?
            {
                inserted += 1;
            }
        }

        Ok(IngestionReport {
            observed,
            inserted,
            already_present: observed - inserted,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, collections::HashSet};

    use anyhow::bail;
    use chrono::{TimeZone, Utc};
    use uuid::Uuid;

    use crate::core::{
        AgentKind, CURRENT_SCHEMA_VERSION, Capability, HistoryEvent, Operation, Outcome,
    };

    use super::*;

    struct StubSource(Vec<HistoryEvent>);

    impl SessionEventSource for StubSource {
        fn events_for_session(&self, _session_id: &str) -> Result<Vec<HistoryEvent>> {
            Ok(self.0.clone())
        }
    }

    #[derive(Default)]
    struct MemorySink(RefCell<HashSet<Uuid>>);

    impl EventSink for MemorySink {
        fn append(&self, event: &HistoryEvent) -> Result<bool> {
            Ok(self.0.borrow_mut().insert(event.id))
        }
    }

    struct FailingSink;

    impl EventSink for FailingSink {
        fn append(&self, _event: &HistoryEvent) -> Result<bool> {
            bail!("sentinel storage failure")
        }
    }

    fn event(id: u128) -> HistoryEvent {
        HistoryEvent {
            id: Uuid::from_u128(id),
            timestamp: Utc.timestamp_millis_opt(1_776_254_400_123).unwrap(),
            session_id: Some("PRIVATE_SESSION".to_owned()),
            project_id: None,
            agent: Some(AgentKind::Codex),
            capability: Capability::Test,
            operation: Operation::Command,
            strategy: None,
            outcome: Outcome::Success,
            duration_ms: None,
            error_class: None,
            schema_version: CURRENT_SCHEMA_VERSION,
        }
    }

    #[test]
    fn reports_new_and_preexisting_events_without_identifiers() {
        let source = StubSource(vec![event(1), event(2), event(1)]);
        let service = IngestionService::new(source, MemorySink::default());

        let report = service.ingest_session("PRIVATE_SESSION").unwrap();

        assert_eq!(
            report,
            IngestionReport {
                observed: 3,
                inserted: 2,
                already_present: 1,
            }
        );
        let encoded = serde_json::to_string(&report).unwrap();
        assert!(!encoded.contains("PRIVATE_SESSION"));
    }

    #[test]
    fn adds_safe_context_to_sink_failures() {
        let service = IngestionService::new(StubSource(vec![event(1)]), FailingSink);
        let error = service.ingest_session("session-1").unwrap_err();
        assert!(
            error
                .to_string()
                .contains("could not persist a sanitized history event")
        );
    }
}
