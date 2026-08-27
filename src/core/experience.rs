//! Schema v3: the encrypted experience capsule.
//!
//! A capsule is a deliberately authored, bounded summary of *when* a
//! procedure applies, not a record of the work that produced the lesson. Every
//! field is either a controlled enum, a bounded admitted sentence, a
//! timestamp, or an opaque local identifier.

use std::{collections::BTreeSet, fmt, str::FromStr};

use anyhow::{Result, bail};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize};
use uuid::Uuid;

use super::{
    AgentKind, Capability, Operation, Strategy, TaskKind,
    admission::{AdmissionRejection, admit_text},
};

pub const EXPERIENCE_SCHEMA_VERSION: u32 = 4;
pub const SITUATION_MAX_CHARS: usize = 240;
pub const LESSON_MAX_CHARS: usize = 320;
pub const CAVEAT_MAX_CHARS: usize = 160;
pub const MAX_PROCEDURE_STEPS: usize = 6;
/// Evidence entries retained per capsule. Older entries are dropped first;
/// the capsule's `created_at` still records when the lesson first appeared.
pub const MAX_EVIDENCE_ENTRIES: usize = 64;

/// A sentence admitted through [`admit_text`] and capped at `N` characters.
/// Deserialization re-validates so a tampered or legacy payload cannot bypass
/// the cap or the structural checks.
#[derive(Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct BoundedText<const N: usize>(String);

impl<const N: usize> BoundedText<N> {
    pub fn new(text: &str) -> Result<Self, AdmissionRejection> {
        admit_text(text, N)?;
        Ok(Self(text.trim().to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub const fn max_chars() -> usize {
        N
    }
}

impl<const N: usize> fmt::Debug for BoundedText<N> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "BoundedText<{N}>({} chars)",
            self.0.chars().count()
        )
    }
}

impl<const N: usize> fmt::Display for BoundedText<N> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl<'de, const N: usize> Deserialize<'de> for BoundedText<N> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Self::new(&text).map_err(serde::de::Error::custom)
    }
}

pub type Situation = BoundedText<SITUATION_MAX_CHARS>;
pub type Lesson = BoundedText<LESSON_MAX_CHARS>;
pub type Caveat = BoundedText<CAVEAT_MAX_CHARS>;

