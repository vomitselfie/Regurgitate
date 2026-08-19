use std::{fs::OpenOptions, path::Path};

#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

use anyhow::{Context, Result, anyhow};
use rusqlite::{Connection, OptionalExtension, params};
use uuid::Uuid;

use crate::{application::EventSink, core::HistoryEvent};

use super::{
    MasterKey,
    crypto::{EnvelopeMetadata, EventCipher},
    private::{PrivateMetadataCipher, PrivateRecordKind},
};

#[cfg(test)]
use super::crypto::ENVELOPE_VERSION;

pub struct EncryptedStore {
    pub(super) connection: Connection,
    cipher: EventCipher,
    pub(super) private_cipher: PrivateMetadataCipher,
}

pub(super) struct StoredEvent {
    pub id: Vec<u8>,
    pub created_at_ms: i64,
    pub schema_version: i64,
    pub envelope_version: i64,
    pub nonce: Vec<u8>,
    pub ciphertext: Vec<u8>,
}

impl EncryptedStore {
    pub fn open(path: &Path, master_key: &MasterKey) -> Result<Self> {
        ensure_private_database_file(path)?;
        let connection = Connection::open(path)
            .with_context(|| format!("could not open Praxis database at {}", path.display()))?;
        Self::from_connection(connection, master_key)
    }

    #[cfg(test)]
    pub(super) fn open_in_memory(master_key: &MasterKey) -> Result<Self> {
        Self::from_connection(Connection::open_in_memory()?, master_key)
    }

    fn from_connection(connection: Connection, master_key: &MasterKey) -> Result<Self> {
        connection.execute_batch(
            "PRAGMA foreign_keys = ON;
             PRAGMA secure_delete = ON;
             CREATE TABLE IF NOT EXISTS events (
                 id               BLOB PRIMARY KEY NOT NULL,
                 project_token    BLOB,
                 created_at_ms    INTEGER NOT NULL,
                 schema_version   INTEGER NOT NULL,
                 envelope_version INTEGER NOT NULL,
                 nonce            BLOB NOT NULL,
                 ciphertext       BLOB NOT NULL
             );
             CREATE TABLE IF NOT EXISTS private_projects (
                 lookup_token     BLOB PRIMARY KEY NOT NULL,
                 envelope_version INTEGER NOT NULL,
                 nonce            BLOB NOT NULL,
                 ciphertext       BLOB NOT NULL
             );
             CREATE TABLE IF NOT EXISTS private_cursors (
                 lookup_token     BLOB PRIMARY KEY NOT NULL,
                 envelope_version INTEGER NOT NULL,
                 nonce            BLOB NOT NULL,
                 ciphertext       BLOB NOT NULL
             );",
        )?;
        ensure_event_project_token_column(&connection)?;
        connection.execute(
            "CREATE INDEX IF NOT EXISTS events_project_token_created_idx
             ON events(project_token, created_at_ms DESC)",
            [],
        )?;
        Ok(Self {
            connection,
            cipher: EventCipher::new(master_key)?,
            private_cipher: PrivateMetadataCipher::new(master_key)?,
        })
    }

