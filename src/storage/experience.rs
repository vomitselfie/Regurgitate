use anyhow::{Context, Result, bail};
use chrono::Utc;
use rusqlite::{OptionalExtension, params};
use uuid::Uuid;

use crate::{
    application::{ExperienceStore, ScopeKey},
    core::ExperienceCapsule,
};

use super::{
    crypto::{ENVELOPE_VERSION, ExperienceEnvelope},
    private::PrivateRecordKind,
    sqlite::EncryptedStore,
};

struct StoredExperience {
    id: Vec<u8>,
    scope_token: Vec<u8>,
    origin_token: Vec<u8>,
    created_at_ms: i64,
    updated_at_ms: i64,
    schema_version: i64,
    envelope_version: i64,
    nonce: Vec<u8>,
    ciphertext: Vec<u8>,
}

const SELECT_COLUMNS: &str = "id, scope_token, origin_token, created_at_ms, updated_at_ms,
    schema_version, envelope_version, nonce, ciphertext";

impl EncryptedStore {
    pub(super) fn experience_scope_token(&self, scope: ScopeKey) -> Result<[u8; 32]> {
        self.private_cipher
            .lookup_token(PrivateRecordKind::ExperienceScope, &scope.identity_bytes())
    }

    fn experience_origin_token(&self, project_id: Uuid) -> Result<[u8; 32]> {
        // Shares the event project token so forgetting a project covers both
        // ledgers with one tombstone.
        self.private_cipher
            .lookup_token(PrivateRecordKind::EventProject, project_id.as_bytes())
    }

    fn seal_experience(
        &self,
        capsule: &ExperienceCapsule,
        created_at_ms: i64,
    ) -> Result<(ExperienceEnvelope, super::crypto::SealedEvent)> {
        capsule.validate()?;
        let scope = ScopeKey::for_capsule(capsule)?;
        let envelope = ExperienceEnvelope {
            id: capsule.id,
            scope_token: self.experience_scope_token(scope)?,
            origin_token: self.experience_origin_token(capsule.project_id)?,
            created_at_ms,
            updated_at_ms: Utc::now()
                .timestamp_millis()
                .max(capsule.last_confirmed_at.timestamp_millis()),
            schema_version: capsule.schema_version,
            envelope_version: ENVELOPE_VERSION,
        };
        let sealed = self.experience_cipher.seal(&envelope, capsule)?;
        Ok((envelope, sealed))
    }

    fn decrypt_experience(&self, row: StoredExperience) -> Result<ExperienceCapsule> {
        let envelope = ExperienceEnvelope {
            id: Uuid::from_slice(&row.id).context("invalid experience id in database")?,
            scope_token: row
                .scope_token
                .as_slice()
                .try_into()
                .context("invalid experience scope token in database")?,
            origin_token: row
                .origin_token
                .as_slice()
                .try_into()
                .context("invalid experience origin token in database")?,
            created_at_ms: row.created_at_ms,
            updated_at_ms: row.updated_at_ms,
            schema_version: row
                .schema_version
                .try_into()
                .context("invalid experience schema version in database")?,
            envelope_version: row
                .envelope_version
                .try_into()
                .context("invalid experience envelope version in database")?,
        };
        self.experience_cipher
            .open(&envelope, &row.nonce, &row.ciphertext)
    }

    fn experiences_where(
        &self,
        column: &str,
        token: &[u8; 32],
        limit: usize,
    ) -> Result<Vec<ExperienceCapsule>> {
        let mut stored = Vec::new();
        {
            let mut statement = self.connection.prepare(&format!(
                "SELECT {SELECT_COLUMNS} FROM experiences
                 WHERE {column} = ?1
                 ORDER BY updated_at_ms DESC
                 LIMIT ?2"
            ))?;
            let rows = statement.query_map(
                params![
                    token.as_slice(),
                    i64::try_from(limit.min(i64::MAX as usize))?
                ],
                stored_experience_from_row,
            )?;
            for row in rows {
                stored.push(row?);
            }
        }
        let mut capsules = Vec::with_capacity(stored.len());
        for row in stored {
            capsules.push(self.decrypt_experience(row)?);
        }
        Ok(capsules)
    }