macro_rules! controlled_enum {
    ($(#[$meta:meta])* $name:ident { $($variant:ident => $label:literal),+ $(,)? }) => {
        controlled_enum!($(#[$meta])* $name { $($variant => $label),+ } aliases {});
    };
    (
        $(#[$meta:meta])* $name:ident { $($variant:ident => $label:literal),+ $(,)? }
        aliases { $($alias:literal => $target:ident),* $(,)? }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(rename_all = "snake_case")]
        pub enum $name { $($variant),+ }

        impl $name {
            pub const ALL: &'static [Self] = &[$(Self::$variant),+];

            pub const fn label(self) -> &'static str {
                match self { $(Self::$variant => $label),+ }
            }
        }

        impl FromStr for $name {
            type Err = anyhow::Error;

            fn from_str(value: &str) -> Result<Self> {
                let normalized = value.trim().to_ascii_lowercase().replace('_', "-");
                $(if normalized == $label { return Ok(Self::$variant); })+
                $(if normalized == $alias { return Ok(Self::$target); })*
                bail!(
                    "unknown {} {:?}; expected one of: {}",
                    stringify!($name),
                    value,
                    Self::ALL.iter().map(|item| item.label()).collect::<Vec<_>>().join(", ")
                )
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(self.label())
            }
        }
    };
}

controlled_enum! {
    /// Relevance prior, not access control. Encryption remains the boundary.
    MemoryScope {
        Project => "project",
        Workspace => "workspace",
        Ecosystem => "ecosystem",
        Machine => "machine",
        Global => "global",
    }
}

controlled_enum! {
    ArtifactKind {
        Source => "source",
        GeneratedSource => "generated-source",
        NativeCad => "native-cad",
        Config => "config",
        Dataset => "dataset",
        Binary => "binary",
        Docs => "docs",
    }
}

controlled_enum! {
    Phase {
        Diagnose => "diagnose",
        Mutate => "mutate",
        Verify => "verify",
        Release => "release",
        Research => "research",
        Integration => "integration",
    }
}

controlled_enum! {
    Ecosystem {
        Rust => "rust",
        Python => "python",
        Javascript => "javascript",
        Typescript => "typescript",
        Go => "go",
        Java => "java",
        Dotnet => "dotnet",
        C => "c",
        Cpp => "cpp",
        Shell => "shell",
        Php => "php",
        Laravel => "laravel",
        Ruby => "ruby",
        Cuda => "cuda",
        Kicad => "kicad",
        Git => "git",
        Docker => "docker",
        Sql => "sql",
        Generic => "generic",
    }
}

controlled_enum! {
    ToolFamily {
        Cargo => "cargo",
        Pytest => "pytest",
        Npm => "npm",
        Pnpm => "pnpm",
        Yarn => "yarn",
        Jest => "jest",
        Make => "make",
        Cmake => "cmake",
        Gcc => "gcc",
        Clang => "clang",
        Nvcc => "nvcc",
        Git => "git",
        Docker => "docker",
        Kubectl => "kubectl",
        Terraform => "terraform",
        Kicad => "kicad",
        Playwright => "playwright",
        Other => "other",
    }
    aliases {
        "generic" => Other,
        "none" => Other,
    }
}

controlled_enum! {
    RiskShape {
        Destructive => "destructive",
        Expensive => "expensive",
        Flaky => "flaky",
        VersionSensitive => "version-sensitive",
        SandboxSensitive => "sandbox-sensitive",
    }
}

controlled_enum! {
    HostClass {
        Linux => "linux",
        Macos => "macos",
        Windows => "windows",
        Container => "container",
        Ci => "ci",
    }
}

impl HostClass {
    /// The class of the machine running this process; never an identifier.
    pub fn current() -> Option<Self> {
        if std::env::var_os("CI").is_some() {
            return Some(Self::Ci);
        }
        if cfg!(target_os = "linux") {
            Some(Self::Linux)
        } else if cfg!(target_os = "macos") {
            Some(Self::Macos)
        } else if cfg!(target_os = "windows") {
            Some(Self::Windows)
        } else {
            None
        }
    }
}

controlled_enum! {
    /// Semantic reason a procedure was rejected. Distinct from provider
    /// `ErrorClass`, which is operational telemetry.
    FailureReason {
        IncorrectResult => "incorrect-result",
        VerificationFailed => "verification-failed",
        InvalidArtifact => "invalid-artifact",
        RegressionIntroduced => "regression-introduced",
        Incomplete => "incomplete",
        TooBroad => "too-broad",
        TooSlow => "too-slow",
        ToolMismatch => "tool-mismatch",
        EnvironmentMismatch => "environment-mismatch",
        VersionMismatch => "version-mismatch",
        Other => "other",
    }
}

controlled_enum! {
    MemoryLifecycle {
        Active => "active",
        Challenged => "challenged",
        Superseded => "superseded",
        Obsolete => "obsolete",
    }
}

/// Recovery bookkeeping for a challenged capsule. Evidence recorded before
/// `challenged_at` is never eligible to reactivate it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChallengeState {
    pub challenged_at: DateTime<Utc>,
    pub supporting_outcome: SemanticOutcome,
    pub recovery_evidence: u8,
}

controlled_enum! {
    SemanticOutcome {
        Success => "success",
        Failure => "failure",
    }
}

controlled_enum! {
    /// The actor or trusted integration that supplied an evidence claim.
    EvidenceSource {
        AgentJudgment => "agent-judgment",
        HostObservation => "host-observation",
        HumanConfirmation => "human-confirmation",
    }
}

impl Default for EvidenceSource {
    fn default() -> Self {
        Self::AgentJudgment
    }
}

controlled_enum! {
    /// Bounded verification claim associated with one observation.
    EvidenceVerification {
        None => "none",
        Targeted => "targeted",
        Full => "full",
        Native => "native",
    }
}

impl Default for EvidenceVerification {
    fn default() -> Self {
        Self::None
    }
}

controlled_enum! {
    /// Trust boundary for a verification claim. Agent-supplied evidence is
    /// always self-reported; only trusted host and human entry points may
    /// create stronger attestations.
    EvidenceAttestation {
        SelfReported => "self-reported",
        HostAttested => "host-attested",
        HumanAttested => "human-attested",
    }
}

impl Default for EvidenceAttestation {
    fn default() -> Self {
        Self::SelfReported
    }
}

controlled_enum! {
    MutationMode {
        StructuredPatch => "structured-patch",
        DirectTextMutation => "direct-text-mutation",
        IncrementalNativeRegeneration => "incremental-native-regeneration",
        BulkChange => "bulk-change",
    }
}

controlled_enum! {
    VerificationMode {
        Targeted => "targeted-verification",
        Full => "full-verification",
        Native => "native-verification",
    }
}

controlled_enum! {
    ExecutionMode {
        PreviewThenApply => "preview-then-apply",
        AtomicWrite => "atomic-write",
        NativeTool => "native-tool",
    }
}

controlled_enum! {
    ResearchMode {
        ReproduceThenCompare => "reproduce-then-compare",
        PerSubjectStreaming => "per-subject-streaming",
        ResourceCapFirst => "resource-cap-first",
    }
}

controlled_enum! {
    IntegrationMode {
        NativeHook => "native-hook",
        TranscriptFallback => "transcript-fallback",
    }
}

controlled_enum! {
    ProcedureStep {
        Inspect => "inspect",
        Reproduce => "reproduce",
        Compare => "compare",
        Patch => "patch",
        Regenerate => "regenerate",
        Preview => "preview",
        Apply => "apply",
        VerifyTargeted => "verify-targeted",
        VerifyFull => "verify-full",
        VerifyNative => "verify-native",
        Rollback => "rollback",
        Hypothesize => "hypothesize",
        Classify => "classify",
        Annotate => "annotate",
    }
}

/// A compositional description of how work was done. Dimensions are
/// orthogonal; `steps` carries order when order matters.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Procedure {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mutation: Option<MutationMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verification: Option<VerificationMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution: Option<ExecutionMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub research: Option<ResearchMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub integration: Option<IntegrationMode>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub steps: Vec<ProcedureStep>,
}

