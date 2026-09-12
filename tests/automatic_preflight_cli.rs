use std::{
    io::Write,
    process::{Command, Stdio},
};

#[test]
fn automatic_hooks_are_silent_for_empty_invalid_and_oversized_input() {
    for agent in ["codex", "claude"] {
        for input in [
            "not JSON".to_owned(),
            r#"{"cwd":"/absent","hook_event_name":"PostToolUse","prompt":"test rust"}"#.to_owned(),
            r#"{"cwd":"/absent","hook_event_name":"UserPromptSubmit","prompt":"thanks!"}"#
                .to_owned(),
            r#"{"cwd":"/absent","hook_event_name":"UserPromptSubmit","prompt":"debug rust tests"}"#
                .to_owned(),
            "x".repeat(70_000),
        ] {
            let temp = tempfile::tempdir().unwrap();
            let data = temp.path().join("absent");
            let mut child = Command::new(env!("CARGO_BIN_EXE_regurgitate"))
                .args(["preflight", "--agent", agent, "--data-home"])
                .arg(&data)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
            // Oversized inputs may cause the reader to close before the writer.
            let _ = child.stdin.take().unwrap().write_all(input.as_bytes());
            let output = child.wait_with_output().unwrap();
            assert!(output.status.success());
            assert!(output.stdout.is_empty());
            assert!(output.stderr.is_empty());
            assert!(!data.exists());
        }
    }
}
