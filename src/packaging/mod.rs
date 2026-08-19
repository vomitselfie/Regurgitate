mod aoe_config;
mod skill;

use serde::Serialize;

pub use aoe_config::{AOE_CONFIG_SNIPPET, AoeHookInstallReport, install_aoe_hook};
pub use skill::{SkillInstallReport, install_skill};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InstallStatus {
    Planned,
    Installed,
    AlreadyCurrent,
}