impl Procedure {
    pub fn is_empty(&self) -> bool {
        self.mutation.is_none()
            && self.verification.is_none()
            && self.execution.is_none()
            && self.research.is_none()
            && self.integration.is_none()
            && self.steps.is_empty()
    }

    pub fn validate(&self) -> Result<()> {
        if self.is_empty() {
            bail!("a procedure needs at least one dimension or step");
        }
        if self.steps.len() > MAX_PROCEDURE_STEPS {
            bail!("a procedure may list at most {MAX_PROCEDURE_STEPS} steps");
        }
        Ok(())
    }

    /// Parses a comma-separated list of dimension labels. Each dimension may
    /// appear at most once; steps are supplied separately.
    pub fn parse_dimensions(value: &str) -> Result<Self> {
        let mut procedure = Self::default();
        for label in value
            .split(',')
            .map(str::trim)
            .filter(|label| !label.is_empty())
        {
            if let Ok(mode) = label.parse::<MutationMode>() {
                set_once(&mut procedure.mutation, mode, "mutation")?;
            } else if let Ok(mode) = label.parse::<VerificationMode>() {
                set_once(&mut procedure.verification, mode, "verification")?;
            } else if let Ok(mode) = label.parse::<ExecutionMode>() {
                set_once(&mut procedure.execution, mode, "execution")?;
            } else if let Ok(mode) = label.parse::<ResearchMode>() {
                set_once(&mut procedure.research, mode, "research")?;
            } else if let Ok(mode) = label.parse::<IntegrationMode>() {
                set_once(&mut procedure.integration, mode, "integration")?;
            } else {
                bail!(
                    "unknown procedure dimension {label:?}; the vocabulary is fixed and \
                     generic (put domain detail in the lesson text). Valid labels: {}",
                    Self::dimension_labels().join(", ")
                );
            }
        }
        Ok(procedure)
    }

