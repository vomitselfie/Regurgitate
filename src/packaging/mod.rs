mod aoe_config;
mod claude_config;
mod skill;

use serde::Serialize;

pub use aoe_config::{
    AOE_CONFIG_SNIPPET, AoeHookInstallReport, inspect_aoe_hook, install_aoe_hook,
};
pub use claude_config::{CLAUDE_CONFIG_SNIPPET, inspect_claude_hook};
pub use skill::{SkillInstallReport, install_skill};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InstallStatus {
    Planned,
    Installed,
    AlreadyCurrent,
}
