use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const CURRENT_SCHEMA_VERSION: u32 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentKind {
    Codex,
    Other,
    Claude,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    Application,
    Window,
    Pointer,
    Keyboard,
    Scroll,
    Accessibility,
    Vision,
    Filesystem,
    Shell,
    Browser,
    Network,
    Search,
    Edit,
    Patch,
    Git,
    Build,
    Test,
    Lint,
    Format,
    PackageManager,
    Python,
    Container,
    Wait,
    Verify,
    Research,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Operation {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Success,
    Failure,
    Unknown,
}

/// Distinguishes operational hook telemetry from a deliberately evaluated
/// procedural outcome. The discriminant is authenticated structural metadata
/// so storage can retrieve bounded samples without decrypting unrelated rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum EvidenceKind {
    HookExecution = 1,
    LearnedPractice = 2,
}

impl EvidenceKind {
    pub const fn storage_code(self) -> u8 {
        self as u8
    }

    pub const fn from_storage_code(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::HookExecution),
            2 => Some(Self::LearnedPractice),
            _ => None,
        }
    }
}

/// A deliberately broad, controlled task vocabulary. It supplies enough
/// context to discriminate procedural evidence without retaining arbitrary
/// task descriptions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskKind {
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

/// A deliberately small initial strategy vocabulary. Adapters should leave
/// this unset unless the strategy can be derived without retaining arguments
/// or content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Strategy {
    StructuredPatch,
    DirectTextMutation,
    IncrementalNativeRegeneration,
    BulkChange,
    NativeTool,
    AtomicWrite,
    PreviewThenApply,
    TargetedVerification,
    FullVerification,
    NativeHook,
    TranscriptFallback,
    ReproduceThenCompare,
    PerSubjectStreaming,
    ResourceCapFirst,
    Other,
}

impl Strategy {
    /// Canonical controlled classification for an explicitly learned practice.
    /// This keeps the CLI from accepting arbitrary or semantically unrelated
    /// capability/operation combinations.
    pub fn practice_classification(self) -> (Capability, Operation) {
        match self {
            Self::StructuredPatch
            | Self::DirectTextMutation
            | Self::IncrementalNativeRegeneration
            | Self::BulkChange => (Capability::Patch, Operation::ApplyPatch),
            Self::AtomicWrite => (Capability::Filesystem, Operation::WriteFile),
            Self::PreviewThenApply => (Capability::Verify, Operation::ToolCall),
            Self::TargetedVerification | Self::FullVerification => {
                (Capability::Test, Operation::Command)
            }
            Self::ReproduceThenCompare | Self::PerSubjectStreaming | Self::ResourceCapFirst => {
                (Capability::Research, Operation::Analyze)
            }
            Self::NativeHook | Self::TranscriptFallback | Self::NativeTool | Self::Other => {
                (Capability::Other, Operation::ToolCall)
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorClass {
    PermissionDenied,
    CommandNotFound,
    NonzeroExit,
    ParseError,
    TestFailure,
    BuildFailure,
    NetworkFailure,
    Timeout,
    SandboxDenied,
    MissingDependency,
    InvalidPatch,
    GitConflict,
    Unknown,
}

/// The only value that may cross from an adapter into persistence/query code.
///
/// Every string here is either an opaque local identifier or drawn from a
/// controlled enum. Adapters must never put prompts, commands, paths, tool
/// arguments, or tool results into this type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoryEvent {
    pub id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub session_id: Option<String>,
    pub project_id: Option<Uuid>,
    pub agent: Option<AgentKind>,
    pub evidence_kind: EvidenceKind,
    pub task: Option<TaskKind>,
    pub capability: Capability,
    pub operation: Operation,
    pub strategy: Option<Strategy>,
    pub outcome: Outcome,
    pub duration_ms: Option<u64>,
    pub error_class: Option<ErrorClass>,
    pub schema_version: u32,
}

impl HistoryEvent {
    pub fn has_valid_evidence_shape(&self) -> bool {
        match self.evidence_kind {
            EvidenceKind::HookExecution => self.task.is_none(),
            EvidenceKind::LearnedPractice => {
                self.task.is_some()
                    && self.strategy.is_some()
                    && self.outcome != Outcome::Unknown
                    && self.session_id.is_none()
                    && self.agent.is_none()
            }
        }
    }
}

/// Deliberately omits identifiers and timestamps from debug/agent output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct DebugEvent {
    pub evidence_kind: EvidenceKind,
    pub capability: Capability,
    pub operation: Operation,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strategy: Option<Strategy>,
    pub outcome: Outcome,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_class: Option<ErrorClass>,
    pub schema_version: u32,
}

impl From<&HistoryEvent> for DebugEvent {
    fn from(event: &HistoryEvent) -> Self {
        Self {
            evidence_kind: event.evidence_kind,
            capability: event.capability,
            operation: event.operation,
            strategy: event.strategy,
            outcome: event.outcome,
            error_class: event.error_class,
            schema_version: event.schema_version,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn research_strategies_use_one_analysis_classification() {
        for strategy in [
            Strategy::ReproduceThenCompare,
            Strategy::PerSubjectStreaming,
            Strategy::ResourceCapFirst,
        ] {
            assert_eq!(
                strategy.practice_classification(),
                (Capability::Research, Operation::Analyze)
            );
        }
    }

    #[test]
    fn history_event_round_trips() {
        let event = HistoryEvent {
            id: Uuid::nil(),
            timestamp: DateTime::parse_from_rfc3339("2026-08-19T12:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
            session_id: Some("session-local-id".into()),
            project_id: Some(Uuid::nil()),
            agent: Some(AgentKind::Codex),
            evidence_kind: EvidenceKind::HookExecution,
            task: None,
            capability: Capability::Test,
            operation: Operation::Command,
            strategy: Some(Strategy::NativeTool),
            outcome: Outcome::Failure,
            duration_ms: Some(42),
            error_class: Some(ErrorClass::NonzeroExit),
            schema_version: CURRENT_SCHEMA_VERSION,
        };

        let encoded = serde_json::to_vec(&event).unwrap();
        let decoded: HistoryEvent = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(decoded, event);
    }

    #[test]
    fn learned_practice_requires_semantic_fields_and_no_provider_identity() {
        let mut event = HistoryEvent {
            id: Uuid::nil(),
            timestamp: Utc::now(),
            session_id: None,
            project_id: Some(Uuid::nil()),
            agent: None,
            evidence_kind: EvidenceKind::LearnedPractice,
            task: Some(TaskKind::DataImport),
            capability: Capability::Research,
            operation: Operation::Analyze,
            strategy: Some(Strategy::PerSubjectStreaming),
            outcome: Outcome::Success,
            duration_ms: None,
            error_class: None,
            schema_version: CURRENT_SCHEMA_VERSION,
        };
        assert!(event.has_valid_evidence_shape());

        event.outcome = Outcome::Unknown;
        assert!(!event.has_valid_evidence_shape());
    }
}
