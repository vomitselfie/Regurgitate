use anyhow::{Context, Result, bail};
use chrono::{DateTime, Duration, Utc};
use serde::Serialize;

pub const MAX_RETENTION_DAYS: u32 = 36_500;
pub const MAX_KEEP_RECENT_EVENTS: u64 = 1_000_000;
pub const RETENTION_DELETE_BATCH_SIZE: u64 = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetentionPolicy {
    OlderThanDays(u32),
    KeepRecent(u64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ValidatedRetentionPolicy {
    selection: RetentionSelection,
}

impl RetentionPolicy {
    pub fn validate(self, now: DateTime<Utc>) -> Result<ValidatedRetentionPolicy> {
        Ok(ValidatedRetentionPolicy {
            selection: resolve_policy(self, now)?,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetentionSelection {
    Before(DateTime<Utc>),
    BeyondNewest(u64),
}

pub trait RetentionStore {
    fn count_retention_candidates(&self, selection: RetentionSelection) -> Result<u64>;
    fn delete_retention_batch(&self, selection: RetentionSelection, limit: u64) -> Result<u64>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RetentionStatus {
    Planned,
    Pruned,
    NoChanges,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct RetentionReport {
    pub status: RetentionStatus,
    pub events: u64,
}

pub struct RetentionService<H> {
    history: H,
}

impl<H> RetentionService<H>
where
    H: RetentionStore,
{
    pub fn new(history: H) -> Self {
        Self { history }
    }

    pub fn enforce(
        &self,
        policy: ValidatedRetentionPolicy,
        apply: bool,
    ) -> Result<RetentionReport> {
        let selection = policy.selection;
        if !apply {
            let events = self
                .history
                .count_retention_candidates(selection)
                .context("could not preview encrypted history retention")?;
            return Ok(RetentionReport {
                status: if events == 0 {
                    RetentionStatus::NoChanges
                } else {
                    RetentionStatus::Planned
                },
                events,
            });
        }

        let mut events = 0_u64;
        loop {
            let deleted = self
                .history
                .delete_retention_batch(selection, RETENTION_DELETE_BATCH_SIZE)
                .context("could not prune an encrypted history batch")?;
            if deleted > RETENTION_DELETE_BATCH_SIZE {
                bail!("retention store exceeded the deletion batch limit");
            }
            events = events
                .checked_add(deleted)
                .context("retention deletion count overflow")?;
            if deleted < RETENTION_DELETE_BATCH_SIZE {
                break;
            }
        }
        Ok(RetentionReport {
            status: if events == 0 {
                RetentionStatus::NoChanges
            } else {
                RetentionStatus::Pruned
            },
            events,
        })
    }
}

fn resolve_policy(policy: RetentionPolicy, now: DateTime<Utc>) -> Result<RetentionSelection> {
    match policy {
        RetentionPolicy::OlderThanDays(days) => {
            if days == 0 || days > MAX_RETENTION_DAYS {
                bail!("retention days must be between 1 and {MAX_RETENTION_DAYS}");
            }
            let age = Duration::try_days(i64::from(days)).context("retention age overflow")?;
            let cutoff = now
                .checked_sub_signed(age)
                .context("retention cutoff is outside the supported time range")?;
            Ok(RetentionSelection::Before(cutoff))
        }
        RetentionPolicy::KeepRecent(count) => {
            if count > MAX_KEEP_RECENT_EVENTS {
                bail!("keep-recent must not exceed {MAX_KEEP_RECENT_EVENTS}");
            }
            Ok(RetentionSelection::BeyondNewest(count))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use chrono::TimeZone;

    use super::*;

    struct MemoryStore {
        candidates: Cell<u64>,
        largest_requested_batch: Cell<u64>,
    }

    impl RetentionStore for MemoryStore {
        fn count_retention_candidates(&self, _selection: RetentionSelection) -> Result<u64> {
            Ok(self.candidates.get())
        }

        fn delete_retention_batch(
            &self,
            _selection: RetentionSelection,
            limit: u64,
        ) -> Result<u64> {
            self.largest_requested_batch
                .set(self.largest_requested_batch.get().max(limit));
            let deleted = self.candidates.get().min(limit);
            self.candidates.set(self.candidates.get() - deleted);
            Ok(deleted)
        }
    }

    fn service(candidates: u64) -> RetentionService<MemoryStore> {
        RetentionService::new(MemoryStore {
            candidates: Cell::new(candidates),
            largest_requested_batch: Cell::new(0),
        })
    }

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 19, 12, 0, 0).unwrap()
    }

    #[test]
    fn preview_returns_only_a_count_without_deleting() {
        let service = service(17);
        let report = service
            .enforce(
                RetentionPolicy::OlderThanDays(30).validate(now()).unwrap(),
                false,
            )
            .unwrap();
        assert_eq!(
            report,
            RetentionReport {
                status: RetentionStatus::Planned,
                events: 17,
            }
        );
        assert_eq!(service.history.candidates.get(), 17);
        assert_eq!(service.history.largest_requested_batch.get(), 0);
    }

    #[test]
    fn apply_uses_fixed_size_batches_until_complete() {
        let service = service(RETENTION_DELETE_BATCH_SIZE * 2 + 7);
        let report = service
            .enforce(
                RetentionPolicy::KeepRecent(100).validate(now()).unwrap(),
                true,
            )
            .unwrap();
        assert_eq!(report.status, RetentionStatus::Pruned);
        assert_eq!(report.events, RETENTION_DELETE_BATCH_SIZE * 2 + 7);
        assert_eq!(service.history.candidates.get(), 0);
        assert_eq!(
            service.history.largest_requested_batch.get(),
            RETENTION_DELETE_BATCH_SIZE
        );
    }

    #[test]
    fn validates_policies_before_touching_storage() {
        assert!(RetentionPolicy::OlderThanDays(0).validate(now()).is_err());
        assert!(
            RetentionPolicy::KeepRecent(MAX_KEEP_RECENT_EVENTS + 1)
                .validate(now())
                .is_err()
        );
    }
}