    /// Encrypts completely in memory before invoking SQLite. Returns `false`
    /// when the deterministic event id is already present.
    pub fn append(&self, event: &HistoryEvent) -> Result<bool> {
        let sealed = self.cipher.seal(event)?;
        let metadata = EnvelopeMetadata::for_event(event);
        let project_token = event
            .project_id
            .map(|project_id| {
                self.private_cipher
                    .lookup_token(PrivateRecordKind::EventProject, project_id.as_bytes())
            })
            .transpose()?;
        let changed = self.connection.execute(
            "INSERT OR IGNORE INTO events
                (id, project_token, created_at_ms, schema_version, envelope_version, nonce, ciphertext)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                event.id.as_bytes().as_slice(),
                project_token.as_ref().map(|token| token.as_slice()),
                metadata.created_at_ms,
                i64::from(metadata.schema_version),
                i64::from(metadata.envelope_version),
                sealed.nonce.as_slice(),
                sealed.ciphertext,
            ],
        )?;
        Ok(changed == 1)
    }

    pub fn get(&self, event_id: Uuid) -> Result<Option<HistoryEvent>> {
        let stored = self
            .connection
            .query_row(
                "SELECT id, created_at_ms, schema_version, envelope_version, nonce, ciphertext
                 FROM events WHERE id = ?1",
                [event_id.as_bytes().as_slice()],
                |row| {
                    Ok(StoredEvent {
                        id: row.get(0)?,
                        created_at_ms: row.get(1)?,
                        schema_version: row.get(2)?,
                        envelope_version: row.get(3)?,
                        nonce: row.get(4)?,
                        ciphertext: row.get(5)?,
                    })
                },
            )
            .optional()?;
        stored.map(|stored| self.decrypt_row(stored)).transpose()
    }

    pub fn delete(&self, event_id: Uuid) -> Result<bool> {
        let changed = self.connection.execute(
            "DELETE FROM events WHERE id = ?1",
            [event_id.as_bytes().as_slice()],
        )?;
        Ok(changed == 1)
    }

    pub fn count(&self) -> Result<u64> {
        let count: i64 = self
            .connection
            .query_row("SELECT COUNT(*) FROM events", [], |row| row.get(0))?;
        count
            .try_into()
            .map_err(|_| anyhow!("database returned an invalid event count"))
    }

    pub(super) fn decrypt_row(&self, row: StoredEvent) -> Result<HistoryEvent> {
        let event_id = Uuid::from_slice(&row.id).context("invalid event id in database")?;
        let schema_version = row
            .schema_version
            .try_into()
            .context("invalid event schema version in database")?;
        let envelope_version = row
            .envelope_version
            .try_into()
            .context("invalid encryption envelope version in database")?;
        let metadata = EnvelopeMetadata {
            event_id,
            created_at_ms: row.created_at_ms,
            schema_version,
            envelope_version,
        };
        self.cipher.open(&metadata, &row.nonce, &row.ciphertext)
    }
}

fn ensure_event_project_token_column(connection: &Connection) -> Result<()> {
    let mut statement = connection.prepare("PRAGMA table_info(events)")?;
    let columns = statement.query_map([], |row| row.get::<_, String>(1))?;
    for column in columns {
        if column? == "project_token" {
            return Ok(());
        }
    }
    connection.execute("ALTER TABLE events ADD COLUMN project_token BLOB", [])?;
    Ok(())
}

impl EventSink for EncryptedStore {
    fn append(&self, event: &HistoryEvent) -> Result<bool> {
        EncryptedStore::append(self, event)
    }
}

