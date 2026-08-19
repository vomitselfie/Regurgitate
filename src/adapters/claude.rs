use std::{io::Read, path::PathBuf};

use anyhow::{Context, Result, bail};
use chrono::Utc;
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    application::{HookObservation, ProjectLocator},
    core::{AgentKind, CURRENT_SCHEMA_VERSION, ErrorClass, HistoryEvent, Outcome, classify_tool},
};

/// The complete allowlist read from Claude Code tool hooks. In particular,
/// this type has no fields for tool input, tool response, failure text,
/// transcript paths, prompts, or model output.
#[derive(Debug, Deserialize)]
struct ClaudeHookInput {
    session_id: String,
    cwd: PathBuf,
    hook_event_name: String,
    tool_name: String,
    tool_use_id: String,
    duration_ms: Option<u64>,
}

/// Normalize a successful or failed Claude Code tool hook without inspecting
/// its raw request, response, or error. The hook event itself is the outcome
/// signal, so no provider content needs to cross the adapter boundary.
pub fn normalize_tool_hook<R: Read>(reader: R) -> Result<HookObservation> {
    let input: ClaudeHookInput =
        serde_json::from_reader(reader).context("invalid Claude hook JSON")?;
    let (outcome, error_class) = match input.hook_event_name.as_str() {
        "PostToolUse" => (Outcome::Success, None),
        "PostToolUseFailure" => (Outcome::Failure, Some(ErrorClass::Unknown)),
        _ => bail!("expected a Claude PostToolUse or PostToolUseFailure event"),
    };
    let (capability, operation) = classify_tool(&input.tool_name);
    let event = HistoryEvent {
        id: stable_event_id(&input.session_id, &input.tool_use_id),
        timestamp: Utc::now(),
        session_id: Some(input.session_id),
        project_id: None,
        agent: Some(AgentKind::Claude),
        capability,
        operation,
        strategy: None,
        outcome,
        duration_ms: input.duration_ms,
        error_class,
        schema_version: CURRENT_SCHEMA_VERSION,
    };
    Ok(HookObservation::new(event, ProjectLocator::new(input.cwd)))
}

fn stable_event_id(session_id: &str, tool_use_id: &str) -> Uuid {
    let source = format!("claude:{session_id}:{tool_use_id}");
    Uuid::new_v5(&Uuid::NAMESPACE_OID, source.as_bytes())
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::core::{Capability, Operation};

    use super::*;

    #[test]
    fn normalizes_success_without_inspecting_raw_values() {
        let fixture = br#"{
            "session_id": "claude-session-1",
            "transcript_path": "/home/alice/.claude/private.jsonl",
            "cwd": "/home/alice/secret-project",
            "permission_mode": "default",
            "hook_event_name": "PostToolUse",
            "tool_name": "Write",
            "tool_use_id": "tool-1",
            "duration_ms": 17,
            "tool_input": {"file_path": "/home/alice/secret.rs", "content": "API_KEY=SECRET"},
            "tool_response": {"content": "PASSWORD=hunter2"}
        }"#;

        let first = normalize_tool_hook(&fixture[..]).unwrap();
        let second = normalize_tool_hook(&fixture[..]).unwrap();
        assert_eq!(first.event().id, second.event().id);
        assert_eq!(first.event().agent, Some(AgentKind::Claude));
        assert_eq!(first.event().capability, Capability::Patch);
        assert_eq!(first.event().operation, Operation::ApplyPatch);
        assert_eq!(first.event().outcome, Outcome::Success);
        assert_eq!(first.event().duration_ms, Some(17));
        assert_eq!(
            first.project().as_path(),
            Path::new("/home/alice/secret-project")
        );

        let encoded = serde_json::to_string(first.event()).unwrap();
        for forbidden in ["API_KEY", "SECRET", "hunter2", "/home/alice"] {
            assert!(!encoded.contains(forbidden), "leaked {forbidden:?}");
        }
    }

    #[test]
    fn failure_event_determines_outcome_without_reading_error_text() {
        let fixture = br#"{
            "session_id": "claude-session-1",
            "cwd": "/private/project",
            "hook_event_name": "PostToolUseFailure",
            "tool_name": "Bash",
            "tool_use_id": "tool-2",
            "error": "SECRET_FAILURE_TEXT"
        }"#;

        let observation = normalize_tool_hook(&fixture[..]).unwrap();
        assert_eq!(observation.event().outcome, Outcome::Failure);
        assert_eq!(observation.event().error_class, Some(ErrorClass::Unknown));
        assert!(
            !serde_json::to_string(observation.event())
                .unwrap()
                .contains("SECRET_FAILURE_TEXT")
        );
    }
}
