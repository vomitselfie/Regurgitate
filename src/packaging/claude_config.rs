use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use serde::Serialize;
use serde_json::{Value, json};

use crate::application::HookReadiness;

use super::{
    InstallStatus,
    config_file::{acquire_config_lock, atomic_write_config, containing_directory, read_config},
};

const CLAUDE_HOOK_COMMAND: &str = "regurgitate record-hook --agent claude";
const LEGACY_CLAUDE_HOOK_COMMAND: &str = "praxis record-hook --agent claude";
const CONFIG_LOCK_FILENAME: &str = "settings.json.lock";
const HOOK_EVENTS: [&str; 2] = ["PostToolUse", "PostToolUseFailure"];

pub const CLAUDE_CONFIG_SNIPPET: &str = r#"{
  "hooks": {
    "PostToolUse": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "regurgitate record-hook --agent claude"
          }
        ]
      }
    ],
    "PostToolUseFailure": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "regurgitate record-hook --agent claude"
          }
        ]
      }
    ]
  }
}"#;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ClaudeHookInstallReport {
    pub status: InstallStatus,
    pub config: PathBuf,
    pub changes: Vec<&'static str>,
}

struct PreparedConfig {
    content: String,
    changes: Vec<&'static str>,
}

pub fn inspect_claude_hook(config: &Path) -> Result<HookReadiness> {
    inspect_claude_hook_command(config, CLAUDE_HOOK_COMMAND)
}

pub fn inspect_claude_hook_command(config: &Path, hook_command: &str) -> Result<HookReadiness> {
    validate_hook_command(hook_command)?;
    let content = read_config(config, "Claude")?;
    Ok(match prepare_config(&content, hook_command) {
        Ok(prepared) if prepared.changes.is_empty() => HookReadiness::Installed,
        Ok(_) => HookReadiness::NotInstalled,
        Err(_) => HookReadiness::Conflicting,
    })
}

/// Preview or add Regurgitate to both terminal Claude tool events. Existing settings,
/// matcher groups, and personal commands are retained.
pub fn install_claude_hook(config: &Path, apply: bool) -> Result<ClaudeHookInstallReport> {
    install_claude_hook_command(config, CLAUDE_HOOK_COMMAND, apply)
}

/// Variant used by the AoE worker, whose downloaded executable is not on PATH.
pub fn install_claude_hook_command(
    config: &Path,
    hook_command: &str,
    apply: bool,
) -> Result<ClaudeHookInstallReport> {
    validate_hook_command(hook_command)?;
    let prepared = prepare_config(&read_config(config, "Claude")?, hook_command)?;
    if prepared.changes.is_empty() {
        return Ok(report(InstallStatus::AlreadyCurrent, config, Vec::new()));
    }
    if !apply {
        return Ok(report(InstallStatus::Planned, config, prepared.changes));
    }

    let config_directory = containing_directory(config);
    fs::create_dir_all(config_directory).context("could not create Claude config directory")?;
    let _lock = acquire_config_lock(config_directory, CONFIG_LOCK_FILENAME, "Claude")?;

    let prepared = prepare_config(&read_config(config, "Claude")?, hook_command)?;
    if prepared.changes.is_empty() {
        return Ok(report(InstallStatus::AlreadyCurrent, config, Vec::new()));
    }
    atomic_write_config(config, prepared.content.as_bytes(), "Claude")?;
    Ok(report(InstallStatus::Installed, config, prepared.changes))
}

fn prepare_config(content: &str, hook_command: &str) -> Result<PreparedConfig> {
    let mut document = if content.trim().is_empty() {
        json!({})
    } else {
        serde_json::from_str::<Value>(content).context("Claude settings are not valid JSON")?
    };
    let root = document
        .as_object_mut()
        .context("Claude settings must be a JSON object")?;
    match root.get("disableAllHooks") {
        Some(Value::Bool(true)) => bail!("Claude hooks are disabled"),
        Some(Value::Bool(false)) | None => {}
        Some(_) => bail!("Claude disableAllHooks must be a boolean"),
    }

    let hooks = root
        .entry("hooks")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .context("Claude hooks must be a JSON object")?;
    let mut changes = Vec::with_capacity(HOOK_EVENTS.len());

    for event in HOOK_EVENTS {
        let groups = hooks
            .entry(event)
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .with_context(|| format!("Claude hooks.{event} must be an array"))?;
        let migrated = migrate_legacy_hooks(groups, hook_command)?;
        match regurgitate_hook_coverage(groups, hook_command)? {
            RegurgitateHookCoverage::AllTools => {
                if migrated {
                    changes.push(event);
                }
                continue;
            }
            RegurgitateHookCoverage::Restricted => {
                bail!("Regurgitate Claude {event} hook is already restricted by a matcher")
            }
            RegurgitateHookCoverage::Missing => {}
        }
        groups.push(json!({
            "hooks": [{
                "type": "command",
                "command": hook_command
            }]
        }));
        changes.push(event);
    }

    let mut content = serde_json::to_string_pretty(&document)?;
    content.push('\n');
    Ok(PreparedConfig { content, changes })
}

