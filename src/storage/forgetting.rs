use anyhow::{Context, Result, bail};
use rusqlite::{Connection, Transaction, TransactionBehavior};
use uuid::Uuid;

use crate::application::{ProjectHistoryEraser, ProjectLocator};

use super::{private::PrivateRecordKind, project::ProjectIdentity, sqlite::EncryptedStore};

impl ProjectHistoryEraser for EncryptedStore {
    fn count_project_events(&self, project: &ProjectLocator) -> Result<Option<u64>> {
        let Some(identity) = self.find_project_identity_in(&self.connection, project)? else {
            return Ok(None);
        };
        self.count_identity_events(&self.connection, &identity)
            .map(Some)
    }

    fn erase_project(&self, project: &ProjectLocator) -> Result<Option<u64>> {
        // IMMEDIATE serializes this deletion with append transactions before
        // the identity is loaded. The tombstone rejects a writer that resolved
        // the old identity before this transaction began.
        let transaction =
            Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)
                .context("could not begin project forgetting transaction")?;
        let Some(identity) = self.find_project_identity_in(&transaction, project)? else {
            transaction.rollback()?;
            return Ok(None);
        };
        let event_token = self.event_project_token(identity.project_id)?;

        transaction.execute(
            "INSERT OR IGNORE INTO forgotten_project_tokens (project_token) VALUES (?1)",
            [event_token.as_slice()],
        )?;
        let deleted = transaction.execute(
            "DELETE FROM events WHERE project_token = ?1",
            [event_token.as_slice()],
        )?;
        let deleted_experiences = transaction.execute(
            "DELETE FROM experiences WHERE origin_token = ?1",
            [event_token.as_slice()],
        )?;
        let deleted_mapping = transaction.execute(
            "DELETE FROM private_projects WHERE lookup_token = ?1",
            [identity.lookup_token.as_slice()],
        )?;
        if deleted_mapping != 1 {
            bail!("encrypted project identity changed during deletion");
        }
        transaction.commit()?;
        u64::try_from(deleted + deleted_experiences)
            .context("project deletion returned an invalid event count")
            .map(Some)
    }
}

impl EncryptedStore {
    fn count_identity_events(
        &self,
        connection: &Connection,
        identity: &ProjectIdentity,
    ) -> Result<u64> {
        let event_token = self.event_project_token(identity.project_id)?;
        let count: i64 = connection.query_row(
            "SELECT (SELECT COUNT(*) FROM events WHERE project_token = ?1)
                  + (SELECT COUNT(*) FROM experiences WHERE origin_token = ?1)",
            [event_token.as_slice()],
            |row| row.get(0),
        )?;
        count
            .try_into()
            .context("history database returned an invalid project event count")
    }

