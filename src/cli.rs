use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};

use crate::core::{
    ArtifactKind, Ecosystem, FailureReason, MemoryScope, Operation, Outcome, Phase, RiskShape,
    Strategy, TaskKind, ToolFamily,
};

#[derive(Parser)]
#[command(name = "regurgitate", version, about)]
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

    /// Record one verified outcome for a controlled procedural strategy.
    Learn {
        /// Local project directory used only for encrypted identity lookup.
        #[arg(long)]
        project: PathBuf,

        /// Controlled task category in which the strategy was evaluated.
        #[arg(long, value_enum)]
        task: TaskKindArg,

        /// Privacy-safe strategy whose outcome was directly verified.
        #[arg(long, value_enum)]
        strategy: StrategyArg,

        /// Known result; unknown outcomes cannot be explicitly learned.
        #[arg(long, value_enum)]
        outcome: LearnedOutcomeArg,

        /// Override XDG_DATA_HOME (primarily for fixture tests).
        #[arg(long, hide = true)]
        data_home: Option<PathBuf>,
    },

    /// Ingest the Codex session identified by an AoE status-hook environment.
    AoeHook,

    /// Print an AoE status-hook configuration snippet for manual installation.
    PrintAoeConfig,

    /// Print a Codex PostToolUse configuration snippet for manual installation.
    PrintCodexConfig,

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

        /// Explicit Codex config file to inspect without modifying.
        #[arg(long, value_name = "FILE")]
        codex_config: Option<PathBuf>,

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

    /// Preview or add Regurgitate to an explicit global AoE config.
    InstallAoeHook {
        /// Global AoE config.toml file to update.
        #[arg(long, value_name = "FILE")]
        config: PathBuf,

        /// Apply the displayed changes instead of previewing them.
        #[arg(long)]
        apply: bool,
    },

    /// Preview or add native recording to an explicit user Codex config.
    InstallCodexHook {
        /// User-level Codex config.toml file to update.
        #[arg(long, value_name = "FILE")]
        config: PathBuf,

        /// Apply the displayed changes instead of previewing them.
        #[arg(long)]
        apply: bool,
    },

    /// Preview or add native recording to an explicit Claude settings file.
    InstallClaudeHook {
        /// User-level Claude settings.json file to update.
        #[arg(long, value_name = "FILE")]
        config: PathBuf,

        /// Apply the displayed changes instead of previewing them.
        #[arg(long)]
        apply: bool,
    },

    /// Preview, install, or explicitly replace the bundled agent recall skill.
    InstallSkill {
        /// Agent host's skills directory; Regurgitate adds a regurgitate-recall child.
        #[arg(long, value_name = "DIRECTORY")]
        target: PathBuf,

        /// Apply the displayed installation instead of previewing it.
        #[arg(long)]
        apply: bool,

        /// Permit replacing a different existing skill; still previews unless --apply is set.
        #[arg(long)]
        replace: bool,
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

    /// Return task-matched learned practice plus a separate hook summary.
    Recall {
        /// Local project directory used only for encrypted identity lookup.
        #[arg(long)]
        project: PathBuf,

        /// Restrict lessons to procedures with one canonical operation.
        #[arg(long)]
        operation: Option<OperationArg>,

        /// Include only semantically failed learned practices.
        #[arg(long)]
        failures: bool,

        /// Maximum lessons to return (hard maximum: 20); dropped ones are counted in `omitted`.
        #[arg(long, default_value_t = crate::query::DEFAULT_RECALL_LIMIT)]
        limit: usize,

        /// Ephemeral task text reduced to controlled categories for filtering.
        #[arg(long)]
        query: Option<String>,

        /// Explicit controlled task category; outranks keyword inference.
        #[arg(long, value_enum)]
        task: Option<TaskKindArg>,

        /// Explicit phase: diagnose, mutate, verify, release, research, integration.
        #[arg(long)]
        phase: Option<Phase>,

        /// Explicit artifact kind: source, generated-source, native-cad, config, dataset, binary, docs.
        #[arg(long)]
        artifact: Option<ArtifactKind>,

        /// Explicit ecosystem such as rust, python, kicad, git, or generic.
        #[arg(long)]
        ecosystem: Option<Ecosystem>,

        /// Explicit tool family such as cargo, pytest, npm, kicad, or docker.
        #[arg(long)]
        tool_family: Option<ToolFamily>,

        /// Approximate maximum JSON output tokens (hard maximum: 1000).
        #[arg(long, default_value_t = crate::query::DEFAULT_TOKEN_BUDGET)]
        token_budget: usize,

        /// Override XDG_DATA_HOME (primarily for fixture tests).
        #[arg(long, hide = true)]
        data_home: Option<PathBuf>,
    },

    /// Record, confirm, list, or transition encrypted experience capsules.
    Experience {
        #[command(subcommand)]
        command: ExperienceCommand,
    },

    /// Emit a plain-text experience brief for host preflight injection.
    /// Unlike `recall`, only lessons with moderate or strong evidence are
    /// included; a prompt with no recognizable task produces no output.
    #[command(verbatim_doc_comment)]
    Preflight {
        /// Local project directory; read from the hook payload with --agent.
        #[arg(long, required_unless_present = "agent")]
        project: Option<PathBuf>,

        /// Ephemeral task text reduced to controlled categories.
        #[arg(long)]
        query: Option<String>,

        /// Explicit controlled task category.
        #[arg(long, value_enum)]
        task: Option<TaskKindArg>,

        /// Explicit phase.
        #[arg(long)]
        phase: Option<Phase>,

        /// Explicit artifact kind.
        #[arg(long)]
        artifact: Option<ArtifactKind>,

        /// Explicit ecosystem.
        #[arg(long)]
        ecosystem: Option<Ecosystem>,

        /// Explicit tool family.
        #[arg(long)]
        tool_family: Option<ToolFamily>,

        /// Approximate maximum brief tokens (hard maximum: 1000).
        #[arg(long, default_value_t = crate::query::DEFAULT_PREFLIGHT_TOKEN_BUDGET)]
        token_budget: usize,

        /// Read a host lifecycle payload from stdin and answer in its format.
        #[arg(long, value_enum)]
        agent: Option<PreflightAgentArg>,

        /// Override XDG_DATA_HOME (primarily for fixture tests).
        #[arg(long, hide = true)]
        data_home: Option<PathBuf>,
    },

    /// Summarize paired cold/warm benchmark runs from a JSONL file.
    BenchReport {
        /// JSONL file; one object per paired run (see benchmarks/README.md).
        #[arg(long)]
        runs: PathBuf,
    },
}

