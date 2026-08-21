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

#[test]
fn adversarial_text_never_becomes_a_capsule_sentence() {
    use regurgitate::core::{Caveat, Lesson, Situation};

    let fixture =
        std::fs::read_to_string("tests/fixtures/experience/adversarial-text.txt").unwrap();
    let mut checked = 0;
    for line in fixture.lines().filter(|line| !line.trim().is_empty()) {
        checked += 1;
        assert!(
            Situation::new(line).is_err(),
            "situation admitted adversarial text: {line:?}"
        );
        assert!(
            Lesson::new(line).is_err(),
            "lesson admitted adversarial text: {line:?}"
        );
        assert!(
            Caveat::new(line).is_err(),
            "caveat admitted adversarial text: {line:?}"
        );
    }
    assert!(checked >= 20);

    // Length caps hold even for otherwise innocent prose.
    let long = "verify the placement class ".repeat(20);
    assert!(Situation::new(&long).is_err());
    assert!(Lesson::new(&long).is_err());
    assert!(Caveat::new(&long).is_err());
}

#[test]
fn capsule_payloads_re_validate_on_decode() {
    use regurgitate::core::ExperienceCapsule;

    // A payload that smuggles a URL past the constructor by being written
    // directly still fails when decoded.
    let payload = serde_json::json!({
        "id": "00000000-0000-0000-0000-000000000001",
        "project_id": "00000000-0000-0000-0000-000000000007",
        "scope": "project",
        "scope_id": "00000000-0000-0000-0000-000000000007",
        "task": "debugging",
        "lesson": "see https://private.example.test/notes for the fix",
        "procedure": {"mutation": "structured_patch"},
        "lifecycle": "active",
        "evidence": [{"at": "2026-08-19T12:00:00Z", "outcome": "success"}],
        "created_at": "2026-08-19T12:00:00Z",
        "last_confirmed_at": "2026-08-19T12:00:00Z",
        "schema_version": 3
    });
    assert!(serde_json::from_value::<ExperienceCapsule>(payload).is_err());
}