    fn event_project_token(&self, project_id: Uuid) -> Result<[u8; 32]> {
        self.private_cipher
            .lookup_token(PrivateRecordKind::EventProject, project_id.as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use chrono::{TimeZone, Utc};
    use tempfile::tempdir;

    use crate::{
        application::{CursorStore, IngestionCursor, ProjectResolver},
        core::{
            AgentKind, CURRENT_SCHEMA_VERSION, Capability, EvidenceKind, HistoryEvent, Operation,
            Outcome,
        },
        query::ProjectLookup,
        storage::MasterKey,
    };

    use super::*;

    fn event(id: u128, project_id: Uuid) -> HistoryEvent {
        HistoryEvent {
            id: Uuid::from_u128(id),
            timestamp: Utc.timestamp_millis_opt(1_776_254_400_123).unwrap(),
            session_id: Some("PRIVATE_SESSION".to_owned()),
            project_id: Some(project_id),
            agent: Some(AgentKind::Claude),
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
    fn deletion_is_project_scoped_and_tombstones_the_old_identity() {
        let temp = tempdir().unwrap();
        let wanted_path = temp.path().join("PRIVATE_WANTED_PROJECT");
        let other_path = temp.path().join("PRIVATE_OTHER_PROJECT");
        fs::create_dir(&wanted_path).unwrap();
        fs::create_dir(&other_path).unwrap();
        let store = EncryptedStore::open_in_memory(&MasterKey::from_bytes([71; 32])).unwrap();
        let wanted = ProjectLocator::new(wanted_path);
        let other = ProjectLocator::new(other_path);
        let wanted_id = store.resolve_project(&wanted).unwrap();
        let other_id = store.resolve_project(&other).unwrap();
        store
            .save_cursor("PRIVATE_CURSOR_SESSION", &IngestionCursor::empty())
            .unwrap();

        store.append(&event(1, wanted_id)).unwrap();
        store.append(&event(2, wanted_id)).unwrap();
        store.append(&event(3, other_id)).unwrap();
        assert_eq!(store.count_project_events(&wanted).unwrap(), Some(2));
        assert_eq!(store.erase_project(&wanted).unwrap(), Some(2));
        assert_eq!(store.count().unwrap(), 1);
        assert_eq!(store.find_project(&wanted).unwrap(), None);
        assert_eq!(store.count_project_events(&wanted).unwrap(), None);
        assert_eq!(store.find_project(&other).unwrap(), Some(other_id));
        assert_eq!(
            store.load_cursor("PRIVATE_CURSOR_SESSION").unwrap(),
            Some(IngestionCursor::empty())
        );

        let tombstone: Vec<u8> = store
            .connection
            .query_row(
                "SELECT project_token FROM forgotten_project_tokens",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_ne!(tombstone, wanted_id.as_bytes());

        // A hook that resolved the old ID before deletion cannot recreate an
        // orphaned event after the transaction commits.
        assert!(!store.append(&event(4, wanted_id)).unwrap());
        assert_eq!(store.count().unwrap(), 1);
        let replacement_id = store.resolve_project(&wanted).unwrap();
        assert_ne!(replacement_id, wanted_id);
        assert!(store.append(&event(5, replacement_id)).unwrap());
    }

    #[test]
    fn forgetting_removes_capsules_recorded_from_the_project_in_every_scope() {
        use crate::{
            application::{ExperienceStore, ScopeKey},
            core::{
                ApplicabilityTags, EXPERIENCE_SCHEMA_VERSION, EnvironmentFingerprint,
                EvidenceEntry, ExperienceCapsule, MemoryLifecycle, MemoryScope, MutationMode,
                Procedure, SemanticOutcome, TaskKind,
            },
        };
        let temp = tempdir().unwrap();
        let wanted_path = temp.path().join("WANTED");
        let other_path = temp.path().join("OTHER");
        fs::create_dir(&wanted_path).unwrap();
        fs::create_dir(&other_path).unwrap();
        let store = EncryptedStore::open_in_memory(&MasterKey::from_bytes([72; 32])).unwrap();
        let wanted = ProjectLocator::new(wanted_path);
        let wanted_id = store.resolve_project(&wanted).unwrap();
        let other_id = store
            .resolve_project(&ProjectLocator::new(other_path))
            .unwrap();
        let capsule = |id: u128, project: Uuid, scope: MemoryScope| {
            let at = Utc.timestamp_millis_opt(1_776_254_400_000).unwrap();
            ExperienceCapsule {
                id: Uuid::from_u128(id),
                project_id: project,
                scope,
                scope_id: (scope == MemoryScope::Project).then_some(project),
                task: TaskKind::Testing,
                situation: None,
                lesson: None,
                caveat: None,
                procedure: Procedure {
                    mutation: Some(MutationMode::StructuredPatch),
                    ..Procedure::default()
                },
                applicability: ApplicabilityTags::default(),
                lifecycle: MemoryLifecycle::Active,
                challenge: None,
                evidence: vec![EvidenceEntry::agent_reported(
                    at,
                    SemanticOutcome::Success,
                    None,
                    EnvironmentFingerprint::default(),
                )],
                created_at: at,
                last_confirmed_at: at,
                schema_version: EXPERIENCE_SCHEMA_VERSION,
            }
        };
        store
            .append_experience(&capsule(1, wanted_id, MemoryScope::Project))
            .unwrap();
        store
            .append_experience(&capsule(2, wanted_id, MemoryScope::Global))
            .unwrap();
        store
            .append_experience(&capsule(3, other_id, MemoryScope::Global))
            .unwrap();
        store.append(&event(4, wanted_id)).unwrap();

        assert_eq!(store.count_project_events(&wanted).unwrap(), Some(3));
        assert_eq!(store.erase_project(&wanted).unwrap(), Some(3));
        let globals = store.scoped_experiences(ScopeKey::Global, 10).unwrap();
        assert_eq!(globals.len(), 1);
        assert_eq!(globals[0].project_id, other_id);
        assert!(
            !store
                .append_experience(&capsule(5, wanted_id, MemoryScope::Global))
                .unwrap()
        );
    }
}