    /// Every accepted dimension label, for help and error text.
    pub fn dimension_labels() -> Vec<&'static str> {
        let mut labels = Vec::new();
        labels.extend(MutationMode::ALL.iter().map(|mode| mode.label()));
        labels.extend(VerificationMode::ALL.iter().map(|mode| mode.label()));
        labels.extend(ExecutionMode::ALL.iter().map(|mode| mode.label()));
        labels.extend(ResearchMode::ALL.iter().map(|mode| mode.label()));
        labels.extend(IntegrationMode::ALL.iter().map(|mode| mode.label()));
        labels
    }

    pub fn parse_steps(value: &str) -> Result<Vec<ProcedureStep>> {
        value
            .split(',')
            .map(str::trim)
            .filter(|label| !label.is_empty())
            .map(str::parse)
            .collect()
    }

    /// Migration mapping from the v2 single-label strategy vocabulary.
    pub fn from_strategy(strategy: Strategy) -> Self {
        let mut procedure = Self::default();
        match strategy {
            Strategy::StructuredPatch => procedure.mutation = Some(MutationMode::StructuredPatch),
            Strategy::DirectTextMutation => {
                procedure.mutation = Some(MutationMode::DirectTextMutation);
            }
            Strategy::IncrementalNativeRegeneration => {
                procedure.mutation = Some(MutationMode::IncrementalNativeRegeneration);
            }
            Strategy::BulkChange => procedure.mutation = Some(MutationMode::BulkChange),
            Strategy::TargetedVerification => {
                procedure.verification = Some(VerificationMode::Targeted);
            }
            Strategy::FullVerification => procedure.verification = Some(VerificationMode::Full),
            Strategy::NativeTool => procedure.execution = Some(ExecutionMode::NativeTool),
            Strategy::AtomicWrite => procedure.execution = Some(ExecutionMode::AtomicWrite),
            Strategy::PreviewThenApply => {
                procedure.execution = Some(ExecutionMode::PreviewThenApply);
                procedure.steps = vec![ProcedureStep::Preview, ProcedureStep::Apply];
            }
            Strategy::ReproduceThenCompare => {
                procedure.research = Some(ResearchMode::ReproduceThenCompare);
                procedure.steps = vec![ProcedureStep::Reproduce, ProcedureStep::Compare];
            }
            Strategy::PerSubjectStreaming => {
                procedure.research = Some(ResearchMode::PerSubjectStreaming);
            }
            Strategy::ResourceCapFirst => procedure.research = Some(ResearchMode::ResourceCapFirst),
            Strategy::NativeHook => procedure.integration = Some(IntegrationMode::NativeHook),
            Strategy::TranscriptFallback => {
                procedure.integration = Some(IntegrationMode::TranscriptFallback);
            }
            Strategy::Other => procedure.execution = Some(ExecutionMode::NativeTool),
        }
        procedure
    }

    /// The dominant v2 strategy label, used for legacy-compatible filtering
    /// and output. Composite procedures report their most specific dimension.
    pub fn legacy_strategy(&self) -> Strategy {
        if let Some(mutation) = self.mutation {
            return match mutation {
                MutationMode::StructuredPatch => Strategy::StructuredPatch,
                MutationMode::DirectTextMutation => Strategy::DirectTextMutation,
                MutationMode::IncrementalNativeRegeneration => {
                    Strategy::IncrementalNativeRegeneration
                }
                MutationMode::BulkChange => Strategy::BulkChange,
            };
        }
        if let Some(research) = self.research {
            return match research {
                ResearchMode::ReproduceThenCompare => Strategy::ReproduceThenCompare,
                ResearchMode::PerSubjectStreaming => Strategy::PerSubjectStreaming,
                ResearchMode::ResourceCapFirst => Strategy::ResourceCapFirst,
            };
        }
        if let Some(verification) = self.verification {
            return match verification {
                VerificationMode::Targeted => Strategy::TargetedVerification,
                VerificationMode::Full => Strategy::FullVerification,
                VerificationMode::Native => Strategy::NativeTool,
            };
        }
        if let Some(execution) = self.execution {
            return match execution {
                ExecutionMode::PreviewThenApply => Strategy::PreviewThenApply,
                ExecutionMode::AtomicWrite => Strategy::AtomicWrite,
                ExecutionMode::NativeTool => Strategy::NativeTool,
            };
        }
        if let Some(integration) = self.integration {
            return match integration {
                IntegrationMode::NativeHook => Strategy::NativeHook,
                IntegrationMode::TranscriptFallback => Strategy::TranscriptFallback,
            };
        }
        Strategy::Other
    }

    /// Canonical capability/operation pair, reusing the v2 mapping so legacy
    /// `--operation` filters keep working.
    pub fn classification(&self) -> (Capability, Operation) {
        self.legacy_strategy().practice_classification()
    }

    /// Compact human label such as `structured-patch + targeted-verification
    /// (patch -> verify-targeted)`.
    pub fn summary(&self) -> String {
        let mut dimensions = Vec::new();
        if let Some(mode) = self.mutation {
            dimensions.push(mode.label());
        }
        if let Some(mode) = self.verification {
            dimensions.push(mode.label());
        }
        if let Some(mode) = self.execution {
            dimensions.push(mode.label());
        }
        if let Some(mode) = self.research {
            dimensions.push(mode.label());
        }
        if let Some(mode) = self.integration {
            dimensions.push(mode.label());
        }
        let mut summary = dimensions.join(" + ");
        if !self.steps.is_empty() {
            let steps: Vec<&str> = self.steps.iter().map(|step| step.label()).collect();
            if summary.is_empty() {
                summary = steps.join(" -> ");
            } else {
                summary.push_str(" (");
                summary.push_str(&steps.join(" -> "));
                summary.push(')');
            }
        }
        summary
    }
}

