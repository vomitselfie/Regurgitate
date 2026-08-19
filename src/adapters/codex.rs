use std::{
    collections::HashMap,
    io::{BufRead, BufReader, Read},
    path::PathBuf,
};

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::Value;
use uuid::Uuid;

use crate::{
    application::{HookObservation, ProjectLocator},
    core::{
        AgentKind, CURRENT_SCHEMA_VERSION, HistoryEvent, Outcome, classify_strategy, classify_tool,
        classify_tool_response,
    },
};

#[derive(Debug, Deserialize)]
struct CodexHookInput {
    session_id: String,
    cwd: Option<PathBuf>,
    hook_event_name: String,
    tool_name: Option<String>,
    tool_use_id: Option<String>,
    tool_response: Option<Value>,
    duration_ms: Option<u64>,
}

/// Normalize a current Codex hook payload. Unknown fields—including `cwd`,
/// `transcript_path`, `tool_input`, prompts, and future additions—are discarded
/// by Serde and never become members of this adapter type.
pub fn normalize_post_tool_hook<R: Read>(reader: R) -> Result<HistoryEvent> {
    let input: CodexHookInput =
        serde_json::from_reader(reader).context("invalid Codex hook JSON")?;
    Ok(normalize_hook_input(input)?.0)
}

/// Normalize a current Codex hook into the complete, still-sanitized value
/// needed for direct recording. The project path remains a non-serializable
/// locator and never becomes part of the event.
pub fn normalize_post_tool_observation<R: Read>(reader: R) -> Result<HookObservation> {
    let input: CodexHookInput =
        serde_json::from_reader(reader).context("invalid Codex hook JSON")?;
    let (event, cwd) = normalize_hook_input(input)?;
    let cwd = cwd.context("Codex hook event has no working directory")?;
    Ok(HookObservation::new(event, ProjectLocator::new(cwd)))
}

fn normalize_hook_input(input: CodexHookInput) -> Result<(HistoryEvent, Option<PathBuf>)> {
    if input.hook_event_name != "PostToolUse" {
        bail!("expected a Codex PostToolUse event");
    }

    let tool_name = input.tool_name.context("hook event has no tool name")?;
    let source_event_id = input.tool_use_id.context("hook event has no tool use id")?;
    let (capability, operation) = classify_tool(&tool_name);
    let (outcome, error_class) = input
        .tool_response
        .as_ref()
        .map(classify_tool_response)
        .unwrap_or((Outcome::Unknown, None));

    let event = HistoryEvent {
        id: stable_event_id(&input.session_id, &source_event_id),
        timestamp: Utc::now(),
        session_id: Some(input.session_id),
        project_id: None,
        agent: Some(AgentKind::Codex),
        capability,
        operation,
        strategy: classify_strategy(&tool_name),
        outcome,
        duration_ms: input.duration_ms,
        error_class,
        schema_version: CURRENT_SCHEMA_VERSION,
    };
    Ok((event, input.cwd))
}

#[derive(Debug, Deserialize)]
pub(super) struct TranscriptRecord {
    pub(super) timestamp: Option<DateTime<Utc>>,
    #[serde(rename = "type")]
    pub(super) record_type: String,
    pub(super) payload: TranscriptPayload,
}

#[derive(Debug, Deserialize)]
pub(super) struct TranscriptPayload {
    #[serde(rename = "type")]
    pub(super) payload_type: Option<String>,
    pub(super) call_id: Option<String>,
    pub(super) name: Option<String>,
    pub(super) output: Option<Value>,
}

#[derive(Debug)]
struct PendingCall {
    timestamp: DateTime<Utc>,
    tool_name: String,
}

/// Parse the existing Codex JSONL representation conservatively. The typed
/// transcript structs intentionally have no fields for arguments, input,
/// messages, reasoning, model output, paths, or git metadata.
pub fn normalize_transcript<R: Read>(reader: R, session_id: &str) -> Result<Vec<HistoryEvent>> {
    let mut calls = HashMap::<String, PendingCall>::new();
    let mut events = Vec::new();

    for (index, line) in BufReader::new(reader).lines().enumerate() {
        let line = line.with_context(|| format!("could not read transcript line {}", index + 1))?;
        if line.trim().is_empty() {
            continue;
        }
        let record: TranscriptRecord = serde_json::from_str(&line)
            .with_context(|| format!("invalid transcript record at line {}", index + 1))?;
        if record.record_type != "response_item" {
            continue;
        }

        match record.payload.payload_type.as_deref() {
            Some("function_call" | "custom_tool_call") => {
                let Some(call_id) = record.payload.call_id else {
                    continue;
                };
                let Some(tool_name) = record.payload.name else {
                    continue;
                };
                calls.insert(
                    call_id,
                    PendingCall {
                        timestamp: record.timestamp.unwrap_or_else(Utc::now),
                        tool_name,
                    },
                );
            }
            Some("function_call_output" | "custom_tool_call_output") => {
                let Some(call_id) = record.payload.call_id else {
                    continue;
                };
                let Some(call) = calls.remove(&call_id) else {
                    continue;
                };
                let (outcome, error_class) = record
                    .payload
                    .output
                    .as_ref()
                    .map(classify_tool_response)
                    .unwrap_or((Outcome::Unknown, None));
                events.push(history_event(
                    session_id,
                    &call_id,
                    call.timestamp,
                    &call.tool_name,
                    outcome,
                    error_class,
                ));
            }
            _ => {}
        }
    }

    for (call_id, call) in calls {
        events.push(history_event(
            session_id,
            &call_id,
            call.timestamp,
            &call.tool_name,
            Outcome::Unknown,
            None,
        ));
    }
    events.sort_by_key(|event| event.timestamp);
    Ok(events)
}