    pub fn experience_count(&self) -> Result<u64> {
        let count: i64 =
            self.connection
                .query_row("SELECT COUNT(*) FROM experiences", [], |row| row.get(0))?;
        count
            .try_into()
            .map_err(|_| anyhow::anyhow!("database returned an invalid experience count"))
    }
}

fn stored_experience_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredExperience> {
    Ok(StoredExperience {
        id: row.get(0)?,
        scope_token: row.get(1)?,
        origin_token: row.get(2)?,
        created_at_ms: row.get(3)?,
        updated_at_ms: row.get(4)?,
        schema_version: row.get(5)?,
        envelope_version: row.get(6)?,
        nonce: row.get(7)?,
        ciphertext: row.get(8)?,
    })
}

impl ExperienceStore for EncryptedStore {
    fn append_experience(&self, capsule: &ExperienceCapsule) -> Result<bool> {
        let (envelope, sealed) =
            self.seal_experience(capsule, capsule.created_at.timestamp_millis())?;
        let changed = self.connection.execute(
            "INSERT OR IGNORE INTO experiences
                (id, scope_token, origin_token, created_at_ms, updated_at_ms,
                 schema_version, envelope_version, nonce, ciphertext)
             SELECT ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9
             WHERE NOT EXISTS (
                 SELECT 1 FROM forgotten_project_tokens WHERE project_token = ?3
             )",
            params![
                envelope.id.as_bytes().as_slice(),
                envelope.scope_token.as_slice(),
                envelope.origin_token.as_slice(),
                envelope.created_at_ms,
                envelope.updated_at_ms,
                i64::from(envelope.schema_version),
                i64::from(envelope.envelope_version),
                sealed.nonce.as_slice(),
                sealed.ciphertext,
            ],
        )?;
        Ok(changed == 1)
    }

    fn replace_experience(&self, capsule: &ExperienceCapsule) -> Result<bool> {
        let created_at_ms: Option<i64> = self
            .connection
            .query_row(
                "SELECT created_at_ms FROM experiences WHERE id = ?1",
                [capsule.id.as_bytes().as_slice()],
                |row| row.get(0),
            )
            .optional()?;
        let Some(created_at_ms) = created_at_ms else {
            return Ok(false);
        };
        if created_at_ms != capsule.created_at.timestamp_millis() {
            bail!("experience creation time cannot change");
        }
        let (envelope, sealed) = self.seal_experience(capsule, created_at_ms)?;
        let changed = self.connection.execute(
            "UPDATE experiences
             SET scope_token = ?2, origin_token = ?3, updated_at_ms = ?4,
                 schema_version = ?5, envelope_version = ?6, nonce = ?7, ciphertext = ?8
             WHERE id = ?1
               AND NOT EXISTS (
                   SELECT 1 FROM forgotten_project_tokens WHERE project_token = ?3
               )",
            params![
                envelope.id.as_bytes().as_slice(),
                envelope.scope_token.as_slice(),
                envelope.origin_token.as_slice(),
                envelope.updated_at_ms,
                i64::from(envelope.schema_version),
                i64::from(envelope.envelope_version),
                sealed.nonce.as_slice(),
                sealed.ciphertext,
            ],
        )?;
        Ok(changed == 1)
    }

    fn scoped_experiences(&self, scope: ScopeKey, limit: usize) -> Result<Vec<ExperienceCapsule>> {
        let token = self.experience_scope_token(scope)?;
        let capsules = self.experiences_where("scope_token", &token, limit)?;
        Ok(capsules
            .into_iter()
            .filter(|capsule| ScopeKey::for_capsule(capsule).ok() == Some(scope))
            .collect())
    }

    fn project_experiences(
        &self,
        project_id: Uuid,
        limit: usize,
    ) -> Result<Vec<ExperienceCapsule>> {
        let token = self.experience_origin_token(project_id)?;
        let capsules = self.experiences_where("origin_token", &token, limit)?;
        Ok(capsules
            .into_iter()
            .filter(|capsule| capsule.project_id == project_id)
            .collect())
    }

    fn experiences_by_prefix(&self, prefix: &str) -> Result<Vec<ExperienceCapsule>> {
        self.experiences_with_id_prefix(prefix)
    }

