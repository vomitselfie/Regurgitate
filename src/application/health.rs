use anyhow::Result;
use serde::Serialize;

pub trait KeyReadinessProbe {
    /// Returns whether an existing valid key is available. Implementations
    /// must not create or replace one.
    fn key_is_present(&self) -> Result<bool>;
}

pub trait HistoryReadinessProbe {
    /// Returns `None` when no history database exists. Implementations must
    /// inspect an existing database without creating or migrating it.
    fn event_count(&self) -> Result<Option<u64>>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ComponentReadiness {
    Ready,
    NotConfigured,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OverallHealth {
    Ready,
    NotConfigured,
    Degraded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct HistoryHealth {
    pub status: ComponentReadiness,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_count: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct HealthReport {
    pub status: OverallHealth,
    pub key_store: ComponentReadiness,
    pub history: HistoryHealth,
}

/// Reduces backend-specific probes to a small, non-sensitive projection.
/// Backend errors deliberately become `unavailable` and are never included in
/// the report.
pub struct HealthService<K, H> {
    key: K,
    history: H,
}

impl<K, H> HealthService<K, H>
where
    K: KeyReadinessProbe,
    H: HistoryReadinessProbe,
{
    pub fn new(key: K, history: H) -> Self {
        Self { key, history }
    }

    pub fn inspect(&self) -> HealthReport {
        let key_store = match self.key.key_is_present() {
            Ok(true) => ComponentReadiness::Ready,
            Ok(false) => ComponentReadiness::NotConfigured,
            Err(_) => ComponentReadiness::Unavailable,
        };
        let history = match self.history.event_count() {
            Ok(Some(event_count)) => HistoryHealth {
                status: ComponentReadiness::Ready,
                event_count: Some(event_count),
            },
            Ok(None) => HistoryHealth {
                status: ComponentReadiness::NotConfigured,
                event_count: None,
            },
            Err(_) => HistoryHealth {
                status: ComponentReadiness::Unavailable,
                event_count: None,
            },
        };
        let status = match (key_store, history.status) {
            (ComponentReadiness::Ready, ComponentReadiness::Ready) => OverallHealth::Ready,
            (ComponentReadiness::NotConfigured, ComponentReadiness::NotConfigured) => {
                OverallHealth::NotConfigured
            }
            _ => OverallHealth::Degraded,
        };
        HealthReport {
            status,
            key_store,
            history,
        }
    }
}

#[cfg(test)]
mod tests {
    use anyhow::bail;

    use super::*;

    struct KeyProbe(Result<bool>);

    impl KeyReadinessProbe for KeyProbe {
        fn key_is_present(&self) -> Result<bool> {
            self.0
                .as_ref()
                .copied()
                .map_err(|error| anyhow::anyhow!("{error}"))
        }
    }

    struct HistoryProbe(Result<Option<u64>>);

    impl HistoryReadinessProbe for HistoryProbe {
        fn event_count(&self) -> Result<Option<u64>> {
            match &self.0 {
                Ok(value) => Ok(*value),
                Err(_) => bail!("PRIVATE_DATABASE_ERROR /home/alice/history.db"),
            }
        }
    }

    #[test]
    fn reports_ready_with_only_an_aggregate_count() {
        let report = HealthService::new(KeyProbe(Ok(true)), HistoryProbe(Ok(Some(7)))).inspect();
        assert_eq!(report.status, OverallHealth::Ready);
        assert_eq!(report.key_store, ComponentReadiness::Ready);
        assert_eq!(report.history.event_count, Some(7));
    }

    #[test]
    fn reports_a_clean_unconfigured_state() {
        let report = HealthService::new(KeyProbe(Ok(false)), HistoryProbe(Ok(None))).inspect();
        assert_eq!(report.status, OverallHealth::NotConfigured);
        assert_eq!(report.key_store, ComponentReadiness::NotConfigured);
        assert_eq!(report.history.status, ComponentReadiness::NotConfigured);
    }

    #[test]
    fn backend_errors_are_reduced_without_leaking_details() {
        let report = HealthService::new(
            KeyProbe(Err(anyhow::anyhow!("PRIVATE_KEYRING_ERROR"))),
            HistoryProbe(Err(anyhow::anyhow!("ignored"))),
        )
        .inspect();
        assert_eq!(report.status, OverallHealth::Degraded);
        assert_eq!(report.key_store, ComponentReadiness::Unavailable);
        assert_eq!(report.history.status, ComponentReadiness::Unavailable);

        let encoded = serde_json::to_string(&report).unwrap();
        for forbidden in [
            "PRIVATE_KEYRING_ERROR",
            "PRIVATE_DATABASE_ERROR",
            "/home/alice",
        ] {
            assert!(!encoded.contains(forbidden), "leaked {forbidden:?}");
        }
    }
}