fn set_once<T: Copy>(slot: &mut Option<T>, value: T, dimension: &str) -> Result<()> {
    if slot.is_some() {
        bail!("procedure dimension {dimension} may only be given once");
    }
    *slot = Some(value);
    Ok(())
}

/// Controlled tags used for coarse retrieval. Arbitrary tags are not
/// accepted; this must not become the new leakage path.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApplicabilityTags {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_kind: Option<ArtifactKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phase: Option<Phase>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ecosystem: Option<Ecosystem>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_family: Option<ToolFamily>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub risk_shapes: BTreeSet<RiskShape>,
}

/// Minimal compatibility hints. Enough to distinguish "worked on KiCad 10"
/// from a timeless rule; never an environment dump.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnvironmentFingerprint {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_family: Option<ToolFamily>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub major_version: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host_class: Option<HostClass>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceEntry {
    pub at: DateTime<Utc>,
    pub outcome: SemanticOutcome,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure_reason: Option<FailureReason>,
    #[serde(default)]
    pub source: EvidenceSource,
    #[serde(default)]
    pub verification: EvidenceVerification,
    #[serde(default)]
    pub attestation: EvidenceAttestation,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<AgentKind>,
    #[serde(default)]
    pub environment: EnvironmentFingerprint,
    /// Digest of a confirmation receipt. It is encrypted with the capsule and
    /// exists only to make one recalled reference idempotent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub receipt_digest: Option<[u8; 32]>,
    /// Opaque grouping token for observations that may share one underlying
    /// run. Absence is treated conservatively by the evidence policy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cohort: Option<[u8; 32]>,
}

