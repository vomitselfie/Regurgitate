use anyhow::{Context, Result};
use serde::Serialize;

use super::ProjectLocator;

/// Project-scoped administrative port. Implementations own encrypted identity
/// lookup and transactional deletion; neither a project ID nor event IDs cross
/// into the application report.
pub trait ProjectHistoryEraser {
    fn count_project_events(&self, project: &ProjectLocator) -> Result<Option<u64>>;
    fn erase_project(&self, project: &ProjectLocator) -> Result<Option<u64>>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ForgetStatus {
    Planned,
    Forgotten,
    NotFound,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ForgetReport {
    pub status: ForgetStatus,
    pub events: u64,
}

pub struct ForgetService<H> {
    history: H,
}

impl<H> ForgetService<H>
where
    H: ProjectHistoryEraser,
{
    pub fn new(history: H) -> Self {
        Self { history }
    }

    pub fn forget(&self, project: &ProjectLocator, apply: bool) -> Result<ForgetReport> {
        let events = if apply {
            self.history
                .erase_project(project)
                .context("could not forget encrypted project history")?
        } else {
            self.history
                .count_project_events(project)
                .context("could not preview encrypted project history deletion")?
        };
        Ok(match events {
            Some(events) => ForgetReport {
                status: if apply {
                    ForgetStatus::Forgotten
                } else {
                    ForgetStatus::Planned
                },
                events,
            },
            None => ForgetReport {
                status: ForgetStatus::NotFound,
                events: 0,
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::Cell, path::PathBuf};

    use anyhow::bail;

    use super::*;

    struct MemoryEraser {
        events: Cell<Option<u64>>,
    }

    impl ProjectHistoryEraser for MemoryEraser {
        fn count_project_events(&self, _project: &ProjectLocator) -> Result<Option<u64>> {
            Ok(self.events.get())
        }

        fn erase_project(&self, _project: &ProjectLocator) -> Result<Option<u64>> {
            Ok(self.events.replace(None))
        }
    }

    struct FailingEraser;

    impl ProjectHistoryEraser for FailingEraser {
        fn count_project_events(&self, _project: &ProjectLocator) -> Result<Option<u64>> {
            bail!("PRIVATE_PREVIEW_FAILURE")
        }

        fn erase_project(&self, _project: &ProjectLocator) -> Result<Option<u64>> {
            bail!("PRIVATE_ERASE_FAILURE")
        }
    }

    fn locator() -> ProjectLocator {
        ProjectLocator::new(PathBuf::from("/private/project"))
    }

    #[test]
    fn preview_does_not_erase_and_apply_is_idempotent() {
        let service = ForgetService::new(MemoryEraser {
            events: Cell::new(Some(4)),
        });
        assert_eq!(
            service.forget(&locator(), false).unwrap(),
            ForgetReport {
                status: ForgetStatus::Planned,
                events: 4,
            }
        );
        assert_eq!(
            service.forget(&locator(), true).unwrap(),
            ForgetReport {
                status: ForgetStatus::Forgotten,
                events: 4,
            }
        );
        assert_eq!(
            service.forget(&locator(), true).unwrap(),
            ForgetReport {
                status: ForgetStatus::NotFound,
                events: 0,
            }
        );
    }

    #[test]
    fn reports_and_errors_do_not_expose_the_locator() {
        let report = ForgetService::new(MemoryEraser {
            events: Cell::new(Some(1)),
        })
        .forget(&locator(), false)
        .unwrap();
        assert!(
            !serde_json::to_string(&report)
                .unwrap()
                .contains("/private/project")
        );

        let error = ForgetService::new(FailingEraser)
            .forget(&locator(), true)
            .unwrap_err();
        assert!(!error.to_string().contains("/private/project"));
        assert!(
            error
                .to_string()
                .contains("could not forget encrypted project history")
        );
    }
}