fn history_event(
    session_id: &str,
    source_event_id: &str,
    timestamp: DateTime<Utc>,
    tool_name: &str,
    outcome: Outcome,
    error_class: Option<crate::core::ErrorClass>,
) -> HistoryEvent {
    let (capability, operation) = classify_tool(tool_name);
    HistoryEvent {
        id: stable_event_id(session_id, source_event_id),
        timestamp,
        session_id: Some(session_id.to_owned()),
        project_id: None,
        agent: Some(AgentKind::Codex),
        capability,
        operation,
        strategy: classify_strategy(tool_name),
        outcome,
        duration_ms: None,
        error_class,
        schema_version: CURRENT_SCHEMA_VERSION,
    }
}

pub(super) fn stable_event_id(session_id: &str, source_event_id: &str) -> Uuid {
    let mut source = String::with_capacity(session_id.len() + source_event_id.len() + 1);
    source.push_str(session_id);
    source.push(':');
    source.push_str(source_event_id);
    Uuid::new_v5(&Uuid::NAMESPACE_OID, source.as_bytes())
}

#[cfg(test)]
mod tests {
    use crate::core::{Capability, ErrorClass, Operation, Strategy};

    use super::*;

    #[test]
    fn normalizes_hook_without_retaining_raw_fields() {
        let fixture = br#"{
            "session_id": "session-1",
            "transcript_path": "/home/alice/private/session.jsonl",
            "cwd": "/home/alice/secret-project",
            "hook_event_name": "PostToolUse",
            "tool_name": "Bash",
            "tool_use_id": "call-1",
            "tool_input": {"command": "curl https://example.test/?token=SECRET_TOKEN"},
            "tool_response": {"exit_code": 1, "output": "PASSWORD=hunter2"}
        }"#;

        let event = normalize_post_tool_hook(&fixture[..]).unwrap();
        assert_eq!(event.capability, Capability::Shell);
        assert_eq!(event.operation, Operation::Command);
        assert_eq!(event.strategy, None);
        assert_eq!(event.outcome, Outcome::Failure);
        assert_eq!(event.error_class, Some(ErrorClass::NonzeroExit));

        let encoded = serde_json::to_string(&event).unwrap();
        for forbidden in [
            "SECRET_TOKEN",
            "hunter2",
            "example.test",
            "/home/alice",
            "curl",
        ] {
            assert!(!encoded.contains(forbidden), "leaked {forbidden:?}");
        }
    }

    #[test]
    fn derives_structured_patch_strategy_without_reading_patch_content() {
        let fixture = br#"{
            "session_id": "session-1",
            "cwd": "/private/project",
            "hook_event_name": "PostToolUse",
            "tool_name": "apply_patch",
            "tool_use_id": "call-1",
            "tool_input": {"command": "SECRET_PATCH_CONTENT"},
            "tool_response": {"success": true}
        }"#;

        let event = normalize_post_tool_observation(&fixture[..]).unwrap();
        assert_eq!(event.event().strategy, Some(Strategy::StructuredPatch));
        assert!(
            !serde_json::to_string(event.event())
                .unwrap()
                .contains("SECRET_PATCH_CONTENT")
        );
    }

    #[test]
    fn preserves_project_only_as_a_non_serializable_locator() {
        let fixture = br#"{
            "session_id": "session-1",
            "cwd": "/home/alice/secret-project",
            "hook_event_name": "PostToolUse",
            "tool_name": "Bash",
            "tool_use_id": "call-1",
            "duration_ms": 42,
            "tool_input": {"command": "SECRET_COMMAND"},
            "tool_response": {"exit_code": 0, "output": "SECRET_OUTPUT"}
        }"#;

        let observation = normalize_post_tool_observation(&fixture[..]).unwrap();
        assert_eq!(
            observation.project().as_path(),
            std::path::Path::new("/home/alice/secret-project")
        );
        assert_eq!(observation.event().duration_ms, Some(42));
        assert!(observation.event().project_id.is_none());

        let encoded = serde_json::to_string(observation.event()).unwrap();
        for forbidden in ["SECRET_COMMAND", "SECRET_OUTPUT", "/home/alice"] {
            assert!(!encoded.contains(forbidden), "leaked {forbidden:?}");
        }
    }

    #[test]
    fn transcript_parsing_is_deterministic_and_sanitized() {
        let fixture = concat!(
            "{\"timestamp\":\"2026-08-19T12:00:00Z\",\"type\":\"response_item\",\"payload\":{\"type\":\"function_call\",\"name\":\"exec_command\",\"call_id\":\"call-1\",\"arguments\":\"{SECRET_COMMAND}\"}}\n",
            "{\"timestamp\":\"2026-08-19T12:00:01Z\",\"type\":\"response_item\",\"payload\":{\"type\":\"function_call_output\",\"call_id\":\"call-1\",\"output\":\"Chunk ID: private\\nProcess exited with code 0\\nFinal output:\\nPASSWORD=hunter2\"}}\n"
        );

        let first = normalize_transcript(fixture.as_bytes(), "session-1").unwrap();
        let second = normalize_transcript(fixture.as_bytes(), "session-1").unwrap();
        assert_eq!(first, second);
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].outcome, Outcome::Success);

        let encoded = serde_json::to_string(&first).unwrap();
        assert!(!encoded.contains("SECRET_COMMAND"));
        assert!(!encoded.contains("hunter2"));
        assert!(!encoded.contains("private"));
    }
}