impl EvidenceEntry {
    pub fn agent_reported(
        at: DateTime<Utc>,
        outcome: SemanticOutcome,
        failure_reason: Option<FailureReason>,
        environment: EnvironmentFingerprint,
    ) -> Self {
        Self {
            at,
            outcome,
            failure_reason,
            source: EvidenceSource::AgentJudgment,
            verification: EvidenceVerification::None,
            attestation: EvidenceAttestation::SelfReported,
            agent: None,
            environment,
            receipt_digest: None,
            cohort: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExperienceCapsule {
    pub id: Uuid,
    /// Project in which the capsule was recorded. Forgetting that project
    /// removes the capsule regardless of scope.
    pub project_id: Uuid,
    pub scope: MemoryScope,
    /// Identity the scope token is derived from: the project id for
    /// `project`, the workspace id for `workspace`, otherwise absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope_id: Option<Uuid>,
    pub task: TaskKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub situation: Option<Situation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lesson: Option<Lesson>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caveat: Option<Caveat>,
    pub procedure: Procedure,
    #[serde(default)]
    pub applicability: ApplicabilityTags,
    pub lifecycle: MemoryLifecycle,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub challenge: Option<ChallengeState>,
    pub evidence: Vec<EvidenceEntry>,
    pub created_at: DateTime<Utc>,
    pub last_confirmed_at: DateTime<Utc>,
    pub schema_version: u32,
}

/// Controlled identity used for deduplication and clustering. Two capsules
/// with equal identity describe the same procedure in the same region.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ExperienceIdentity {
    pub task: TaskKind,
    pub procedure_summary: String,
    pub applicability: String,
}

impl ExperienceCapsule {
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != EXPERIENCE_SCHEMA_VERSION {
            bail!(
                "unsupported experience schema version {}",
                self.schema_version
            );
        }
        self.procedure.validate()?;
        match self.scope {
            MemoryScope::Project => {
                if self.scope_id != Some(self.project_id) {
                    bail!("a project-scoped capsule must be anchored to its project");
                }
            }
            MemoryScope::Workspace => {
                if self.scope_id.is_none() {
                    bail!("a workspace-scoped capsule needs a workspace identity");
                }
            }
            MemoryScope::Ecosystem => {
                if self.applicability.ecosystem.is_none() {
                    bail!("an ecosystem-scoped capsule needs an ecosystem tag");
                }
                if self.scope_id.is_some() {
                    bail!("an ecosystem-scoped capsule must not carry a scope identity");
                }
            }
            MemoryScope::Machine | MemoryScope::Global => {
                if self.scope_id.is_some() {
                    bail!("machine and global capsules must not carry a scope identity");
                }
            }
        }
        if self.evidence.is_empty() {
            bail!("a capsule needs at least one evidence entry");
        }
        if self.evidence.len() > MAX_EVIDENCE_ENTRIES {
            bail!("a capsule may retain at most {MAX_EVIDENCE_ENTRIES} evidence entries");
        }
        for entry in &self.evidence {
            if entry.failure_reason.is_some() && entry.outcome != SemanticOutcome::Failure {
                bail!("an evidence failure reason requires a failure outcome");
            }
            let valid_attestation = matches!(
                (entry.source, entry.attestation),
                (
                    EvidenceSource::AgentJudgment,
                    EvidenceAttestation::SelfReported
                ) | (
                    EvidenceSource::HostObservation,
                    EvidenceAttestation::HostAttested
                ) | (
                    EvidenceSource::HumanConfirmation,
                    EvidenceAttestation::HumanAttested
                )
            );
            if !valid_attestation {
                bail!("evidence source and attestation do not share a trust boundary");
            }
        }
        if self.lesson.is_none() && self.situation.is_some() {
            bail!("a situation without a lesson is not a usable capsule");
        }
        if self.last_confirmed_at < self.created_at {
            bail!("capsule confirmation cannot precede its creation");
        }
        if self.lifecycle != MemoryLifecycle::Challenged && self.challenge.is_some() {
            bail!("only a challenged capsule may retain challenge recovery state");
        }
        Ok(())
    }

    pub fn has_text(&self) -> bool {
        self.lesson.is_some()
    }

    pub fn identity(&self) -> ExperienceIdentity {
        ExperienceIdentity {
            task: self.task,
            procedure_summary: self.procedure.summary(),
            applicability: format!(
                "{:?}|{:?}|{:?}|{:?}|{:?}",
                self.applicability.artifact_kind,
                self.applicability.phase,
                self.applicability.ecosystem,
                self.applicability.tool_family,
                self.applicability.risk_shapes
            ),
        }
    }

    pub fn successes(&self) -> usize {
        self.evidence
            .iter()
            .filter(|entry| entry.outcome == SemanticOutcome::Success)
            .count()
    }

    pub fn failures(&self) -> usize {
        self.evidence.len() - self.successes()
    }

    fn dominant_outcome(&self) -> SemanticOutcome {
        if self.successes() >= self.failures() {
            SemanticOutcome::Success
        } else {
            SemanticOutcome::Failure
        }
    }

    pub fn challenge(&mut self, at: DateTime<Utc>) {
        self.lifecycle = MemoryLifecycle::Challenged;
        self.challenge = Some(ChallengeState {
            challenged_at: at,
            supporting_outcome: self.dominant_outcome(),
            recovery_evidence: 0,
        });
    }

    pub fn set_lifecycle(&mut self, lifecycle: MemoryLifecycle, at: DateTime<Utc>) {
        if lifecycle == MemoryLifecycle::Challenged {
            self.challenge(at);
        } else {
            self.lifecycle = lifecycle;
            self.challenge = None;
        }
    }

    /// Adds evidence and advances a challenge only when the new observation
    /// supports the capsule's pre-challenge outcome. Opposing evidence resets
    /// progress instead of accidentally helping reactivation.
    pub fn confirm_with_recovery(&mut self, entry: EvidenceEntry, required: usize) {
        if self.lifecycle != MemoryLifecycle::Challenged {
            self.confirm(entry);
            return;
        }
        if self.challenge.is_none() {
            // Compatibility for early v4 payloads written before explicit
            // challenge bookkeeping existed.
            self.challenge(self.last_confirmed_at);
        }
        let state = self.challenge.expect("challenge state initialized");
        let supports = entry.at > state.challenged_at && entry.outcome == state.supporting_outcome;
        let independent = supports
            && !self.evidence.iter().any(|existing| {
                existing.at > state.challenged_at
                    && existing.outcome == state.supporting_outcome
                    && same_evidence_cohort(existing, &entry)
            });
        self.confirm(entry);
        let state = self
            .challenge
            .as_mut()
            .expect("challenge state initialized");
        state.recovery_evidence = if independent {
            state.recovery_evidence.saturating_add(1)
        } else if !supports {
            state.challenged_at = entry.at;
            0
        } else {
            state.recovery_evidence
        };
        if usize::from(state.recovery_evidence) >= required {
            self.lifecycle = MemoryLifecycle::Active;
            self.challenge = None;
        }
    }

    /// Appends one evidence entry, dropping the oldest when the bound is hit.
    pub fn confirm(&mut self, entry: EvidenceEntry) {
        self.evidence.push(entry);
        if self.evidence.len() > MAX_EVIDENCE_ENTRIES {
            let excess = self.evidence.len() - MAX_EVIDENCE_ENTRIES;
            self.evidence.drain(..excess);
        }
        if entry.at > self.last_confirmed_at {
            self.last_confirmed_at = entry.at;
        }
    }

    /// Lower-case alphanumeric tokens from situation and lesson, for
    /// ephemeral in-memory similarity only. Never persisted.
    pub fn text_tokens(&self) -> BTreeSet<String> {
        let mut tokens = BTreeSet::new();
        for text in [
            self.situation.as_ref().map(|text| text.as_str()),
            self.lesson.as_ref().map(|text| text.as_str()),
        ]
        .into_iter()
        .flatten()
        {
            tokens.extend(tokenize(text));
        }
        tokens
    }
}

/// Explicit cohorts dominate. Unattributed agent observations are grouped by
/// UTC day and controlled provenance/environment, preventing rapid repeated
/// confirmations from masquerading as independent challenge recovery.
fn same_evidence_cohort(left: &EvidenceEntry, right: &EvidenceEntry) -> bool {
    match (left.cohort, right.cohort) {
        (Some(left), Some(right)) => left == right,
        (None, None) => {
            left.at.timestamp().div_euclid(86_400) == right.at.timestamp().div_euclid(86_400)
                && left.source == right.source
                && left.agent == right.agent
                && left.environment == right.environment
        }
        _ => false,
    }
}

pub fn tokenize(text: &str) -> impl Iterator<Item = String> + '_ {
    text.split(|character: char| !character.is_alphanumeric())
        .filter(|token| token.chars().count() >= 3)
        .map(|token| token.to_lowercase())
}

