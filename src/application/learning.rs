use anyhow::{Result, bail};
use chrono::Utc;
use serde::Serialize;
use uuid::Uuid;

use crate::core::{
    CURRENT_SCHEMA_VERSION, EvidenceKind, HistoryEvent, Outcome, Strategy, TaskKind,
};

use super::{EventSink, HookObservation, ProjectLocator, ProjectResolver, RecordingService};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LearningStatus {
    Recorded,
    AlreadyPresent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct LearningReport {
    pub status: LearningStatus,
}

/// Records one explicit, controlled practice outcome. This is the fallback for
/// provider contracts that cannot reliably expose semantic success or failure.
pub struct LearningService<H> {
    recording: RecordingService<H>,
}

impl<H> LearningService<H>
where
    H: EventSink + ProjectResolver,
{
    pub fn new(history: H) -> Self {
        Self {
            recording: RecordingService::new(history),
        }
    }

    pub fn learn(
        &self,
        project: ProjectLocator,
        task: TaskKind,
        strategy: Strategy,
        outcome: Outcome,
    ) -> Result<LearningReport> {
        if outcome == Outcome::Unknown {
            bail!("an explicitly learned practice requires a known outcome");
        }
        let (capability, operation) = strategy.practice_classification();
        let observation = HookObservation::new(
            HistoryEvent {
                id: Uuid::new_v4(),
                timestamp: Utc::now(),
                session_id: None,
                project_id: None,
                agent: None,
                evidence_kind: EvidenceKind::LearnedPractice,
                task: Some(task),
                capability,
                operation,
                strategy: Some(strategy),
                outcome,
                duration_ms: None,
                error_class: None,
                schema_version: CURRENT_SCHEMA_VERSION,
            },
            project,
        );
        let report = self.recording.record(observation)?;
        Ok(LearningReport {
            status: if report.inserted == 1 {
                LearningStatus::Recorded
            } else {
                LearningStatus::AlreadyPresent
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, path::PathBuf, rc::Rc};

    use uuid::Uuid;

    use crate::core::{Capability, Operation};

    use super::*;

    #[derive(Default)]
    struct MemoryHistory {
        events: RefCell<Vec<HistoryEvent>>,
    }

    impl EventSink for Rc<MemoryHistory> {
        fn append(&self, event: &HistoryEvent) -> Result<bool> {
            self.events.borrow_mut().push(event.clone());
            Ok(true)
        }
    }

    impl ProjectResolver for Rc<MemoryHistory> {
        fn resolve_project(&self, _locator: &ProjectLocator) -> Result<Uuid> {
            Ok(Uuid::from_u128(7))
        }
    }

    #[test]
    fn records_only_controlled_practice_fields() {
        let history = Rc::new(MemoryHistory::default());
        let report = LearningService::new(Rc::clone(&history))
            .learn(
                ProjectLocator::new(PathBuf::from("/private/SECRET_PROJECT")),
                TaskKind::Configuration,
                Strategy::AtomicWrite,
                Outcome::Success,
            )
            .unwrap();

        assert_eq!(report.status, LearningStatus::Recorded);
        let events = history.events.borrow();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].capability, Capability::Filesystem);
        assert_eq!(events[0].evidence_kind, EvidenceKind::LearnedPractice);
        assert_eq!(events[0].task, Some(TaskKind::Configuration));
        assert_eq!(events[0].operation, Operation::WriteFile);
        assert_eq!(events[0].strategy, Some(Strategy::AtomicWrite));
        assert_eq!(events[0].outcome, Outcome::Success);
        assert!(events[0].session_id.is_none());
        assert!(events[0].agent.is_none());
        assert!(
            !serde_json::to_string(&events[0])
                .unwrap()
                .contains("SECRET_PROJECT")
        );
    }

    #[test]
    fn rejects_unknown_outcomes_before_storage() {
        let history = Rc::new(MemoryHistory::default());
        let error = LearningService::new(Rc::clone(&history))
            .learn(
                ProjectLocator::new(PathBuf::from("/private/project")),
                TaskKind::Testing,
                Strategy::StructuredPatch,
                Outcome::Unknown,
            )
            .unwrap_err();

        assert!(error.to_string().contains("known outcome"));
        assert!(history.events.borrow().is_empty());
    }

    #[test]
    fn semantic_failure_is_not_misrepresented_as_a_provider_error() {
        let history = Rc::new(MemoryHistory::default());
        LearningService::new(Rc::clone(&history))
            .learn(
                ProjectLocator::new(PathBuf::from("/private/project")),
                TaskKind::Documentation,
                Strategy::DirectTextMutation,
                Outcome::Failure,
            )
            .unwrap();

        let events = history.events.borrow();
        assert_eq!(events[0].outcome, Outcome::Failure);
        assert_eq!(events[0].error_class, None);
    }
}
