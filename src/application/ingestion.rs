use anyhow::{Context, Result};
use serde::Serialize;
use uuid::Uuid;

use crate::core::HistoryEvent;

use super::{IngestionCursor, ProjectLocator};

pub struct EventBatch {
    pub events: Vec<HistoryEvent>,
    pub next_cursor: IngestionCursor,
    pub project: ProjectLocator,
    pub source_reset: bool,
}

/// Loads already-sanitized events for one opaque source-session identifier.
/// Implementations own all knowledge of transcript and host formats.
pub trait SessionEventSource {
    fn events_for_session(
        &self,
        session_id: &str,
        cursor: Option<&IngestionCursor>,
    ) -> Result<EventBatch>;
}

/// Minimal persistence port needed by ingestion.
pub trait EventSink {
    /// Returns `true` when the event was newly inserted and `false` when its
    /// stable identifier was already present.
    fn append(&self, event: &HistoryEvent) -> Result<bool>;
}

pub trait CursorStore {
    fn load_cursor(&self, session_id: &str) -> Result<Option<IngestionCursor>>;
    fn save_cursor(&self, session_id: &str, cursor: &IngestionCursor) -> Result<()>;
}

pub trait ProjectResolver {
    fn resolve_project(&self, locator: &ProjectLocator) -> Result<Uuid>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct IngestionReport {
    pub observed: usize,
    pub inserted: usize,
    pub already_present: usize,
    pub source_reset: bool,
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
    H: EventSink + CursorStore + ProjectResolver,
{
    pub fn new(source: S, history: H) -> Self {
        Self { source, history }
    }

    pub fn ingest_session(&self, session_id: &str) -> Result<IngestionReport> {
        let cursor = self
            .history
            .load_cursor(session_id)
            .context("could not load the encrypted ingestion cursor")?;
        let batch = self
            .source
            .events_for_session(session_id, cursor.as_ref())
            .context("could not load sanitized session events")?;
        let project_id = self
            .history
            .resolve_project(&batch.project)
            .context("could not resolve the encrypted project identity")?;
        let observed = batch.events.len();
        let mut inserted = 0;

        for mut event in batch.events {
            event.project_id = Some(project_id);
            if self
                .history
                .append(&event)
                .context("could not persist a sanitized history event")?
            {
                inserted += 1;
            }
        }

        self.history
            .save_cursor(session_id, &batch.next_cursor)
            .context("could not save the encrypted ingestion cursor")?;

        Ok(IngestionReport {
            observed,
            inserted,
            already_present: observed - inserted,
            source_reset: batch.source_reset,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::{
        cell::{Cell, RefCell},
        collections::HashMap,
        path::PathBuf,
        rc::Rc,
    };

    use anyhow::bail;
    use chrono::{TimeZone, Utc};
    use uuid::Uuid;

    use crate::core::{
        AgentKind, CURRENT_SCHEMA_VERSION, Capability, EvidenceKind, HistoryEvent, Operation,
        Outcome,
    };

    use super::*;

    struct StubSource(Vec<HistoryEvent>);

    impl SessionEventSource for StubSource {
        fn events_for_session(
            &self,
            _session_id: &str,
            _cursor: Option<&IngestionCursor>,
        ) -> Result<EventBatch> {
            Ok(EventBatch {
                events: self.0.clone(),
                next_cursor: IngestionCursor::empty(),
                project: ProjectLocator::new(PathBuf::from("/private/project")),
                source_reset: false,
            })
        }
    }

    #[derive(Default)]
    struct MemorySink {
        events: RefCell<HashMap<Uuid, Option<Uuid>>>,
        cursors: RefCell<HashMap<String, IngestionCursor>>,
    }

    impl EventSink for MemorySink {
        fn append(&self, event: &HistoryEvent) -> Result<bool> {
            let mut events = self.events.borrow_mut();
            if events.contains_key(&event.id) {
                return Ok(false);
            }
            events.insert(event.id, event.project_id);
            Ok(true)
        }
    }

    impl CursorStore for MemorySink {
        fn load_cursor(&self, session_id: &str) -> Result<Option<IngestionCursor>> {
            Ok(self.cursors.borrow().get(session_id).cloned())
        }

        fn save_cursor(&self, session_id: &str, cursor: &IngestionCursor) -> Result<()> {
            self.cursors
                .borrow_mut()
                .insert(session_id.to_owned(), cursor.clone());
            Ok(())
        }
    }

    impl ProjectResolver for MemorySink {
        fn resolve_project(&self, _locator: &ProjectLocator) -> Result<Uuid> {
            Ok(Uuid::from_u128(0x50524f4a454354))
        }
    }

    struct FailingSink {
        cursor_saved: Rc<Cell<bool>>,
    }

    impl EventSink for FailingSink {
        fn append(&self, _event: &HistoryEvent) -> Result<bool> {
            bail!("sentinel storage failure")
        }
    }

    impl CursorStore for FailingSink {
        fn load_cursor(&self, _session_id: &str) -> Result<Option<IngestionCursor>> {
            Ok(None)
        }

        fn save_cursor(&self, _session_id: &str, _cursor: &IngestionCursor) -> Result<()> {
            self.cursor_saved.set(true);
            Ok(())
        }
    }

    impl ProjectResolver for FailingSink {
        fn resolve_project(&self, _locator: &ProjectLocator) -> Result<Uuid> {
            Ok(Uuid::nil())
        }
    }

    fn event(id: u128) -> HistoryEvent {
        HistoryEvent {
            id: Uuid::from_u128(id),
            timestamp: Utc.timestamp_millis_opt(1_776_254_400_123).unwrap(),
            session_id: Some("PRIVATE_SESSION".to_owned()),
            project_id: None,
            agent: Some(AgentKind::Codex),
            evidence_kind: EvidenceKind::HookExecution,
            task: None,
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
                source_reset: false,
            }
        );
        let encoded = serde_json::to_string(&report).unwrap();
        assert!(!encoded.contains("PRIVATE_SESSION"));
        assert!(
            service
                .history
                .events
                .borrow()
                .values()
                .all(Option::is_some)
        );
    }

    #[test]
    fn adds_safe_context_to_sink_failures() {
        let cursor_saved = Rc::new(Cell::new(false));
        let service = IngestionService::new(
            StubSource(vec![event(1)]),
            FailingSink {
                cursor_saved: Rc::clone(&cursor_saved),
            },
        );
        let error = service.ingest_session("session-1").unwrap_err();
        assert!(
            error
                .to_string()
                .contains("could not persist a sanitized history event")
        );
        assert!(
            !cursor_saved.get(),
            "cursor advanced after an event failure"
        );
    }
}
