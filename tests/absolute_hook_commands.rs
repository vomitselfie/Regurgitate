use std::fs;

use regurgitate::{
    application::HookReadiness,
    packaging::{
        InstallStatus, inspect_claude_hook, inspect_codex_hook, install_claude_hook_command,
        install_codex_hook_command,
    },
};
use tempfile::tempdir;

#[test]
fn custom_commands_are_preserved_and_not_reported_as_stock_hooks() {
    let temp = tempdir().unwrap();
    let codex = temp.path().join("config.toml");
    let custom = "'/personal/regurgitate' record-hook --agent codex --custom";
    install_codex_hook_command(&codex, custom, true).unwrap();
    assert_eq!(
        inspect_codex_hook(&codex).unwrap(),
        HookReadiness::NotInstalled
    );
    install_codex_hook_command(&codex, "'/new/regurgitate' record-hook --agent codex", true)
        .unwrap();
    assert!(fs::read_to_string(&codex).unwrap().contains(custom));

    let claude = temp.path().join("settings.json");
    let custom = "'/personal/regurgitate' record-hook --agent claude --custom";
    install_claude_hook_command(&claude, custom, true).unwrap();
    assert_eq!(
        inspect_claude_hook(&claude).unwrap(),
        HookReadiness::NotInstalled
    );
    install_claude_hook_command(
        &claude,
        "'/new/regurgitate' record-hook --agent claude",
        true,
    )
    .unwrap();
    assert!(fs::read_to_string(&claude).unwrap().contains(custom));
}

#[test]
fn claude_recording_does_not_substitute_for_preflight() {
    let temp = tempdir().unwrap();
    let config = temp.path().join("settings.json");
    install_claude_hook_command(
        &config,
        "'/tools/regurgitate' record-hook --agent claude",
        true,
    )
    .unwrap();
    let original = fs::read_to_string(&config)
        .unwrap()
        .replace("preflight --agent claude", "record-hook --agent claude");
    fs::write(&config, &original).unwrap();
    assert_eq!(
        inspect_claude_hook(&config).unwrap(),
        HookReadiness::NotInstalled
    );
    assert_eq!(fs::read_to_string(&config).unwrap(), original);
}

#[test]
fn codex_status_and_rebinding_recognize_installer_paths_without_duplication() {
    let temp = tempdir().unwrap();
    let config = temp.path().join("config.toml");
    let old = "'/standalone tools/regurgitate' record-hook --agent codex";
    let new = "'/aoe/regurgitate' record-hook --agent codex";
    install_codex_hook_command(&config, old, true).unwrap();
    let before = fs::read_to_string(&config).unwrap();
    assert_eq!(
        inspect_codex_hook(&config).unwrap(),
        HookReadiness::Installed
    );
    assert_eq!(fs::read_to_string(&config).unwrap(), before);
    assert_eq!(
        install_codex_hook_command(&config, new, false)
            .unwrap()
            .status,
        InstallStatus::Planned
    );
    assert_eq!(fs::read_to_string(&config).unwrap(), before);
    install_codex_hook_command(&config, new, true).unwrap();
    let after = fs::read_to_string(&config).unwrap();
    assert!(!after.contains(old));
    assert_eq!(after.matches("record-hook --agent codex").count(), 1);
    assert_eq!(after.matches("preflight --agent codex").count(), 1);
    assert_eq!(
        inspect_codex_hook(&config).unwrap(),
        HookReadiness::Installed
    );
    assert_eq!(
        install_codex_hook_command(&config, new, true)
            .unwrap()
            .status,
        InstallStatus::AlreadyCurrent
    );
}

#[test]
fn claude_status_and_rebinding_cover_recording_and_preflight() {
    let temp = tempdir().unwrap();
    let config = temp.path().join("settings.json");
    let old = "'/standalone tools/regurgitate' record-hook --agent claude";
    let new = "'/aoe/regurgitate' record-hook --agent claude";
    install_claude_hook_command(&config, old, true).unwrap();
    let before = fs::read_to_string(&config).unwrap();
    assert_eq!(
        inspect_claude_hook(&config).unwrap(),
        HookReadiness::Installed
    );
    assert_eq!(fs::read_to_string(&config).unwrap(), before);
    install_claude_hook_command(&config, new, false).unwrap();
    assert_eq!(fs::read_to_string(&config).unwrap(), before);
    install_claude_hook_command(&config, new, true).unwrap();
    let after = fs::read_to_string(&config).unwrap();
    assert!(!after.contains("standalone tools"));
    assert_eq!(after.matches("record-hook --agent claude").count(), 2);
    assert_eq!(after.matches("preflight --agent claude").count(), 1);
    assert_eq!(
        inspect_claude_hook(&config).unwrap(),
        HookReadiness::Installed
    );
    assert_eq!(
        install_claude_hook_command(&config, new, true)
            .unwrap()
            .status,
        InstallStatus::AlreadyCurrent
    );
}

#[test]
fn restricted_absolute_hooks_cannot_be_silently_widened() {
    let temp = tempdir().unwrap();
    let codex = temp.path().join("config.toml");
    install_codex_hook_command(&codex, "'/old/regurgitate' record-hook --agent codex", true)
        .unwrap();
    let original = fs::read_to_string(&codex).unwrap().replace(
        "[[hooks.PostToolUse]]",
        "[[hooks.PostToolUse]]\nmatcher = 'Bash'",
    );
    fs::write(&codex, &original).unwrap();
    assert_eq!(
        inspect_codex_hook(&codex).unwrap(),
        HookReadiness::Conflicting
    );
    assert!(
        install_codex_hook_command(&codex, "'/new/regurgitate' record-hook --agent codex", true)
            .is_err()
    );
    assert_eq!(fs::read_to_string(&codex).unwrap(), original);

    let claude = temp.path().join("settings.json");
    install_claude_hook_command(
        &claude,
        "'/old/regurgitate' record-hook --agent claude",
        true,
    )
    .unwrap();
    let mut doc: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&claude).unwrap()).unwrap();
    doc["hooks"]["PostToolUse"][0]["matcher"] = serde_json::json!("Bash");
    let original = serde_json::to_string(&doc).unwrap();
    fs::write(&claude, &original).unwrap();
    assert_eq!(
        inspect_claude_hook(&claude).unwrap(),
        HookReadiness::Conflicting
    );
    assert!(
        install_claude_hook_command(
            &claude,
            "'/new/regurgitate' record-hook --agent claude",
            true
        )
        .is_err()
    );
    assert_eq!(fs::read_to_string(&claude).unwrap(), original);
}