#[derive(Subcommand)]
pub(crate) enum ExperienceCommand {
    /// Record one experience capsule, or confirm an equivalent active one.
    Record {
        /// Local project directory used only for encrypted identity lookup.
        #[arg(long)]
        project: PathBuf,

        /// Relevance scope: project, workspace, ecosystem, machine, global.
        #[arg(long, default_value = "project")]
        scope: MemoryScope,

        /// Controlled task category in which the procedure was evaluated.
        #[arg(long, value_enum)]
        task: TaskKindArg,

        /// Condition under which the lesson applies (one bounded sentence, max 240 chars).
        #[arg(long)]
        situation: String,

        /// Actionable procedural conclusion (one bounded sentence, max 320 chars).
        #[arg(long)]
        lesson: String,

        /// Boundary or counterexample (one bounded sentence, max 160 chars).
        #[arg(long)]
        caveat: Option<String>,

        /// Comma-separated generic procedure dimensions (domain detail goes in --lesson).
        /// Mutation: structured-patch, direct-text-mutation, incremental-native-regeneration,
        /// bulk-change. Verification: targeted-verification, full-verification,
        /// native-verification. Execution: preview-then-apply, atomic-write, native-tool.
        /// Research: reproduce-then-compare, per-subject-streaming, resource-cap-first.
        /// Integration: native-hook, transcript-fallback.
        #[arg(long, verbatim_doc_comment)]
        procedure: String,

        /// Comma-separated ordered steps, max 6: inspect, reproduce, compare, patch,
        /// regenerate, preview, apply, verify-targeted, verify-full, verify-native, rollback.
        #[arg(long, verbatim_doc_comment)]
        steps: Option<String>,

        /// Whether the procedure produced a correct result.
        #[arg(long, value_enum)]
        outcome: LearnedOutcomeArg,

        /// Semantic reason a failed procedure was rejected.
        #[arg(long)]
        failure_reason: Option<FailureReason>,

        /// Applicability phase.
        #[arg(long)]
        phase: Option<Phase>,

        /// Applicability artifact kind.
        #[arg(long)]
        artifact: Option<ArtifactKind>,

        /// Applicability ecosystem (required for --scope ecosystem).
        #[arg(long)]
        ecosystem: Option<Ecosystem>,

        /// Applicability and environment tool family.
        #[arg(long)]
        tool_family: Option<ToolFamily>,

        /// Risk shapes, comma-separated or repeated: destructive, expensive, flaky,
        /// version-sensitive, sandbox-sensitive.
        #[arg(long = "risk", value_delimiter = ',')]
        risks: Vec<RiskShape>,

        /// Major version of the tool family the lesson was observed on.
        #[arg(long)]
        tool_major: Option<u16>,

        /// Override XDG_DATA_HOME (primarily for fixture tests).
        #[arg(long, hide = true)]
        data_home: Option<PathBuf>,
    },

