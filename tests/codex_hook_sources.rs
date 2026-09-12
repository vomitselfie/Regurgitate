use regurgitate::{
    application::HookReadiness,
    packaging::{InstallStatus, inspect_codex_hook, install_codex_hook_command},
};
use std::fs;
use tempfile::tempdir;

const OLD: &str = "'/old/regurgitate' record-hook --agent codex";
const NEW: &str = "'/new/regurgitate' record-hook --agent codex";

#[test]
fn json_is_detected_and_updated_without_writing_toml_or_trust() {
    let temp = tempdir().unwrap();
    let config = temp.path().join("config.toml");
    let json = temp.path().join("hooks.json");
    let toml = "# personal config\n[hooks.state.example]\ntrusted_hash = 'unchanged'\n";
    fs::write(&config, toml).unwrap();
    fs::write(&json, r#"{"description":"personal","hooks":{"Stop":[{"hooks":[{"type":"command","command":"personal"}]}]}}"#).unwrap();
    let result = install_codex_hook_command(&config, OLD, true).unwrap();
    assert_eq!(result.config, json);
    assert_eq!(
        inspect_codex_hook(&config).unwrap(),
        HookReadiness::Installed
    );
    let original = fs::read_to_string(&json).unwrap();
    assert_eq!(
        install_codex_hook_command(&config, NEW, false)
            .unwrap()
            .status,
        InstallStatus::Planned
    );
    assert_eq!(fs::read_to_string(&json).unwrap(), original);
    install_codex_hook_command(&config, NEW, true).unwrap();
    assert_eq!(
        install_codex_hook_command(&config, NEW, true)
            .unwrap()
            .status,
        InstallStatus::AlreadyCurrent
    );
    let result = fs::read_to_string(&json).unwrap();
    assert!(result.contains("personal"));
    assert!(!result.contains(OLD));
    assert_eq!(result.matches("record-hook --agent codex").count(), 1);
    assert_eq!(fs::read_to_string(&config).unwrap(), toml);
}

#[test]
fn malformed_mixed_disabled_and_restricted_sources_are_preserved() {
    for (toml, json) in [
        ("", "broken"),
        ("[features]\nhooks = false", "{}"),
        ("[[hooks.PostToolUse]]", "{}"),
        (
            "",
            r#"{"hooks":{"PostToolUse":[{"matcher":"Bash","hooks":[{"type":"command","command":"regurgitate record-hook --agent codex"}]}]}}"#,
        ),
    ] {
        let temp = tempdir().unwrap();
        let config = temp.path().join("config.toml");
        let hooks = temp.path().join("hooks.json");
        fs::write(&config, toml).unwrap();
        fs::write(&hooks, json).unwrap();
        assert_eq!(
            inspect_codex_hook(&config).unwrap(),
            HookReadiness::Conflicting
        );
        assert!(install_codex_hook_command(&config, NEW, true).is_err());
        assert_eq!(fs::read_to_string(&hooks).unwrap(), json);
        assert_eq!(fs::read_to_string(&config).unwrap(), toml);
    }
}

#[test]
fn explicit_json_preview_creates_nothing() {
    let temp = tempdir().unwrap();
    let hooks = temp.path().join("nested/hooks.json");
    assert_eq!(
        install_codex_hook_command(&hooks, NEW, false)
            .unwrap()
            .status,
        InstallStatus::Planned
    );
    assert!(!hooks.parent().unwrap().exists());
}

#[test]
fn duplicate_json_hooks_require_review_without_rewriting() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("hooks.json");
    let original = serde_json::json!({"hooks":{"PostToolUse":[{"hooks":[
        {"type":"command","command":OLD}, {"type":"command","command":NEW}
    ]}]}})
    .to_string();
    fs::write(&path, &original).unwrap();
    assert_eq!(
        inspect_codex_hook(&path).unwrap(),
        HookReadiness::Conflicting
    );
    assert!(install_codex_hook_command(&path, NEW, true).is_err());
    assert_eq!(fs::read_to_string(&path).unwrap(), original);
}

#[test]
fn legacy_praxis_status_hook_gets_actionable_non_mutating_diagnostic() {
    let temp = tempdir().unwrap();
    let config = temp.path().join("aoe.toml");
    let original = "[status_hooks]\nenabled = true\non_idle = 'praxis aoe-hook'\non_error = 'personal-command'\n";
    fs::write(&config, original).unwrap();
    let error = regurgitate::packaging::install_aoe_hook(&config, true).unwrap_err();
    assert!(error.to_string().contains("legacy Praxis"));
    assert!(error.to_string().contains("native agent recording"));
    assert_eq!(fs::read_to_string(&config).unwrap(), original);
}
