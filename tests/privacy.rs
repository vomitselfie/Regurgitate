use std::fs::File;

use regurgitate::{adapters::codex::normalize_post_tool_hook, core::DebugEvent};

#[test]
fn adversarial_hook_fixture_leaves_no_private_content() {
    let fixture = File::open("tests/fixtures/codex/post-tool-use-success.json").unwrap();
    let event = normalize_post_tool_hook(fixture).unwrap();
    let stored = serde_json::to_string(&event).unwrap();
    let projected = serde_json::to_string(&DebugEvent::from(&event)).unwrap();

    for forbidden in [
        "sk-fixture-secret",
        "hunter2",
        "private.example.test",
        "/home/alice",
        "secret-project",
        "source code",
        "curl",
    ] {
        assert!(
            !stored.contains(forbidden),
            "stored event leaked {forbidden:?}"
        );
        assert!(
            !projected.contains(forbidden),
            "debug projection leaked {forbidden:?}"
        );
    }
}