    fn resolve_confirmation_reference(
        &self,
        reference: &str,
    ) -> Result<Option<crate::application::ConfirmationReference>> {
        EncryptedStore::resolve_confirmation_reference(self, reference)
    }
}

impl EncryptedStore {
    fn experiences_with_id_prefix(&self, prefix: &str) -> Result<Vec<ExperienceCapsule>> {
        if prefix.len() < 8
            || !prefix
                .chars()
                .all(|character| character.is_ascii_hexdigit())
        {
            bail!("a capsule selector is at least eight hexadecimal characters");
        }
        let mut stored = Vec::new();
        {
            let mut statement = self.connection.prepare(&format!(
                "SELECT {SELECT_COLUMNS} FROM experiences
                 WHERE lower(hex(id)) LIKE ?1
                 LIMIT 16"
            ))?;
            let rows = statement.query_map(
                params![format!("{}%", prefix.to_ascii_lowercase())],
                stored_experience_from_row,
            )?;
            for row in rows {
                stored.push(row?);
            }
        }
        let mut capsules = Vec::with_capacity(stored.len());
        for row in stored {
            capsules.push(self.decrypt_experience(row)?);
        }
        Ok(capsules)
    }
}

impl crate::query::ExperienceSource for EncryptedStore {
    fn scoped_experiences(&self, scope: ScopeKey, limit: usize) -> Result<Vec<ExperienceCapsule>> {
        ExperienceStore::scoped_experiences(self, scope, limit)
    }

