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