    /// List capsule status and shape for this project; never lesson text.
    List {
        #[arg(long)]
        project: PathBuf,

        /// Maximum capsules to list.
        #[arg(long, default_value_t = 20)]
        limit: usize,

        #[arg(long, hide = true)]
        data_home: Option<PathBuf>,
    },

    /// Mark a capsule challenged by contradicting evidence.
    Challenge {
        #[arg(long)]
        project: PathBuf,

        /// Opaque capsule selector from `experience list`.
        #[arg(long = "match")]
        selector: String,

        #[arg(long, hide = true)]
        data_home: Option<PathBuf>,
    },

    /// Mark a capsule obsolete; it leaves normal recall.
    Obsolete {
        #[arg(long)]
        project: PathBuf,

        /// Opaque capsule selector from `experience list`.
        #[arg(long = "match")]
        selector: String,

        #[arg(long, hide = true)]
        data_home: Option<PathBuf>,
    },

    /// Replace one capsule with another; the old one is kept for audit only.
    Supersede {
        #[arg(long)]
        project: PathBuf,

        /// Selector of the capsule being replaced.
        #[arg(long)]
        old: String,

        /// Selector of the replacement capsule.
        #[arg(long)]
        new: String,

        #[arg(long, hide = true)]
        data_home: Option<PathBuf>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum PreflightAgentArg {
    Claude,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum HookAgentArg {
    Codex,
    Claude,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum LearnedOutcomeArg {
    Success,
    Failure,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum TaskKindArg {
    Configuration,
    DataImport,
    Debugging,
    DependencyUpdate,
    Documentation,
    FeatureImplementation,
    Integration,
    Performance,
    Refactoring,
    Release,
    Research,
    Security,
    Testing,
}

impl From<TaskKindArg> for TaskKind {
    fn from(value: TaskKindArg) -> Self {
        match value {
            TaskKindArg::Configuration => Self::Configuration,
            TaskKindArg::DataImport => Self::DataImport,
            TaskKindArg::Debugging => Self::Debugging,
            TaskKindArg::DependencyUpdate => Self::DependencyUpdate,
            TaskKindArg::Documentation => Self::Documentation,
            TaskKindArg::FeatureImplementation => Self::FeatureImplementation,
            TaskKindArg::Integration => Self::Integration,
            TaskKindArg::Performance => Self::Performance,
            TaskKindArg::Refactoring => Self::Refactoring,
            TaskKindArg::Release => Self::Release,
            TaskKindArg::Research => Self::Research,
            TaskKindArg::Security => Self::Security,
            TaskKindArg::Testing => Self::Testing,
        }
    }
}

impl From<LearnedOutcomeArg> for Outcome {
    fn from(value: LearnedOutcomeArg) -> Self {
        match value {
            LearnedOutcomeArg::Success => Self::Success,
            LearnedOutcomeArg::Failure => Self::Failure,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum StrategyArg {
    StructuredPatch,
    DirectTextMutation,
    IncrementalNativeRegeneration,
    BulkChange,
    AtomicWrite,
    PreviewThenApply,
    TargetedVerification,
    FullVerification,
    NativeHook,
    TranscriptFallback,
    ReproduceThenCompare,
    PerSubjectStreaming,
    ResourceCapFirst,
}

impl From<StrategyArg> for Strategy {
    fn from(value: StrategyArg) -> Self {
        match value {
            StrategyArg::StructuredPatch => Self::StructuredPatch,
            StrategyArg::DirectTextMutation => Self::DirectTextMutation,
            StrategyArg::IncrementalNativeRegeneration => Self::IncrementalNativeRegeneration,
            StrategyArg::BulkChange => Self::BulkChange,
            StrategyArg::AtomicWrite => Self::AtomicWrite,
            StrategyArg::PreviewThenApply => Self::PreviewThenApply,
            StrategyArg::TargetedVerification => Self::TargetedVerification,
            StrategyArg::FullVerification => Self::FullVerification,
            StrategyArg::NativeHook => Self::NativeHook,
            StrategyArg::TranscriptFallback => Self::TranscriptFallback,
            StrategyArg::ReproduceThenCompare => Self::ReproduceThenCompare,
            StrategyArg::PerSubjectStreaming => Self::PerSubjectStreaming,
            StrategyArg::ResourceCapFirst => Self::ResourceCapFirst,
        }
    }
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
    Analyze,
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
            OperationArg::Analyze => Self::Analyze,
            OperationArg::ToolCall => Self::ToolCall,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn experience_record_parses_controlled_vocabularies_only() {
        let cli = Cli::try_parse_from([
            "regurgitate",
            "experience",
            "record",
            "--project",
            "/private/project",
            "--scope",
            "ecosystem",
            "--task",
            "debugging",
            "--situation",
            "Generated native artifact; parser acceptance is not authoritative.",
            "--lesson",
            "Change one placement class at a time and run native verification.",
            "--procedure",
            "incremental-native-regeneration,native-verification",
            "--steps",
            "regenerate,verify-native",
            "--outcome",
            "failure",
            "--failure-reason",
            "verification-failed",
            "--phase",
            "verify",
            "--artifact",
            "native-cad",
            "--ecosystem",
            "kicad",
            "--tool-family",
            "kicad",
            "--risk",
            "version-sensitive,expensive",
            "--tool-major",
            "10",
        ])
        .unwrap();
        let Command::Experience {
            command:
                ExperienceCommand::Record {
                    scope,
                    failure_reason,
                    phase,
                    ecosystem,
                    risks,
                    tool_major,
                    ..
                },
        } = cli.command
        else {
            panic!("expected experience record");
        };
        assert_eq!(scope, MemoryScope::Ecosystem);
        assert_eq!(failure_reason, Some(FailureReason::VerificationFailed));
        assert_eq!(phase, Some(Phase::Verify));
        assert_eq!(ecosystem, Some(Ecosystem::Kicad));
        assert_eq!(
            risks,
            vec![RiskShape::VersionSensitive, RiskShape::Expensive]
        );
        assert_eq!(tool_major, Some(10));

        assert!(
            Cli::try_parse_from([
                "regurgitate",
                "experience",
                "record",
                "--project",
                "/p",
                "--task",
                "debugging",
                "--situation",
                "a b c",
                "--lesson",
                "d e f",
                "--procedure",
                "structured-patch",
                "--outcome",
                "success",
                "--phase",
                "arbitrary-phase",
            ])
            .is_err()
        );
    }

    #[test]
    fn recall_accepts_explicit_metadata_and_preflight_needs_a_project_or_agent() {
        let cli = Cli::try_parse_from([
            "regurgitate",
            "recall",
            "--project",
            "/p",
            "--task",
            "testing",
            "--ecosystem",
            "rust",
            "--tool-family",
            "cargo",
        ])
        .unwrap();
        let Command::Recall {
            task,
            ecosystem,
            tool_family,
            ..
        } = cli.command
        else {
            panic!("expected recall");
        };
        assert_eq!(task, Some(TaskKindArg::Testing));
        assert_eq!(ecosystem, Some(Ecosystem::Rust));
        assert_eq!(tool_family, Some(ToolFamily::Cargo));

        assert!(Cli::try_parse_from(["regurgitate", "preflight"]).is_err());
        assert!(Cli::try_parse_from(["regurgitate", "preflight", "--agent", "claude"]).is_ok());
        let cli = Cli::try_parse_from(["regurgitate", "preflight", "--project", "/p"]).unwrap();
        let Command::Preflight { token_budget, .. } = cli.command else {
            panic!("expected preflight");
        };
        assert_eq!(token_budget, crate::query::DEFAULT_PREFLIGHT_TOKEN_BUDGET);
    }

    #[test]
    fn debug_hook_defaults_to_codex_and_record_hook_requires_a_provider() {
        let debug = Cli::try_parse_from(["regurgitate", "debug-hook"]).unwrap();
        let Command::DebugHook { agent } = debug.command else {
            panic!("expected debug-hook command");
        };
        assert_eq!(agent, HookAgentArg::Codex);

        let record =
            Cli::try_parse_from(["regurgitate", "record-hook", "--agent", "claude"]).unwrap();
        let Command::RecordHook { agent, data_home } = record.command else {
            panic!("expected record-hook command");
        };
        assert_eq!(agent, HookAgentArg::Claude);
        assert!(data_home.is_none());
        assert!(Cli::try_parse_from(["regurgitate", "record-hook"]).is_err());
    }

    #[test]
    fn learning_requires_one_controlled_strategy_and_known_outcome() {
        let cli = Cli::try_parse_from([
            "regurgitate",
            "learn",
            "--project",
            "/private/project",
            "--task",
            "configuration",
            "--strategy",
            "atomic-write",
            "--outcome",
            "success",
        ])
        .unwrap();
        let Command::Learn {
            project,
            task,
            strategy,
            outcome,
            data_home,
        } = cli.command
        else {
            panic!("expected learn command");
        };
        assert_eq!(project, PathBuf::from("/private/project"));
        assert_eq!(task, TaskKindArg::Configuration);
        assert_eq!(strategy, StrategyArg::AtomicWrite);
        assert_eq!(outcome, LearnedOutcomeArg::Success);
        assert!(data_home.is_none());
        assert!(
            Cli::try_parse_from([
                "regurgitate",
                "learn",
                "--project",
                "/private/project",
                "--strategy",
                "atomic-write",
                "--outcome",
                "success",
            ])
            .is_err()
        );
        assert!(
            Cli::try_parse_from([
                "regurgitate",
                "learn",
                "--project",
                "/private/project",
                "--task",
                "configuration",
                "--strategy",
                "arbitrary-private-label",
                "--outcome",
                "success",
            ])
            .is_err()
        );
        assert!(
            Cli::try_parse_from([
                "regurgitate",
                "learn",
                "--project",
                "/private/project",
                "--task",
                "configuration",
                "--strategy",
                "other",
                "--outcome",
                "success",
            ])
            .is_err()
        );
        for strategy in [
            "reproduce-then-compare",
            "per-subject-streaming",
            "resource-cap-first",
        ] {
            assert!(
                Cli::try_parse_from([
                    "regurgitate",
                    "learn",
                    "--project",
                    "/private/project",
                    "--task",
                    "research",
                    "--strategy",
                    strategy,
                    "--outcome",
                    "success",
                ])
                .is_ok()
            );
        }
    }

    #[test]
    fn status_checks_only_explicit_provider_configs() {
        let status = Cli::try_parse_from([
            "regurgitate",
            "status",
            "--aoe-config",
            "/aoe/config.toml",
            "--claude-config",
            "/claude/settings.json",
            "--codex-config",
            "/codex/config.toml",
        ])
        .unwrap();
        let Command::Status {
            aoe_config,
            claude_config,
            codex_config,
            data_home,
        } = status.command
        else {
            panic!("expected status command");
        };
        assert_eq!(aoe_config, Some(PathBuf::from("/aoe/config.toml")));
        assert_eq!(claude_config, Some(PathBuf::from("/claude/settings.json")));
        assert_eq!(codex_config, Some(PathBuf::from("/codex/config.toml")));
        assert!(data_home.is_none());
    }

    #[test]
    fn forgetting_is_preview_only_unless_apply_is_explicit() {
        let preview =
            Cli::try_parse_from(["regurgitate", "forget", "--project", "/private/project"])
                .unwrap();
        let Command::Forget { apply, .. } = preview.command else {
            panic!("expected forget command");
        };
        assert!(!apply);

        let applied = Cli::try_parse_from([
            "regurgitate",
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
        let age = Cli::try_parse_from(["regurgitate", "prune", "--older-than-days", "30"]).unwrap();
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

        assert!(Cli::try_parse_from(["regurgitate", "prune"]).is_err());
        assert!(
            Cli::try_parse_from([
                "regurgitate",
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
            Cli::try_parse_from(["regurgitate", "install-skill", "--target", "/agent/skills"])
                .unwrap();
        let Command::InstallSkill {
            target,
            apply,
            replace,
        } = preview.command
        else {
            panic!("expected install-skill command");
        };
        assert_eq!(target, PathBuf::from("/agent/skills"));
        assert!(!apply);
        assert!(!replace);

        let applied = Cli::try_parse_from([
            "regurgitate",
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

        let replacement = Cli::try_parse_from([
            "regurgitate",
            "install-skill",
            "--target",
            "/agent/skills",
            "--replace",
        ])
        .unwrap();
        let Command::InstallSkill { apply, replace, .. } = replacement.command else {
            panic!("expected install-skill command");
        };
        assert!(!apply);
        assert!(replace);
    }

    #[test]
    fn aoe_hook_install_is_preview_only_unless_apply_is_explicit() {
        let preview = Cli::try_parse_from([
            "regurgitate",
            "install-aoe-hook",
            "--config",
            "/aoe/config.toml",
        ])
        .unwrap();
        let Command::InstallAoeHook { config, apply } = preview.command else {
            panic!("expected install-aoe-hook command");
        };
        assert_eq!(config, PathBuf::from("/aoe/config.toml"));
        assert!(!apply);

        let applied = Cli::try_parse_from([
            "regurgitate",
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

    #[test]
    fn codex_hook_install_is_preview_only_unless_apply_is_explicit() {
        let preview = Cli::try_parse_from([
            "regurgitate",
            "install-codex-hook",
            "--config",
            "/codex/config.toml",
        ])
        .unwrap();
        let Command::InstallCodexHook { config, apply } = preview.command else {
            panic!("expected install-codex-hook command");
        };
        assert_eq!(config, PathBuf::from("/codex/config.toml"));
        assert!(!apply);

        let applied = Cli::try_parse_from([
            "regurgitate",
            "install-codex-hook",
            "--config",
            "/codex/config.toml",
            "--apply",
        ])
        .unwrap();
        let Command::InstallCodexHook { apply, .. } = applied.command else {
            panic!("expected install-codex-hook command");
        };
        assert!(apply);
    }

    #[test]
    fn claude_hook_install_is_preview_only_unless_apply_is_explicit() {
        let preview = Cli::try_parse_from([
            "regurgitate",
            "install-claude-hook",
            "--config",
            "/claude/settings.json",
        ])
        .unwrap();
        let Command::InstallClaudeHook { config, apply } = preview.command else {
            panic!("expected install-claude-hook command");
        };
        assert_eq!(config, PathBuf::from("/claude/settings.json"));
        assert!(!apply);

        let applied = Cli::try_parse_from([
            "regurgitate",
            "install-claude-hook",
            "--config",
            "/claude/settings.json",
            "--apply",
        ])
        .unwrap();
        let Command::InstallClaudeHook { apply, .. } = applied.command else {
            panic!("expected install-claude-hook command");
        };
        assert!(apply);
    }
}