    fn confirmation_reference(&self, capsule_id: Uuid) -> Result<String> {
        self.issue_confirmation_reference(capsule_id)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use chrono::{TimeZone, Utc};
    use tempfile::tempdir;

    use crate::{
        core::{
            ApplicabilityTags, EXPERIENCE_SCHEMA_VERSION, Ecosystem, EnvironmentFingerprint,
            EvidenceEntry, Lesson, MemoryLifecycle, MemoryScope, MutationMode, Procedure,
            SemanticOutcome, Situation, TaskKind,
        },
        storage::MasterKey,
    };

    use super::*;

    fn capsule(id: u128, project: u128, scope: MemoryScope) -> ExperienceCapsule {
        let at = Utc
            .timestamp_millis_opt(1_776_254_400_000 + id as i64)
            .unwrap();
        ExperienceCapsule {
            id: Uuid::from_u128(id),
            project_id: Uuid::from_u128(project),
            scope,
            scope_id: (scope == MemoryScope::Project).then(|| Uuid::from_u128(project)),
            task: TaskKind::Debugging,
            situation: Some(
                Situation::new("SITUATION SENTINEL generated artifact needs native checks")
                    .unwrap(),
            ),
            lesson: Some(
                Lesson::new("LESSON SENTINEL change one placement class then verify natively")
                    .unwrap(),
            ),
            caveat: None,
            procedure: Procedure {
                mutation: Some(MutationMode::IncrementalNativeRegeneration),
                ..Procedure::default()
            },
            applicability: ApplicabilityTags {
                ecosystem: Some(Ecosystem::Kicad),
                ..ApplicabilityTags::default()
            },
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
    }

    #[test]
    fn capsules_round_trip_by_scope_and_origin() {
        let store = EncryptedStore::open_in_memory(&MasterKey::from_bytes([51; 32])).unwrap();
        let project = capsule(1, 7, MemoryScope::Project);
        let global = capsule(2, 7, MemoryScope::Global);
        let other = capsule(3, 8, MemoryScope::Project);
        assert!(store.append_experience(&project).unwrap());
        assert!(!store.append_experience(&project).unwrap());
        store.append_experience(&global).unwrap();
        store.append_experience(&other).unwrap();

        let scoped = store
            .scoped_experiences(ScopeKey::Project(Uuid::from_u128(7)), 10)
            .unwrap();
        assert_eq!(scoped, vec![project.clone()]);
        let globals = store.scoped_experiences(ScopeKey::Global, 10).unwrap();
        assert_eq!(globals, vec![global.clone()]);
        let origin = store.project_experiences(Uuid::from_u128(7), 10).unwrap();
        assert_eq!(origin.len(), 2);
        assert!(
            origin
                .iter()
                .all(|capsule| capsule.project_id == Uuid::from_u128(7))
        );
        assert_eq!(store.experience_count().unwrap(), 3);
    }

    #[test]
    fn selector_prefix_lookup_finds_capsules_across_scopes() {
        let store = EncryptedStore::open_in_memory(&MasterKey::from_bytes([56; 32])).unwrap();
        let global = capsule(9, 7, MemoryScope::Global);
        store.append_experience(&global).unwrap();
        let selector = crate::application::selector_for(global.id);
        let found = store.experiences_by_prefix(&selector).unwrap();
        assert_eq!(found, vec![global]);
        assert!(
            store
                .experiences_by_prefix("ffffffffffff")
                .unwrap()
                .is_empty()
        );
        assert!(store.experiences_by_prefix("abc").is_err());
    }

    #[test]
    fn replace_re_encrypts_and_preserves_creation_time() {
        let store = EncryptedStore::open_in_memory(&MasterKey::from_bytes([52; 32])).unwrap();
        let mut capsule = capsule(1, 7, MemoryScope::Project);
        store.append_experience(&capsule).unwrap();
        capsule.confirm(EvidenceEntry::agent_reported(
            capsule.created_at + chrono::Duration::days(1),
            SemanticOutcome::Failure,
            None,
            Default::default(),
        ));
        assert!(store.replace_experience(&capsule).unwrap());
        let stored = store
            .scoped_experiences(ScopeKey::Project(Uuid::from_u128(7)), 10)
            .unwrap();
        assert_eq!(stored[0].evidence.len(), 2);

        let mut moved = capsule.clone();
        moved.created_at += chrono::Duration::seconds(1);
        assert!(store.replace_experience(&moved).is_err());
        let missing = self::capsule(99, 7, MemoryScope::Project);
        assert!(!store.replace_experience(&missing).unwrap());
    }

    #[test]
    fn plaintext_columns_hold_no_semantic_content() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("history.db");
        {
            let store = EncryptedStore::open(&path, &MasterKey::from_bytes([53; 32])).unwrap();
            store
                .append_experience(&capsule(1, 7, MemoryScope::Project))
                .unwrap();
            let columns: Vec<String> = store
                .connection
                .prepare("PRAGMA table_info(experiences)")
                .unwrap()
                .query_map([], |row| row.get::<_, String>(1))
                .unwrap()
                .map(Result::unwrap)
                .collect();
            assert_eq!(
                columns,
                [
                    "id",
                    "scope_token",
                    "origin_token",
                    "created_at_ms",
                    "updated_at_ms",
                    "schema_version",
                    "envelope_version",
                    "nonce",
                    "ciphertext"
                ]
            );
        }
        let bytes = fs::read(path).unwrap();
        for sentinel in [
            b"SITUATION SENTINEL".as_slice(),
            b"LESSON SENTINEL",
            b"kicad",
            b"debugging",
            b"incremental",
        ] {
            assert!(
                !bytes
                    .windows(sentinel.len())
                    .any(|window| window == sentinel),
                "database leaked {:?}",
                String::from_utf8_lossy(sentinel)
            );
        }
    }

    #[test]
    fn tampered_scope_token_fails_authentication() {
        let store = EncryptedStore::open_in_memory(&MasterKey::from_bytes([54; 32])).unwrap();
        store
            .append_experience(&capsule(1, 7, MemoryScope::Project))
            .unwrap();
        let global = store.experience_scope_token(ScopeKey::Global).unwrap();
        store
            .connection
            .execute(
                "UPDATE experiences SET scope_token = ?1",
                [global.as_slice()],
            )
            .unwrap();
        let error = store.scoped_experiences(ScopeKey::Global, 10).unwrap_err();
        assert!(error.to_string().contains("authentication failed"));
    }

    #[test]
    fn forgotten_projects_reject_new_capsules() {
        let store = EncryptedStore::open_in_memory(&MasterKey::from_bytes([55; 32])).unwrap();
        let token = store.experience_origin_token(Uuid::from_u128(7)).unwrap();
        store
            .connection
            .execute(
                "INSERT INTO forgotten_project_tokens (project_token) VALUES (?1)",
                [token.as_slice()],
            )
            .unwrap();
        assert!(
            !store
                .append_experience(&capsule(1, 7, MemoryScope::Global))
                .unwrap()
        );
    }
}