/// Jaccard similarity of two token sets; 0 when either is empty.
pub fn jaccard(left: &BTreeSet<String>, right: &BTreeSet<String>) -> f64 {
    if left.is_empty() || right.is_empty() {
        return 0.0;
    }
    let intersection = left.intersection(right).count();
    let union = left.union(right).count();
    intersection as f64 / union as f64
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    fn capsule() -> ExperienceCapsule {
        let at = Utc.timestamp_millis_opt(1_776_254_400_000).unwrap();
        ExperienceCapsule {
            id: Uuid::from_u128(1),
            project_id: Uuid::from_u128(7),
            scope: MemoryScope::Project,
            scope_id: Some(Uuid::from_u128(7)),
            task: TaskKind::Debugging,
            situation: Some(
                Situation::new(
                    "Generated native artifact; parser acceptance is not authoritative.",
                )
                .unwrap(),
            ),
            lesson: Some(
                Lesson::new("Change one placement class at a time and run native verification.")
                    .unwrap(),
            ),
            caveat: Some(
                Caveat::new("Do not infer correctness from serialization success alone.").unwrap(),
            ),
            procedure: Procedure {
                mutation: Some(MutationMode::IncrementalNativeRegeneration),
                verification: Some(VerificationMode::Native),
                steps: vec![ProcedureStep::Regenerate, ProcedureStep::VerifyNative],
                ..Procedure::default()
            },
            applicability: ApplicabilityTags {
                artifact_kind: Some(ArtifactKind::NativeCad),
                phase: Some(Phase::Verify),
                ecosystem: Some(Ecosystem::Kicad),
                ..ApplicabilityTags::default()
            },
            lifecycle: MemoryLifecycle::Active,
            challenge: None,
            evidence: vec![EvidenceEntry::agent_reported(
                at,
                SemanticOutcome::Success,
                None,
                EnvironmentFingerprint {
                    tool_family: Some(ToolFamily::Kicad),
                    major_version: Some(10),
                    host_class: Some(HostClass::Linux),
                },
            )],
            created_at: at,
            last_confirmed_at: at,
            schema_version: EXPERIENCE_SCHEMA_VERSION,
        }
    }

    #[test]
    fn capsule_round_trips_and_validates() {
        let capsule = capsule();
        capsule.validate().unwrap();
        let encoded = serde_json::to_vec(&capsule).unwrap();
        let decoded: ExperienceCapsule = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(decoded, capsule);
    }

    #[test]
    fn deserialization_re_runs_admission() {
        let mut value = serde_json::to_value(capsule()).unwrap();
        value["lesson"] = serde_json::Value::String("see https://example.test/x".into());
        assert!(serde_json::from_value::<ExperienceCapsule>(value).is_err());

        let mut value = serde_json::to_value(capsule()).unwrap();
        value["situation"] = serde_json::Value::String("x ".repeat(200));
        assert!(serde_json::from_value::<ExperienceCapsule>(value).is_err());
    }

    #[test]
    fn every_strategy_maps_to_a_non_empty_procedure_and_back() {
        for strategy in [
            Strategy::StructuredPatch,
            Strategy::DirectTextMutation,
            Strategy::IncrementalNativeRegeneration,
            Strategy::BulkChange,
            Strategy::NativeTool,
            Strategy::AtomicWrite,
            Strategy::PreviewThenApply,
            Strategy::TargetedVerification,
            Strategy::FullVerification,
            Strategy::NativeHook,
            Strategy::TranscriptFallback,
            Strategy::ReproduceThenCompare,
            Strategy::PerSubjectStreaming,
            Strategy::ResourceCapFirst,
        ] {
            let procedure = Procedure::from_strategy(strategy);
            procedure.validate().unwrap();
            assert_eq!(procedure.legacy_strategy(), strategy, "{strategy:?}");
            assert_eq!(
                procedure.classification(),
                strategy.practice_classification()
            );
        }
        assert!(!Procedure::from_strategy(Strategy::Other).is_empty());
    }

    #[test]
    fn tool_family_forgives_the_ecosystem_catch_all_spelling() {
        assert_eq!("generic".parse::<ToolFamily>().unwrap(), ToolFamily::Other);
        assert_eq!("Generic".parse::<ToolFamily>().unwrap(), ToolFamily::Other);
        assert!("generic".parse::<Phase>().is_err());
        assert_eq!(ToolFamily::Other.label(), "other");
    }

    #[test]
    fn procedures_parse_from_controlled_labels_only() {
        let procedure =
            Procedure::parse_dimensions("incremental-native-regeneration, native_verification")
                .unwrap();
        assert_eq!(
            procedure.mutation,
            Some(MutationMode::IncrementalNativeRegeneration)
        );
        assert_eq!(procedure.verification, Some(VerificationMode::Native));
        let error = Procedure::parse_dimensions("hypothesis-revision")
            .unwrap_err()
            .to_string();
        assert!(error.contains("structured-patch"));
        assert!(error.contains("reproduce-then-compare"));
        assert!(error.contains("lesson text"));
        assert!(Procedure::parse_dimensions("rm -rf everything").is_err());
        assert!(Procedure::parse_dimensions("structured-patch,bulk-change").is_err());
        assert_eq!(
            Procedure::parse_steps("inspect,patch,verify-targeted").unwrap(),
            vec![
                ProcedureStep::Inspect,
                ProcedureStep::Patch,
                ProcedureStep::VerifyTargeted
            ]
        );
        assert!(Procedure::parse_steps("inspect,exfiltrate").is_err());
    }

    #[test]
    fn ordered_steps_distinguish_procedures() {
        let patch_then_verify = Procedure {
            steps: vec![ProcedureStep::Patch, ProcedureStep::VerifyTargeted],
            ..Procedure::default()
        };
        let verify_then_patch = Procedure {
            steps: vec![ProcedureStep::VerifyTargeted, ProcedureStep::Patch],
            ..Procedure::default()
        };
        assert_ne!(patch_then_verify.summary(), verify_then_patch.summary());
        assert_eq!(patch_then_verify.summary(), "patch -> verify-targeted");
    }

    #[test]
    fn scope_anchoring_is_enforced() {
        let mut global = capsule();
        global.scope = MemoryScope::Global;
        assert!(global.validate().is_err());
        global.scope_id = None;
        global.validate().unwrap();

        let mut ecosystem = capsule();
        ecosystem.scope = MemoryScope::Ecosystem;
        ecosystem.scope_id = None;
        ecosystem.applicability.ecosystem = None;
        assert!(ecosystem.validate().is_err());
    }

    #[test]
    fn evidence_is_bounded() {
        let mut capsule = capsule();
        let base = capsule.created_at;
        for offset in 1..=(MAX_EVIDENCE_ENTRIES as i64 + 10) {
            capsule.confirm(EvidenceEntry::agent_reported(
                base + chrono::Duration::minutes(offset),
                SemanticOutcome::Failure,
                Some(FailureReason::VerificationFailed),
                Default::default(),
            ));
        }
        assert_eq!(capsule.evidence.len(), MAX_EVIDENCE_ENTRIES);
        capsule.validate().unwrap();
        assert_eq!(capsule.successes(), 0);
    }

    #[test]
    fn debug_output_hides_text() {
        let debug = format!("{:?}", capsule().lesson.unwrap());
        assert!(!debug.contains("placement"));
    }
}
