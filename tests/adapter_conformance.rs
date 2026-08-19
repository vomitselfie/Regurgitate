use std::path::Path;

use regurgitate::{
    adapters::{claude, codex},
    application::HookObservation,
    core::{AgentKind, CURRENT_SCHEMA_VERSION, Outcome},
};

const CODEX_SUCCESS: &[u8] = include_bytes!("fixtures/codex/post-tool-use-success.json");
const CLAUDE_SUCCESS: &[u8] = include_bytes!("fixtures/claude/post-tool-use-success.json");
const CLAUDE_FAILURE: &[u8] = include_bytes!("fixtures/claude/post-tool-use-failure.json");

fn assert_conforms(
    observation: HookObservation,
    expected_agent: AgentKind,
    expected_project: &Path,
    expected_outcome: Outcome,
) {
    let event = observation.event();
    assert_eq!(event.schema_version, CURRENT_SCHEMA_VERSION);
    assert_eq!(event.agent, Some(expected_agent));
    assert!(event.session_id.as_deref().is_some_and(|id| !id.is_empty()));
    assert!(event.project_id.is_none());
    assert_eq!(observation.project().as_path(), expected_project);

    let encoded = serde_json::to_string(event).unwrap();
    for forbidden in [
        "FIXTURE_SECRET",
        "hunter2",
        "private.example",
        "SECRET_PROJECT",
        "/home/alice",
        "curl",
    ] {
        assert!(!encoded.contains(forbidden), "leaked {forbidden:?}");
    }
    assert_eq!(event.outcome, expected_outcome);
}

#[test]
fn codex_native_hook_conforms_to_the_sanitized_boundary() {
    assert_conforms(
        codex::normalize_post_tool_observation(CODEX_SUCCESS).unwrap(),
        AgentKind::Codex,
        Path::new("/home/alice/secret-project"),
        Outcome::Success,
    );
}

#[test]
fn claude_success_hook_conforms_to_the_sanitized_boundary() {
    assert_conforms(
        claude::normalize_tool_hook(CLAUDE_SUCCESS).unwrap(),
        AgentKind::Claude,
        Path::new("/home/alice/SECRET_PROJECT"),
        Outcome::Success,
    );
}

#[test]
fn claude_failure_hook_conforms_to_the_sanitized_boundary() {
    assert_conforms(
        claude::normalize_tool_hook(CLAUDE_FAILURE).unwrap(),
        AgentKind::Claude,
        Path::new("/home/alice/SECRET_PROJECT"),
        Outcome::Failure,
    );
}
