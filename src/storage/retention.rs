use anyhow::{Context, Result, bail};
use rusqlite::{Connection, Transaction, TransactionBehavior, params};

use crate::application::{RETENTION_DELETE_BATCH_SIZE, RetentionSelection, RetentionStore};

use super::sqlite::EncryptedStore;

impl RetentionStore for EncryptedStore {
    fn count_retention_candidates(&self, selection: RetentionSelection) -> Result<u64> {
        match selection {
            RetentionSelection::Before(cutoff) => {
                count_before(&self.connection, cutoff.timestamp_millis())
            }
            RetentionSelection::BeyondNewest(keep) => {
                let total = event_count(&self.connection)?;
                Ok(total.saturating_sub(keep))
            }
        }
    }

    fn delete_retention_batch(&self, selection: RetentionSelection, limit: u64) -> Result<u64> {
        if limit == 0 || limit > RETENTION_DELETE_BATCH_SIZE {
            bail!("invalid retention deletion batch size");
        }
        let transaction =
            Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)
                .context("could not begin retention transaction")?;
        let limit = i64::try_from(limit)?;
        let deleted = match selection {
            RetentionSelection::Before(cutoff) => {
                let cutoff_ms = cutoff.timestamp_millis();
                let events = transaction.execute(
                    "DELETE FROM events WHERE id IN (
                        SELECT id FROM events
                        WHERE created_at_ms < ?1
                        ORDER BY created_at_ms ASC, id ASC
                        LIMIT ?2
                    )",
                    params![cutoff_ms, limit],
                )?;
                let remaining = limit - i64::try_from(events)?;
                let experiences = if remaining > 0 {
                    transaction.execute(
                        "DELETE FROM experiences WHERE id IN (
                            SELECT id FROM experiences
                            WHERE updated_at_ms < ?1
                            ORDER BY updated_at_ms ASC, id ASC
                            LIMIT ?2
                        )",
                        params![cutoff_ms, remaining],
                    )?
                } else {
                    0
                };
                events + experiences
            }
            RetentionSelection::BeyondNewest(keep) => transaction.execute(
                "DELETE FROM events WHERE id IN (
                    SELECT id FROM events
                    WHERE id NOT IN (
                        SELECT id FROM events
                        ORDER BY created_at_ms DESC, id DESC
                        LIMIT ?1
                    )
                    ORDER BY created_at_ms ASC, id ASC
                    LIMIT ?2
                )",
                params![i64::try_from(keep)?, limit],
            )?,
        };
        transaction.commit()?;
        u64::try_from(deleted).context("retention batch returned an invalid deletion count")
    }
}

fn event_count(connection: &Connection) -> Result<u64> {
    let count: i64 = connection.query_row("SELECT COUNT(*) FROM events", [], |row| row.get(0))?;
    count
        .try_into()
        .context("history database returned an invalid event count")
}

fn count_before(connection: &Connection, cutoff_ms: i64) -> Result<u64> {
    // Capsules age by their last confirmation, so a lesson that keeps being
    // confirmed is retained while stale ones expire with old events.
    let count: i64 = connection.query_row(
        "SELECT (SELECT COUNT(*) FROM events WHERE created_at_ms < ?1)
              + (SELECT COUNT(*) FROM experiences WHERE updated_at_ms < ?1)",
        [cutoff_ms],
        |row| row.get(0),
    )?;
    count
        .try_into()
        .context("history database returned an invalid retention count")
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, TimeZone, Utc};
    use uuid::Uuid;

    use crate::{
        core::{
            AgentKind, CURRENT_SCHEMA_VERSION, Capability, EvidenceKind, HistoryEvent, Operation,
            Outcome,
        },
        storage::MasterKey,
    };

    use super::*;

    fn event(id: u128, timestamp_ms: i64) -> HistoryEvent {
        HistoryEvent {
            id: Uuid::from_u128(id),
            timestamp: Utc.timestamp_millis_opt(timestamp_ms).unwrap(),
            session_id: Some("PRIVATE_RETENTION_SESSION".to_owned()),
            project_id: Some(Uuid::from_u128(0x50524f4a454354)),
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

    fn timestamp(day: i64) -> i64 {
        Utc.with_ymd_and_hms(2026, 8, 1, 0, 0, 0)
            .unwrap()
            .checked_add_signed(Duration::days(day))
            .unwrap()
            .timestamp_millis()
    }

    #[test]
    fn age_policy_is_strict_and_batches_oldest_first() {
        let store = EncryptedStore::open_in_memory(&MasterKey::from_bytes([81; 32])).unwrap();
        for (id, day) in [(1, 0), (2, 1), (3, 2), (4, 3)] {
            store.append(&event(id, timestamp(day))).unwrap();
        }
        let selection = RetentionSelection::Before(Utc.timestamp_millis_opt(timestamp(2)).unwrap());
        assert_eq!(store.count_retention_candidates(selection).unwrap(), 2);
        assert_eq!(store.delete_retention_batch(selection, 1).unwrap(), 1);
        assert_eq!(store.count_retention_candidates(selection).unwrap(), 1);
        assert!(store.get(Uuid::from_u128(1)).unwrap().is_none());
        assert!(store.get(Uuid::from_u128(2)).unwrap().is_some());
        assert!(store.get(Uuid::from_u128(3)).unwrap().is_some());
    }

    #[test]
    fn count_policy_keeps_the_newest_deterministically() {
        let store = EncryptedStore::open_in_memory(&MasterKey::from_bytes([82; 32])).unwrap();
        for id in 1..=5 {
            store.append(&event(id, timestamp(id as i64))).unwrap();
        }
        let selection = RetentionSelection::BeyondNewest(2);
        assert_eq!(store.count_retention_candidates(selection).unwrap(), 3);
        assert_eq!(store.delete_retention_batch(selection, 2).unwrap(), 2);
        assert_eq!(store.delete_retention_batch(selection, 2).unwrap(), 1);
        assert_eq!(store.count().unwrap(), 2);
        assert!(store.get(Uuid::from_u128(4)).unwrap().is_some());
        assert!(store.get(Uuid::from_u128(5)).unwrap().is_some());
    }

    #[test]
    fn retention_does_not_decrypt_event_payloads() {
        let store = EncryptedStore::open_in_memory(&MasterKey::from_bytes([83; 32])).unwrap();
        store.append(&event(1, timestamp(0))).unwrap();
        store
            .connection
            .execute(
                "UPDATE events SET ciphertext = X'00' WHERE id = ?1",
                [Uuid::from_u128(1).as_bytes().as_slice()],
            )
            .unwrap();
        let selection = RetentionSelection::Before(Utc.timestamp_millis_opt(timestamp(1)).unwrap());
        assert_eq!(store.count_retention_candidates(selection).unwrap(), 1);
        assert_eq!(store.delete_retention_batch(selection, 1).unwrap(), 1);
        assert_eq!(store.count().unwrap(), 0);
    }
}