fn migrate_legacy_hooks(groups: &mut [Value], hook_command: &str) -> Result<bool> {
    let mut migrated = false;
    for group in groups {
        let group = group
            .as_object_mut()
            .context("Claude hook groups must be JSON objects")?;
        let restricted = match group.get("matcher") {
            None => false,
            Some(Value::String(matcher)) => !matcher.is_empty() && matcher != "*",
            Some(_) => bail!("Claude hook matcher must be a string"),
        };
        let Some(handlers) = group.get_mut("hooks") else {
            continue;
        };
        let handlers = handlers
            .as_array_mut()
            .context("Claude hook group handlers must be an array")?;
        for handler in handlers {
            let handler = handler
                .as_object_mut()
                .context("Claude hook handlers must be JSON objects")?;
            if handler.get("type").and_then(Value::as_str) != Some("command") {
                continue;
            }
            let Some(command) = handler.get("command").and_then(Value::as_str) else {
                continue;
            };
            if !is_legacy_hook_command(command) {
                continue;
            }
            if restricted {
                bail!("legacy Regurgitate Claude hook is restricted by a matcher");
            }
            handler.insert("command".to_owned(), Value::String(hook_command.to_owned()));
            migrated = true;
        }
    }
    Ok(migrated)
}

fn is_legacy_hook_command(command: &str) -> bool {
    if command == LEGACY_CLAUDE_HOOK_COMMAND {
        return true;
    }
    command
        .strip_suffix(" record-hook --agent claude")
        .is_some_and(|executable| {
            executable.ends_with("/praxis") || executable.ends_with("/praxis'")
        })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RegurgitateHookCoverage {
    Missing,
    AllTools,
    Restricted,
}

fn regurgitate_hook_coverage(
    groups: &[Value],
    hook_command: &str,
) -> Result<RegurgitateHookCoverage> {
    let mut found_restricted = false;
    for group in groups {
        let group = group
            .as_object()
            .context("Claude hook groups must be JSON objects")?;
        let Some(handlers) = group.get("hooks") else {
            continue;
        };
        let handlers = handlers
            .as_array()
            .context("Claude hook group handlers must be an array")?;
        let contains_regurgitate = handlers.iter().try_fold(false, |found, handler| {
            let handler = handler
                .as_object()
                .context("Claude hook handlers must be JSON objects")?;
            let matches = handler.get("type").and_then(Value::as_str) == Some("command")
                && handler
                    .get("command")
                    .and_then(Value::as_str)
                    .is_some_and(|command| {
                        command == hook_command || command == CLAUDE_HOOK_COMMAND
                    });
            Ok::<_, anyhow::Error>(found || matches)
        })?;
        if !contains_regurgitate {
            continue;
        }
        match group.get("matcher") {
            None => return Ok(RegurgitateHookCoverage::AllTools),
            Some(Value::String(matcher)) if matcher.is_empty() || matcher == "*" => {
                return Ok(RegurgitateHookCoverage::AllTools);
            }
            Some(Value::String(_)) => found_restricted = true,
            Some(_) => bail!("Claude hook matcher must be a string"),
        }
    }
    Ok(if found_restricted {
        RegurgitateHookCoverage::Restricted
    } else {
        RegurgitateHookCoverage::Missing
    })
}

fn validate_hook_command(command: &str) -> Result<()> {
    if command.is_empty() || command.chars().any(|character| character.is_control()) {
        bail!("Regurgitate hook command must be non-empty and single-line");
    }
    Ok(())
}

fn report(
    status: InstallStatus,
    config: &Path,
    changes: Vec<&'static str>,
) -> ClaudeHookInstallReport {
    ClaudeHookInstallReport {
        status,
        config: config.to_path_buf(),
        changes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn snippet_is_valid_and_covers_both_terminal_tool_events() {
        let value: Value = serde_json::from_str(CLAUDE_CONFIG_SNIPPET).unwrap();
        let hooks = value["hooks"].as_object().unwrap();
        assert_eq!(hooks.len(), 2);
        for event in HOOK_EVENTS {
            assert_eq!(hooks[event][0]["hooks"][0]["command"], CLAUDE_HOOK_COMMAND);
        }
    }

    #[test]
    fn preview_does_not_create_config_or_lock() {
        let temp = tempdir().unwrap();
        let config = temp.path().join("claude/settings.json");

        let report = install_claude_hook(&config, false).unwrap();

        assert_eq!(report.status, InstallStatus::Planned);
        assert_eq!(report.changes, HOOK_EVENTS);
        assert!(!config.exists());
        assert!(!config.parent().unwrap().exists());
    }

    #[test]
    fn apply_preserves_unrelated_settings_and_personal_hooks() {
        let temp = tempdir().unwrap();
        let config = temp.path().join("settings.json");
        fs::write(
            &config,
            r#"{
                "apiKeyHelper": "PRIVATE_SECRET",
                "hooks": {
                    "PostToolUse": [{"hooks": [{"type": "command", "command": "PRIVATE_COMMAND"}] }]
                }
            }"#,
        )
        .unwrap();

        let report = install_claude_hook(&config, true).unwrap();

        assert_eq!(report.status, InstallStatus::Installed);
        let written = fs::read_to_string(&config).unwrap();
        assert!(written.contains("PRIVATE_SECRET"));
        assert!(written.contains("PRIVATE_COMMAND"));
        assert_eq!(
            inspect_claude_hook(&config).unwrap(),
            HookReadiness::Installed
        );
    }

    #[test]
    fn repeated_install_is_current_without_duplication() {
        let temp = tempdir().unwrap();
        let config = temp.path().join("settings.json");
        install_claude_hook(&config, true).unwrap();
        let before = fs::read_to_string(&config).unwrap();

        let repeated = install_claude_hook(&config, true).unwrap();

        assert_eq!(repeated.status, InstallStatus::AlreadyCurrent);
        assert_eq!(fs::read_to_string(config).unwrap(), before);
    }

    #[test]
    fn disabled_or_restricted_hooks_are_preserved_as_conflicts() {
        let temp = tempdir().unwrap();
        let disabled = temp.path().join("disabled.json");
        fs::write(&disabled, r#"{"disableAllHooks":true}"#).unwrap();
        assert!(install_claude_hook(&disabled, true).is_err());
        assert_eq!(
            inspect_claude_hook(&disabled).unwrap(),
            HookReadiness::Conflicting
        );

        let restricted = temp.path().join("restricted.json");
        fs::write(
            &restricted,
            format!(
                r#"{{"hooks":{{"PostToolUse":[{{"matcher":"Edit","hooks":[{{"type":"command","command":"{CLAUDE_HOOK_COMMAND}"}}]}}]}}}}"#
            ),
        )
        .unwrap();
        assert!(install_claude_hook(&restricted, true).is_err());
        assert_eq!(
            inspect_claude_hook(&restricted).unwrap(),
            HookReadiness::Conflicting
        );

        let malformed = temp.path().join("malformed.json");
        fs::write(
            &malformed,
            format!(
                r#"{{"hooks":{{"PostToolUse":[{{"matcher":true,"hooks":[{{"type":"command","command":"{CLAUDE_HOOK_COMMAND}"}}]}}]}}}}"#
            ),
        )
        .unwrap();
        assert!(install_claude_hook(&malformed, true).is_err());
        assert_eq!(
            inspect_claude_hook(&malformed).unwrap(),
            HookReadiness::Conflicting
        );
    }

    #[test]
    fn explicit_worker_command_is_installed_and_recognized() {
        let temp = tempdir().unwrap();
        let config = temp.path().join("settings.json");
        let command = "'/plugin home/regurgitate' record-hook --agent claude";

        install_claude_hook_command(&config, command, true).unwrap();

        assert_eq!(
            inspect_claude_hook_command(&config, command).unwrap(),
            HookReadiness::Installed
        );
        assert!(fs::read_to_string(config).unwrap().contains(command));
    }

    #[test]
    fn legacy_hooks_are_replaced_instead_of_duplicated() {
        let temp = tempdir().unwrap();
        let config = temp.path().join("settings.json");
        fs::write(
            &config,
            r#"{"hooks":{"PostToolUse":[{"hooks":[{"type":"command","command":"praxis record-hook --agent claude"}]}],"PostToolUseFailure":[{"hooks":[{"type":"command","command":"praxis record-hook --agent claude"}]}]}}"#,
        )
        .unwrap();

        let preview = install_claude_hook(&config, false).unwrap();
        assert_eq!(preview.status, InstallStatus::Planned);
        install_claude_hook(&config, true).unwrap();

        let written = fs::read_to_string(config).unwrap();
        assert_eq!(written.matches(CLAUDE_HOOK_COMMAND).count(), 2);
        assert!(!written.contains(LEGACY_CLAUDE_HOOK_COMMAND));
    }

    #[cfg(unix)]
    #[test]
    fn new_config_and_lock_are_owner_only() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempdir().unwrap();
        let config = temp.path().join("claude/settings.json");
        install_claude_hook(&config, true).unwrap();

        assert_eq!(
            fs::metadata(&config).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let lock = config.parent().unwrap().join(CONFIG_LOCK_FILENAME);
        assert_eq!(
            fs::metadata(lock).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_config_is_preserved() {
        use std::os::unix::fs::symlink;

        let temp = tempdir().unwrap();
        let claude_directory = temp.path().join("claude");
        let dotfiles_directory = temp.path().join("dotfiles");
        fs::create_dir(&claude_directory).unwrap();
        fs::create_dir(&dotfiles_directory).unwrap();
        let target = dotfiles_directory.join("settings.json");
        fs::write(&target, "{}\n").unwrap();
        let config = claude_directory.join("settings.json");
        symlink(&target, &config).unwrap();

        install_claude_hook(&config, true).unwrap();

        assert!(
            fs::symlink_metadata(&config)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert!(
            fs::read_to_string(target)
                .unwrap()
                .contains(CLAUDE_HOOK_COMMAND)
        );
    }
}
