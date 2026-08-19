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
    /// Normalize one native hook event from stdin without retaining raw fields.
    DebugHook {
        /// Hook provider whose JSON is being supplied.
        #[arg(long, value_enum, default_value = "codex")]
        agent: HookAgentArg,
    },

    /// Silently encrypt and record one sanitized native hook event from stdin.
    RecordHook {
        /// Hook provider whose JSON is being supplied.
        #[arg(long, value_enum)]
        agent: HookAgentArg,

        /// Override XDG_DATA_HOME (primarily for fixture tests).
        #[arg(long, hide = true)]
        data_home: Option<PathBuf>,
    },

    /// Ingest the Codex session identified by an AoE status-hook environment.
    AoeHook,

    /// Print an AoE status-hook configuration snippet for manual installation.
    PrintAoeConfig,

    /// Print a Claude Code hook configuration snippet for manual merging.
    PrintClaudeConfig,

    /// Report aggregate local readiness without creating or repairing state.
    Status {
        /// Explicit AoE config to inspect without modifying.
        #[arg(long, value_name = "FILE")]
        aoe_config: Option<PathBuf>,

        /// Explicit Claude settings file to inspect without modifying.
        #[arg(long, value_name = "FILE")]
        claude_config: Option<PathBuf>,

        /// Override XDG_DATA_HOME (primarily for fixture tests).
        #[arg(long, hide = true)]
        data_home: Option<PathBuf>,
    },

    /// Preview or forget all encrypted history associated with one project.
    Forget {
        /// Existing local project directory whose history should be forgotten.
        #[arg(long)]
        project: PathBuf,

        /// Apply the deletion instead of previewing its aggregate count.
        #[arg(long)]
        apply: bool,

        /// Override XDG_DATA_HOME (primarily for fixture tests).
        #[arg(long, hide = true)]
        data_home: Option<PathBuf>,
    },

    /// Preview or prune encrypted history using one bounded retention policy.
    Prune {
        /// Delete events strictly older than this many days.
        #[arg(
            long,
            conflicts_with = "keep_recent",
            required_unless_present = "keep_recent"
        )]
        older_than_days: Option<u32>,

        /// Keep only this many newest events globally; zero selects all events.
        #[arg(
            long,
            conflicts_with = "older_than_days",
            required_unless_present = "older_than_days"
        )]
        keep_recent: Option<u64>,

        /// Apply deletion batches instead of previewing the aggregate count.
        #[arg(long)]
        apply: bool,

        /// Override XDG_DATA_HOME (primarily for fixture tests).
        #[arg(long, hide = true)]
        data_home: Option<PathBuf>,
    },

    /// Preview or add Praxis to an explicit global AoE config.
    InstallAoeHook {
        /// Global AoE config.toml file to update.
        #[arg(long, value_name = "FILE")]
        config: PathBuf,

        /// Apply the displayed changes instead of previewing them.
        #[arg(long)]
        apply: bool,
    },

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum HookAgentArg {
    Codex,
    Claude,
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
    fn debug_hook_defaults_to_codex_and_record_hook_requires_a_provider() {
        let debug = Cli::try_parse_from(["praxis", "debug-hook"]).unwrap();
        let Command::DebugHook { agent } = debug.command else {
            panic!("expected debug-hook command");
        };
        assert_eq!(agent, HookAgentArg::Codex);

        let record = Cli::try_parse_from(["praxis", "record-hook", "--agent", "claude"]).unwrap();
        let Command::RecordHook { agent, data_home } = record.command else {
            panic!("expected record-hook command");
        };
        assert_eq!(agent, HookAgentArg::Claude);
        assert!(data_home.is_none());
        assert!(Cli::try_parse_from(["praxis", "record-hook"]).is_err());
    }

    #[test]
    fn status_checks_only_explicit_provider_configs() {
        let status = Cli::try_parse_from([
            "praxis",
            "status",
            "--aoe-config",
            "/aoe/config.toml",
            "--claude-config",
            "/claude/settings.json",
        ])
        .unwrap();
        let Command::Status {
            aoe_config,
            claude_config,
            data_home,
        } = status.command
        else {
            panic!("expected status command");
        };
        assert_eq!(aoe_config, Some(PathBuf::from("/aoe/config.toml")));
        assert_eq!(claude_config, Some(PathBuf::from("/claude/settings.json")));
        assert!(data_home.is_none());
    }

    #[test]
    fn forgetting_is_preview_only_unless_apply_is_explicit() {
        let preview =
            Cli::try_parse_from(["praxis", "forget", "--project", "/private/project"]).unwrap();
        let Command::Forget { apply, .. } = preview.command else {
            panic!("expected forget command");
        };
        assert!(!apply);

        let applied = Cli::try_parse_from([
            "praxis",
            "forget",
            "--project",
            "/private/project",
            "--apply",
        ])
        .unwrap();
        let Command::Forget { apply, .. } = applied.command else {
            panic!("expected forget command");
        };
        assert!(apply);
    }

    #[test]
    fn pruning_requires_exactly_one_policy_and_previews_by_default() {
        let age = Cli::try_parse_from(["praxis", "prune", "--older-than-days", "30"]).unwrap();
        let Command::Prune {
            older_than_days,
            keep_recent,
            apply,
            ..
        } = age.command
        else {
            panic!("expected prune command");
        };
        assert_eq!(older_than_days, Some(30));
        assert_eq!(keep_recent, None);
        assert!(!apply);

        assert!(Cli::try_parse_from(["praxis", "prune"]).is_err());
        assert!(
            Cli::try_parse_from([
                "praxis",
                "prune",
                "--older-than-days",
                "30",
                "--keep-recent",
                "100",
            ])
            .is_err()
        );
    }

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

    #[test]
    fn aoe_hook_install_is_preview_only_unless_apply_is_explicit() {
        let preview =
            Cli::try_parse_from(["praxis", "install-aoe-hook", "--config", "/aoe/config.toml"])
                .unwrap();
        let Command::InstallAoeHook { config, apply } = preview.command else {
            panic!("expected install-aoe-hook command");
        };
        assert_eq!(config, PathBuf::from("/aoe/config.toml"));
        assert!(!apply);

        let applied = Cli::try_parse_from([
            "praxis",
            "install-aoe-hook",
            "--config",
            "/aoe/config.toml",
            "--apply",
        ])
        .unwrap();
        let Command::InstallAoeHook { apply, .. } = applied.command else {
            panic!("expected install-aoe-hook command");
        };
        assert!(apply);
    }
}
