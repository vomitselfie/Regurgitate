use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const CURRENT_SCHEMA_VERSION: u32 = 1;

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
    pub capability: Capability,
    pub operation: Operation,
    pub strategy: Option<Strategy>,
    pub outcome: Outcome,
    pub duration_ms: Option<u64>,
    pub error_class: Option<ErrorClass>,
    pub schema_version: u32,
}

/// Deliberately omits identifiers and timestamps from debug/agent output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct DebugEvent {
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
}
