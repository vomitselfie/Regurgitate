use anyhow::Result;
use rusqlite::params;
use uuid::Uuid;

use crate::{core::HistoryEvent, query::ProjectEventSource};

use super::{
    private::PrivateRecordKind,
    sqlite::{EncryptedStore, StoredEvent},
};

impl ProjectEventSource for EncryptedStore {
    fn recent_project_events(&self, project_id: Uuid, limit: usize) -> Result<Vec<HistoryEvent>> {
        let token = self
            .private_cipher
            .lookup_token(PrivateRecordKind::EventProject, project_id.as_bytes())?;
        let mut stored = Vec::new();
        {
            let mut statement = self.connection.prepare(
                "SELECT id, created_at_ms, schema_version, envelope_version, nonce, ciphertext
                 FROM events
                 WHERE project_token = ?1
                 ORDER BY created_at_ms DESC
                 LIMIT ?2",
            )?;
            let rows = statement.query_map(
                params![token.as_slice(), i64::try_from(limit)?],
                stored_event_from_row,
            )?;
            for row in rows {
                stored.push(row?);
            }
        }

        // Rows written before project-token indexing are considered during the
        // migration window. Their encrypted payload determines project match.
        if stored.len() < limit {
            let mut statement = self.connection.prepare(
                "SELECT id, created_at_ms, schema_version, envelope_version, nonce, ciphertext
                 FROM events
                 WHERE project_token IS NULL
                 ORDER BY created_at_ms DESC",
            )?;
            let rows = statement.query_map([], stored_event_from_row)?;
            for row in rows {
                stored.push(row?);
            }
        }

        let mut events = Vec::new();
        for row in stored {
            let event = self.decrypt_row(row)?;
            if event.project_id == Some(project_id) {
                events.push(event);
            }
        }
        events.sort_by_key(|event| std::cmp::Reverse(event.timestamp));
        events.truncate(limit);
        Ok(events)
    }
}

pub(super) fn stored_event_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredEvent> {
    Ok(StoredEvent {
        id: row.get(0)?,
        created_at_ms: row.get(1)?,
        schema_version: row.get(2)?,
        envelope_version: row.get(3)?,
        nonce: row.get(4)?,
        ciphertext: row.get(5)?,
    })
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};

    use crate::{
        core::{AgentKind, CURRENT_SCHEMA_VERSION, Capability, Operation, Outcome},
        storage::MasterKey,
    };

    use super::*;

    fn event(id: u128, project_id: Uuid, timestamp: i64) -> HistoryEvent {
        HistoryEvent {
            id: Uuid::from_u128(id),
            timestamp: Utc.timestamp_millis_opt(timestamp).unwrap(),
            session_id: Some("private-session".to_owned()),
            project_id: Some(project_id),
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
    fn query_is_project_scoped_recent_first_and_bounded() {
        let store = EncryptedStore::open_in_memory(&MasterKey::from_bytes([41; 32])).unwrap();
        let wanted = Uuid::from_u128(1);
        let other = Uuid::from_u128(2);
        store.append(&event(1, wanted, 100)).unwrap();
        store.append(&event(2, other, 300)).unwrap();
        store.append(&event(3, wanted, 200)).unwrap();

        let events = store.recent_project_events(wanted, 1).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].id, Uuid::from_u128(3));
        assert!(events.iter().all(|event| event.project_id == Some(wanted)));
    }

    #[test]
    fn event_project_token_does_not_contain_the_project_uuid() {
        let store = EncryptedStore::open_in_memory(&MasterKey::from_bytes([42; 32])).unwrap();
        let project_id = Uuid::from_u128(0x50524f4a4543545f53454e54494e454c);
        store.append(&event(1, project_id, 100)).unwrap();
        let token: Vec<u8> = store
            .connection
            .query_row("SELECT project_token FROM events", [], |row| row.get(0))
            .unwrap();
        assert_ne!(token, project_id.as_bytes());
    }
}
