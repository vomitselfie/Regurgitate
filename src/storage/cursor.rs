use anyhow::{Context, Result, bail};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};

use crate::application::{CURRENT_CURSOR_VERSION, CursorStore, IngestionCursor};

use super::{
    private::{PRIVATE_ENVELOPE_VERSION, PrivateRecordKind},
    sqlite::EncryptedStore,
};

const CURSOR_RECORD_VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CursorRecord {
    session_id: String,
    cursor: IngestionCursor,
    schema_version: u32,
}

struct StoredPrivateRecord {
    envelope_version: i64,
    nonce: Vec<u8>,
    ciphertext: Vec<u8>,
}

impl CursorStore for EncryptedStore {
    fn load_cursor(&self, session_id: &str) -> Result<Option<IngestionCursor>> {
        let token = self
            .private_cipher
            .lookup_token(PrivateRecordKind::Cursor, session_id.as_bytes())?;
        let stored = self
            .connection
            .query_row(
                "SELECT envelope_version, nonce, ciphertext
                 FROM private_cursors WHERE lookup_token = ?1",
                [token.as_slice()],
                |row| {
                    Ok(StoredPrivateRecord {
                        envelope_version: row.get(0)?,
                        nonce: row.get(1)?,
                        ciphertext: row.get(2)?,
                    })
                },
            )
            .optional()?;
        let Some(stored) = stored else {
            return Ok(None);
        };
        let envelope_version = stored
            .envelope_version
            .try_into()
            .context("invalid cursor envelope version in database")?;
        let record: CursorRecord = self.private_cipher.open(
            PrivateRecordKind::Cursor,
            &token,
            envelope_version,
            &stored.nonce,
            &stored.ciphertext,
        )?;
        if record.schema_version != CURSOR_RECORD_VERSION
            || record.cursor.schema_version != CURRENT_CURSOR_VERSION
            || record.session_id != session_id
        {
            bail!("encrypted cursor does not match its lookup identity");
        }
        Ok(Some(record.cursor))
    }

    fn save_cursor(&self, session_id: &str, cursor: &IngestionCursor) -> Result<()> {
        if cursor.schema_version != CURRENT_CURSOR_VERSION {
            bail!("cannot store an unsupported ingestion cursor version");
        }
        let token = self
            .private_cipher
            .lookup_token(PrivateRecordKind::Cursor, session_id.as_bytes())?;
        let record = CursorRecord {
            session_id: session_id.to_owned(),
            cursor: cursor.clone(),
            schema_version: CURSOR_RECORD_VERSION,
        };
        let sealed = self
            .private_cipher
            .seal(PrivateRecordKind::Cursor, &token, &record)?;
        self.connection.execute(
            "INSERT INTO private_cursors
                (lookup_token, envelope_version, nonce, ciphertext)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(lookup_token) DO UPDATE SET
                envelope_version = excluded.envelope_version,
                nonce = excluded.nonce,
                ciphertext = excluded.ciphertext",
            params![
                token.as_slice(),
                i64::from(PRIVATE_ENVELOPE_VERSION),
                sealed.nonce.as_slice(),
                sealed.ciphertext,
            ],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use crate::storage::MasterKey;

    use super::*;

    #[test]
    fn cursor_round_trips_without_plaintext_session_identity() {
        let temp = tempdir().unwrap();
        let database = temp.path().join("history.db");
        let cursor = IngestionCursor {
            committed_offset: 123,
            source_length: 130,
            ..IngestionCursor::empty()
        };
        {
            let store = EncryptedStore::open(&database, &MasterKey::from_bytes([32; 32])).unwrap();
            store
                .save_cursor("PLAINTEXT_SENTINEL_SESSION", &cursor)
                .unwrap();
            assert_eq!(
                store.load_cursor("PLAINTEXT_SENTINEL_SESSION").unwrap(),
                Some(cursor)
            );
        }
        let bytes = fs::read(database).unwrap();
        assert!(
            !bytes
                .windows(b"PLAINTEXT_SENTINEL_SESSION".len())
                .any(|window| window == b"PLAINTEXT_SENTINEL_SESSION")
        );
    }

    #[test]
    fn cursor_ciphertext_is_authenticated() {
        let temp = tempdir().unwrap();
        let database = temp.path().join("history.db");
        let store = EncryptedStore::open(&database, &MasterKey::from_bytes([33; 32])).unwrap();
        store
            .save_cursor("session-1", &IngestionCursor::empty())
            .unwrap();
        let mut ciphertext: Vec<u8> = store
            .connection
            .query_row("SELECT ciphertext FROM private_cursors", [], |row| {
                row.get(0)
            })
            .unwrap();
        ciphertext[0] ^= 0x80;
        store
            .connection
            .execute("UPDATE private_cursors SET ciphertext = ?1", [ciphertext])
            .unwrap();

        let error = store.load_cursor("session-1").unwrap_err();
        assert!(error.to_string().contains("authentication failed"));
    }
}
