use std::{fs, io::ErrorKind, path::PathBuf};

use anyhow::{Context, Result, bail};
use rusqlite::{Connection, OpenFlags};

use crate::{
    application::{HistoryCounts, HistoryReadinessProbe},
    core::EvidenceKind,
};

/// Read-only inspection adapter for an existing Regurgitate history database.
pub struct HistoryDatabaseProbe {
    path: PathBuf,
}

impl HistoryDatabaseProbe {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

impl HistoryReadinessProbe for HistoryDatabaseProbe {
    fn history_counts(&self) -> Result<Option<HistoryCounts>> {
        match fs::metadata(&self.path) {
            Ok(metadata) if metadata.is_file() => {}
            Ok(_) => bail!("history database is not a regular file"),
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error).context("could not inspect history database"),
        }

        let connection = Connection::open_with_flags(
            &self.path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .context("could not open history database read-only")?;
        let integrity: String =
            connection.query_row("PRAGMA quick_check(1)", [], |row| row.get(0))?;
        if integrity != "ok" {
            bail!("history database failed its integrity check");
        }
        let (events, hooks, practices): (i64, i64, i64) = connection.query_row(
            "SELECT
                COUNT(*),
                COALESCE(SUM(evidence_kind = ?1), 0),
                COALESCE(SUM(evidence_kind = ?2), 0)
             FROM events",
            [
                i64::from(EvidenceKind::HookExecution.storage_code()),
                i64::from(EvidenceKind::LearnedPractice.storage_code()),
            ],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
        Ok(Some(HistoryCounts {
            event_count: events
                .try_into()
                .context("history database returned an invalid event count")?,
            hook_event_count: hooks
                .try_into()
                .context("history database returned an invalid hook event count")?,
            learned_practice_count: practices
                .try_into()
                .context("history database returned an invalid learned-practice count")?,
        }))
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use chrono::Utc;
    use tempfile::tempdir;
    use uuid::Uuid;

    use crate::{
        core::{
            AgentKind, CURRENT_SCHEMA_VERSION, Capability, EvidenceKind, HistoryEvent, Operation,
            Outcome, Strategy, TaskKind,
        },
        storage::{EncryptedStore, MasterKey},
    };

    use super::*;

    fn event(id: u128, evidence_kind: EvidenceKind) -> HistoryEvent {
        let learned = evidence_kind == EvidenceKind::LearnedPractice;
        HistoryEvent {
            id: Uuid::from_u128(id),
            timestamp: Utc::now(),
            session_id: (!learned).then(|| "PRIVATE_SESSION".to_owned()),
            project_id: Some(Uuid::from_u128(7)),
            agent: (!learned).then_some(AgentKind::Codex),
            evidence_kind,
            task: learned.then_some(TaskKind::Testing),
            capability: Capability::Test,
            operation: Operation::Command,
            strategy: learned.then_some(Strategy::TargetedVerification),
            outcome: Outcome::Success,
            duration_ms: None,
            error_class: None,
            schema_version: CURRENT_SCHEMA_VERSION,
        }
    }

    #[test]
    fn missing_history_is_reported_without_creating_anything() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("missing/history.db");
        let probe = HistoryDatabaseProbe::new(path.clone());
        assert_eq!(probe.history_counts().unwrap(), None);
        assert!(!path.exists());
        assert!(!path.parent().unwrap().exists());
    }

    #[test]
    fn existing_history_is_counted_read_only() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("history.db");
        let key = MasterKey::from_bytes([61; 32]);
        let store = EncryptedStore::open(&path, &key).unwrap();
        assert_eq!(store.count().unwrap(), 0);
        drop(store);

        let before = fs::metadata(&path).unwrap().modified().unwrap();
        assert_eq!(
            HistoryDatabaseProbe::new(path.clone())
                .history_counts()
                .unwrap(),
            Some(HistoryCounts {
                event_count: 0,
                hook_event_count: 0,
                learned_practice_count: 0,
            })
        );
        let after = fs::metadata(path).unwrap().modified().unwrap();
        assert_eq!(before, after);
    }

    #[test]
    fn history_counts_separate_hooks_from_learned_practice() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("history.db");
        let store = EncryptedStore::open(&path, &MasterKey::from_bytes([62; 32])).unwrap();
        store
            .append(&event(1, EvidenceKind::HookExecution))
            .unwrap();
        store
            .append(&event(2, EvidenceKind::LearnedPractice))
            .unwrap();
        drop(store);

        assert_eq!(
            HistoryDatabaseProbe::new(path).history_counts().unwrap(),
            Some(HistoryCounts {
                event_count: 2,
                hook_event_count: 1,
                learned_practice_count: 1,
            })
        );
    }

    #[test]
    fn damaged_history_is_unavailable_instead_of_being_repaired() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("history.db");
        fs::write(&path, b"PRIVATE_CORRUPTED_DATABASE").unwrap();

        assert!(
            HistoryDatabaseProbe::new(path.clone())
                .history_counts()
                .is_err()
        );
        assert_eq!(fs::read(path).unwrap(), b"PRIVATE_CORRUPTED_DATABASE");
    }
}
