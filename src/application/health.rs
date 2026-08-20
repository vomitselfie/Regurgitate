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
    fn history_counts(&self) -> Result<Option<HistoryCounts>>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HistoryCounts {
    pub event_count: u64,
    pub hook_event_count: u64,
    pub learned_practice_count: u64,
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
#[serde(rename_all = "snake_case")]
pub enum HookProvider {
    Aoe,
    Claude,
    Codex,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HookReadiness {
    Installed,
    NotInstalled,
    Conflicting,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct HookHealth {
    pub provider: HookProvider,
    pub status: HookReadiness,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct HistoryHealth {
    pub status: ComponentReadiness,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hook_event_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub learned_practice_count: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HealthReport {
    pub status: OverallHealth,
    pub key_store: ComponentReadiness,
    pub history: HistoryHealth,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub hooks: Vec<HookHealth>,
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
        self.inspect_with_hooks(std::iter::empty())
    }

    pub fn inspect_with_hooks(
        &self,
        hooks: impl IntoIterator<Item = (HookProvider, Result<HookReadiness>)>,
    ) -> HealthReport {
        let key_store = match self.key.key_is_present() {
            Ok(true) => ComponentReadiness::Ready,
            Ok(false) => ComponentReadiness::NotConfigured,
            Err(_) => ComponentReadiness::Unavailable,
        };
        let history = match self.history.history_counts() {
            Ok(Some(counts)) => HistoryHealth {
                status: ComponentReadiness::Ready,
                event_count: Some(counts.event_count),
                hook_event_count: Some(counts.hook_event_count),
                learned_practice_count: Some(counts.learned_practice_count),
            },
            Ok(None) => HistoryHealth {
                status: ComponentReadiness::NotConfigured,
                event_count: None,
                hook_event_count: None,
                learned_practice_count: None,
            },
            Err(_) => HistoryHealth {
                status: ComponentReadiness::Unavailable,
                event_count: None,
                hook_event_count: None,
                learned_practice_count: None,
            },
        };
        let mut status = match (key_store, history.status) {
            (ComponentReadiness::Ready, ComponentReadiness::Ready) => OverallHealth::Ready,
            (ComponentReadiness::NotConfigured, ComponentReadiness::NotConfigured) => {
                OverallHealth::NotConfigured
            }
            _ => OverallHealth::Degraded,
        };
        let mut hooks: Vec<_> = hooks
            .into_iter()
            .map(|(provider, readiness)| {
                let hook_status = readiness.unwrap_or(HookReadiness::Unavailable);
                if hook_status != HookReadiness::Installed {
                    status = OverallHealth::Degraded;
                }
                HookHealth {
                    provider,
                    status: hook_status,
                }
            })
            .collect();
        let aoe_installed = hooks.iter().any(|hook| {
            hook.provider == HookProvider::Aoe && hook.status == HookReadiness::Installed
        });
        let codex_installed = hooks.iter().any(|hook| {
            hook.provider == HookProvider::Codex && hook.status == HookReadiness::Installed
        });
        if aoe_installed && codex_installed {
            for hook in &mut hooks {
                if matches!(hook.provider, HookProvider::Aoe | HookProvider::Codex) {
                    hook.status = HookReadiness::Conflicting;
                }
            }
            status = OverallHealth::Degraded;
        }
        HealthReport {
            status,
            key_store,
            history,
            hooks,
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

    struct HistoryProbe(Result<Option<HistoryCounts>>);

    impl HistoryReadinessProbe for HistoryProbe {
        fn history_counts(&self) -> Result<Option<HistoryCounts>> {
            match &self.0 {
                Ok(value) => Ok(*value),
                Err(_) => bail!("PRIVATE_DATABASE_ERROR /home/alice/history.db"),
            }
        }
    }

    #[test]
    fn reports_ready_with_separate_aggregate_counts() {
        let report = HealthService::new(
            KeyProbe(Ok(true)),
            HistoryProbe(Ok(Some(HistoryCounts {
                event_count: 7,
                hook_event_count: 5,
                learned_practice_count: 2,
            }))),
        )
        .inspect();
        assert_eq!(report.status, OverallHealth::Ready);
        assert_eq!(report.key_store, ComponentReadiness::Ready);
        assert_eq!(report.history.event_count, Some(7));
        assert_eq!(report.history.hook_event_count, Some(5));
        assert_eq!(report.history.learned_practice_count, Some(2));
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

    #[test]
    fn optional_hook_checks_are_controlled_and_affect_overall_health() {
        let report = HealthService::new(
            KeyProbe(Ok(true)),
            HistoryProbe(Ok(Some(HistoryCounts {
                event_count: 7,
                hook_event_count: 5,
                learned_practice_count: 2,
            }))),
        )
        .inspect_with_hooks([
            (HookProvider::Aoe, Ok(HookReadiness::Installed)),
            (
                HookProvider::Claude,
                Err(anyhow::anyhow!("PRIVATE_CONFIG_PATH")),
            ),
        ]);
        assert_eq!(report.status, OverallHealth::Degraded);
        assert_eq!(
            report.hooks,
            vec![
                HookHealth {
                    provider: HookProvider::Aoe,
                    status: HookReadiness::Installed,
                },
                HookHealth {
                    provider: HookProvider::Claude,
                    status: HookReadiness::Unavailable,
                },
            ]
        );
        assert!(
            !serde_json::to_string(&report)
                .unwrap()
                .contains("PRIVATE_CONFIG_PATH")
        );
    }

    #[test]
    fn simultaneous_codex_sources_are_reported_as_conflicting() {
        let report = HealthService::new(
            KeyProbe(Ok(true)),
            HistoryProbe(Ok(Some(HistoryCounts {
                event_count: 7,
                hook_event_count: 5,
                learned_practice_count: 2,
            }))),
        )
        .inspect_with_hooks([
            (HookProvider::Aoe, Ok(HookReadiness::Installed)),
            (HookProvider::Codex, Ok(HookReadiness::Installed)),
        ]);

        assert_eq!(report.status, OverallHealth::Degraded);
        assert_eq!(
            report.hooks,
            vec![
                HookHealth {
                    provider: HookProvider::Aoe,
                    status: HookReadiness::Conflicting,
                },
                HookHealth {
                    provider: HookProvider::Codex,
                    status: HookReadiness::Conflicting,
                },
            ]
        );
    }
}