fn ensure_private_database_file(path: &Path) -> Result<()> {
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true);
    #[cfg(unix)]
    options.mode(0o600);
    options
        .open(path)
        .with_context(|| format!("could not create Praxis database at {}", path.display()))?;

    #[cfg(unix)]
    {
        let mut permissions = std::fs::metadata(path)?.permissions();
        permissions.set_mode(0o600);
        std::fs::set_permissions(path, permissions)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use chrono::{TimeZone, Utc};
    use tempfile::tempdir;

    use crate::core::{
        AgentKind, CURRENT_SCHEMA_VERSION, Capability, ErrorClass, HistoryEvent, Operation, Outcome,
    };

    use super::*;

    fn master(byte: u8) -> MasterKey {
        MasterKey::from_bytes([byte; 32])
    }

    fn event() -> HistoryEvent {
        HistoryEvent {
            id: Uuid::from_u128(0x1234),
            timestamp: Utc.timestamp_millis_opt(1_776_254_400_123).unwrap(),
            session_id: Some("PLAINTEXT_SENTINEL_SESSION".to_owned()),
            project_id: Some(Uuid::from_u128(0x5678)),
            agent: Some(AgentKind::Codex),
            capability: Capability::Shell,
            operation: Operation::Command,
            strategy: None,
            outcome: Outcome::Failure,
            duration_ms: Some(17),
            error_class: Some(ErrorClass::NonzeroExit),
            schema_version: CURRENT_SCHEMA_VERSION,
        }
    }

    #[test]
    fn encrypted_event_round_trips_and_is_idempotent() {
        let store = EncryptedStore::open_in_memory(&master(7)).unwrap();
        let event = event();
        assert!(store.append(&event).unwrap());
        assert!(!store.append(&event).unwrap());
        assert_eq!(store.count().unwrap(), 1);
        assert_eq!(store.get(event.id).unwrap(), Some(event));
    }

    #[test]
    fn wrong_key_cannot_decrypt_a_copied_database() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("history.db");
        let event = event();
        {
            let store = EncryptedStore::open(&path, &master(1)).unwrap();
            store.append(&event).unwrap();
        }

        let copied = temp.path().join("copied.db");
        fs::copy(&path, &copied).unwrap();
        let store = EncryptedStore::open(&copied, &master(2)).unwrap();
        let error = store.get(event.id).unwrap_err();
        assert!(error.to_string().contains("authentication failed"));
    }

    #[test]
    fn modified_ciphertext_fails_authentication() {
        let store = EncryptedStore::open_in_memory(&master(3)).unwrap();
        let event = event();
        store.append(&event).unwrap();
        let mut ciphertext: Vec<u8> = store
            .connection
            .query_row("SELECT ciphertext FROM events", [], |row| row.get(0))
            .unwrap();
        ciphertext[0] ^= 0x80;
        store
            .connection
            .execute("UPDATE events SET ciphertext = ?1", [ciphertext])
            .unwrap();

        let error = store.get(event.id).unwrap_err();
        assert!(error.to_string().contains("authentication failed"));
    }

    #[test]
    fn authenticated_metadata_cannot_be_modified() {
        let store = EncryptedStore::open_in_memory(&master(4)).unwrap();
        let event = event();
        store.append(&event).unwrap();
        store
            .connection
            .execute("UPDATE events SET schema_version = schema_version + 1", [])
            .unwrap();

        let error = store.get(event.id).unwrap_err();
        assert!(error.to_string().contains("authentication failed"));
    }

    #[test]
    fn each_encryption_uses_a_unique_nonce() {
        let store = EncryptedStore::open_in_memory(&master(5)).unwrap();
        let first = event();
        let mut second = event();
        second.id = Uuid::from_u128(0x9999);
        store.append(&first).unwrap();
        store.append(&second).unwrap();

        let distinct: i64 = store
            .connection
            .query_row("SELECT COUNT(DISTINCT hex(nonce)) FROM events", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(distinct, 2);
    }

    #[test]
    fn database_contains_no_event_plaintext() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("history.db");
        {
            let store = EncryptedStore::open(&path, &master(6)).unwrap();
            store.append(&event()).unwrap();
        }
        let bytes = fs::read(path).unwrap();
        assert!(
            !bytes
                .windows(b"PLAINTEXT_SENTINEL_SESSION".len())
                .any(|window| window == b"PLAINTEXT_SENTINEL_SESSION")
        );
    }

    #[test]
    fn delete_removes_the_encrypted_record() {
        let store = EncryptedStore::open_in_memory(&master(8)).unwrap();
        let event = event();
        store.append(&event).unwrap();
        assert!(store.delete(event.id).unwrap());
        assert!(!store.delete(event.id).unwrap());
        assert_eq!(store.get(event.id).unwrap(), None);
    }

    #[cfg(unix)]
    #[test]
    fn database_file_is_owner_only() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("history.db");
        let _store = EncryptedStore::open(&path, &master(9)).unwrap();
        let mode = fs::metadata(path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[test]
    fn envelope_version_is_explicit() {
        assert_eq!(ENVELOPE_VERSION, 1);
    }

    #[test]
    fn opens_the_pre_project_index_schema_non_destructively() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("history.db");
        let connection = Connection::open(&path).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE events (
                    id BLOB PRIMARY KEY NOT NULL,
                    created_at_ms INTEGER NOT NULL,
                    schema_version INTEGER NOT NULL,
                    envelope_version INTEGER NOT NULL,
                    nonce BLOB NOT NULL,
                    ciphertext BLOB NOT NULL
                );",
            )
            .unwrap();
        drop(connection);

        let store = EncryptedStore::open(&path, &master(10)).unwrap();
        let mut statement = store
            .connection
            .prepare("PRAGMA table_info(events)")
            .unwrap();
        let columns: Vec<String> = statement
            .query_map([], |row| row.get(1))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert!(columns.iter().any(|column| column == "project_token"));
    }
}
