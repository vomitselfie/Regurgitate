use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};

use crate::core::Operation;

#[derive(Parser)]
#[command(name = "praxis", version, about)]
pub struct Cli {
    #[command(subcommand)]
    pub(crate) command: Command,
}

impl Cli {
    pub fn from_env() -> Self {
        Self::parse()
    }
}

#[derive(Subcommand)]
pub(crate) enum Command {
    /// Normalize one Codex hook event from stdin without retaining raw fields.
    DebugHook,

    /// Ingest the Codex session identified by an AoE status-hook environment.
    AoeHook,

    /// Print an AoE status-hook configuration snippet for manual installation.
    PrintAoeConfig,

    /// Preview or install the bundled agent recall skill without overwriting.
    InstallSkill {
        /// Agent host's skills directory; Praxis adds a praxis-recall child.
        #[arg(long, value_name = "DIRECTORY")]
        target: PathBuf,

        /// Apply the displayed installation instead of previewing it.
        #[arg(long)]
        apply: bool,
    },

    /// Discover an AoE-managed Codex transcript and print sanitized events.
    DebugParse {
        /// Stable AoE session identifier.
        #[arg(long)]
        session: String,

        /// AoE profile containing the session.
        #[arg(long, default_value = "default")]
        profile: String,

        /// Override AoE's config directory (primarily for fixture tests).
        #[arg(long, hide = true)]
        aoe_config_dir: Option<PathBuf>,

        /// Override CODEX_HOME (primarily for fixture tests).
        #[arg(long, hide = true)]
        codex_home: Option<PathBuf>,
    },

    /// Encrypt and retain sanitized events from an AoE-managed Codex session.
    Ingest {
        /// Stable AoE session identifier.
        #[arg(long)]
        session: String,

        /// AoE profile containing the session.
        #[arg(long, default_value = "default")]
        profile: String,

        /// Override AoE's config directory (primarily for fixture tests).
        #[arg(long, hide = true)]
        aoe_config_dir: Option<PathBuf>,

        /// Override CODEX_HOME (primarily for fixture tests).
        #[arg(long, hide = true)]
        codex_home: Option<PathBuf>,

        /// Override XDG_DATA_HOME (primarily for fixture tests).
        #[arg(long, hide = true)]
        data_home: Option<PathBuf>,
    },

    /// Return a bounded aggregate of procedural history for one project.
    Recall {
        /// Local project directory used only for encrypted identity lookup.
        #[arg(long)]
        project: PathBuf,

        /// Restrict observations to one controlled operation.
        #[arg(long)]
        operation: Option<OperationArg>,

        /// Include only failed historical attempts.
        #[arg(long)]
        failures: bool,

        /// Maximum aggregate observations to return (hard maximum: 20).
        #[arg(long, default_value_t = crate::query::DEFAULT_RECALL_LIMIT)]
        limit: usize,

        /// Ephemeral task text used only to rank controlled observations.
        #[arg(long)]
        query: Option<String>,

        /// Approximate maximum JSON output tokens (hard maximum: 1000).
        #[arg(long, default_value_t = crate::query::DEFAULT_TOKEN_BUDGET)]
        token_budget: usize,

        /// Override XDG_DATA_HOME (primarily for fixture tests).
        #[arg(long, hide = true)]
        data_home: Option<PathBuf>,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub(crate) enum OperationArg {
    Command,
    ContinueCommand,
    ApplyPatch,
    ReadFile,
    WriteFile,
    Search,
    WebRequest,
    InspectImage,
    UpdatePlan,
    Delegate,
    Wait,
    ToolCall,
}

impl From<OperationArg> for Operation {
    fn from(value: OperationArg) -> Self {
        match value {
            OperationArg::Command => Self::Command,
            OperationArg::ContinueCommand => Self::ContinueCommand,
            OperationArg::ApplyPatch => Self::ApplyPatch,
            OperationArg::ReadFile => Self::ReadFile,
            OperationArg::WriteFile => Self::WriteFile,
            OperationArg::Search => Self::Search,
            OperationArg::WebRequest => Self::WebRequest,
            OperationArg::InspectImage => Self::InspectImage,
            OperationArg::UpdatePlan => Self::UpdatePlan,
            OperationArg::Delegate => Self::Delegate,
            OperationArg::Wait => Self::Wait,
            OperationArg::ToolCall => Self::ToolCall,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skill_install_is_preview_only_unless_apply_is_explicit() {
        let preview =
            Cli::try_parse_from(["praxis", "install-skill", "--target", "/agent/skills"]).unwrap();
        let Command::InstallSkill { target, apply } = preview.command else {
            panic!("expected install-skill command");
        };
        assert_eq!(target, PathBuf::from("/agent/skills"));
        assert!(!apply);

        let applied = Cli::try_parse_from([
            "praxis",
            "install-skill",
            "--target",
            "/agent/skills",
            "--apply",
        ])
        .unwrap();
        let Command::InstallSkill { apply, .. } = applied.command else {
            panic!("expected install-skill command");
        };
        assert!(apply);
    }
}
