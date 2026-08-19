use std::fs::{self, File};

use praxis::{
    adapters::codex::normalize_post_tool_hook,
    storage::{EncryptedStore, MasterKey},
};
use tempfile::tempdir;

#[test]
fn adversarial_hook_content_never_reaches_the_database() {
    let fixture = File::open("tests/fixtures/codex/post-tool-use-success.json").unwrap();
    let event = normalize_post_tool_hook(fixture).unwrap();
    let event_id = event.id;

    let temp = tempdir().unwrap();
    let database = temp.path().join("history.db");
    {
        let key = MasterKey::from_bytes([0x42; 32]);
        let store = EncryptedStore::open(&database, &key).unwrap();
        assert!(store.append(&event).unwrap());
        assert_eq!(store.get(event_id).unwrap(), Some(event));
    }

    let persisted = fs::read(database).unwrap();
    for forbidden in [
        "sk-fixture-secret",
        "hunter2",
        "private.example.test",
        "/home/alice",
        "secret-project",
        "source code",
        "curl",
        "session-fixture",
    ] {
        assert!(
            !persisted
                .windows(forbidden.len())
                .any(|window| window == forbidden.as_bytes()),
            "database leaked {forbidden:?}"
        );
    }
}
