use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use serde::Serialize;
use toml_edit::{ArrayOfTables, DocumentMut, Item, Table, table, value};

use crate::application::HookReadiness;

use super::{
    InstallStatus,
    config_file::{acquire_config_lock, atomic_write_config, containing_directory, read_config},
};

const CODEX_HOOK_COMMAND: &str = "regurgitate record-hook --agent codex";
const CONFIG_LOCK_FILENAME: &str = "config.toml.lock";

pub const CODEX_CONFIG_SNIPPET: &str = r#"# Merge into the user-level Codex config.toml.
[[hooks.PostToolUse]]

[[hooks.PostToolUse.hooks]]
type = "command"
command = "regurgitate record-hook --agent codex"
timeout = 5"#;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CodexHookInstallReport {
    pub status: InstallStatus,
    pub config: PathBuf,
    pub changes: Vec<&'static str>,
}

struct PreparedConfig {
    content: String,
    changes: Vec<&'static str>,
}

pub fn inspect_codex_hook(config: &Path) -> Result<HookReadiness> {
    inspect_codex_hook_command(config, CODEX_HOOK_COMMAND)
}

pub fn inspect_codex_hook_command(config: &Path, hook_command: &str) -> Result<HookReadiness> {
    validate_hook_command(hook_command)?;
    let content = read_config(config, "Codex")?;
    Ok(match prepare_config(&content, hook_command) {
        Ok(prepared) if prepared.changes.is_empty() => HookReadiness::Installed,
        Ok(_) => HookReadiness::NotInstalled,
        Err(_) => HookReadiness::Conflicting,
    })
}

/// Preview or append one additive PostToolUse group to an explicit user-level
/// Codex config. Existing matcher groups and handlers are preserved.
pub fn install_codex_hook(config: &Path, apply: bool) -> Result<CodexHookInstallReport> {
    install_codex_hook_command(config, CODEX_HOOK_COMMAND, apply)
}

/// Preview or install a PostToolUse hook using a specific Regurgitate executable.
/// This is used by the AoE worker, whose downloaded binary is not on PATH.
pub fn install_codex_hook_command(
    config: &Path,
    hook_command: &str,
    apply: bool,
) -> Result<CodexHookInstallReport> {
    validate_hook_command(hook_command)?;
    let prepared = prepare_config(&read_config(config, "Codex")?, hook_command)?;
    if prepared.changes.is_empty() {
        return Ok(report(InstallStatus::AlreadyCurrent, config, Vec::new()));
    }
    if !apply {
        return Ok(report(InstallStatus::Planned, config, prepared.changes));
    }

    let config_directory = containing_directory(config);
    fs::create_dir_all(config_directory).context("could not create Codex config directory")?;
    let _lock = acquire_config_lock(config_directory, CONFIG_LOCK_FILENAME, "Codex")?;

    // The preview is informational. Re-read and validate while holding the
    // same adjacent lock used by Codex config writers.
    let prepared = prepare_config(&read_config(config, "Codex")?, hook_command)?;
    if prepared.changes.is_empty() {
        return Ok(report(InstallStatus::AlreadyCurrent, config, Vec::new()));
    }
    atomic_write_config(config, prepared.content.as_bytes(), "Codex")?;
    Ok(report(InstallStatus::Installed, config, prepared.changes))
}

fn prepare_config(content: &str, hook_command: &str) -> Result<PreparedConfig> {
    let mut document = if content.trim().is_empty() {
        DocumentMut::new()
    } else {
        content
            .parse::<DocumentMut>()
            .context("Codex config is not valid TOML")?
    };
    ensure_hooks_enabled(&document)?;

    if document.get("hooks").is_none() {
        document["hooks"] = table();
    }
    let hooks = document["hooks"]
        .as_table_mut()
        .context("Codex hooks must be a TOML table")?;

    if hooks.get("PostToolUse").is_none() {
        hooks.insert("PostToolUse", Item::ArrayOfTables(ArrayOfTables::new()));
    }
    let groups = hooks["PostToolUse"]
        .as_array_of_tables_mut()
        .context("Codex hooks.PostToolUse must be an array of tables")?;
    match regurgitate_hook_coverage(groups, hook_command) {
        RegurgitateHookCoverage::AllTools => {
            return Ok(PreparedConfig {
                content: document.to_string(),
                changes: Vec::new(),
            });
        }
        RegurgitateHookCoverage::Restricted => {
            bail!("Regurgitate Codex hook is already restricted by a matcher");
        }
        RegurgitateHookCoverage::Missing => {}
    }

    let mut handler = Table::new();
    handler.insert("type", value("command"));
    handler.insert("command", value(hook_command));
    handler.insert("timeout", value(5));
    let mut handlers = ArrayOfTables::new();
    handlers.push(handler);

    let mut group = Table::new();
    group.insert("hooks", Item::ArrayOfTables(handlers));
    groups.push(group);

    Ok(PreparedConfig {
        content: document.to_string(),
        changes: vec!["hooks.PostToolUse"],
    })
}

fn ensure_hooks_enabled(document: &DocumentMut) -> Result<()> {
    let Some(features) = document.get("features") else {
        return Ok(());
    };
    let features = features
        .as_table_like()
        .context("Codex features must be a TOML table")?;
    for key in ["hooks", "codex_hooks"] {
        let Some(value) = features.get(key) else {
            continue;
        };
        match value.as_bool() {
            Some(true) => {}
            Some(false) => bail!("Codex lifecycle hooks are disabled"),
            None => bail!("Codex features.{key} must be a boolean"),
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RegurgitateHookCoverage {
    Missing,
    AllTools,
    Restricted,
}

fn regurgitate_hook_coverage(
    groups: &ArrayOfTables,
    hook_command: &str,
) -> RegurgitateHookCoverage {
    let mut found_restricted = false;
    for group in groups {
        let contains_regurgitate =
            group
                .get("hooks")
                .and_then(Item::as_array_of_tables)
                .is_some_and(|handlers| {
                    handlers.iter().any(|handler| {
                        handler.get("type").and_then(Item::as_str) == Some("command")
                            && handler.get("command").and_then(Item::as_str).is_some_and(
                                |command| command == hook_command || command == CODEX_HOOK_COMMAND,
                            )
                    })
                });
        if !contains_regurgitate {
            continue;
        }
        match group.get("matcher").and_then(Item::as_str) {
            None | Some("" | "*") => return RegurgitateHookCoverage::AllTools,
            Some(_) => found_restricted = true,
        }
    }
    if found_restricted {
        RegurgitateHookCoverage::Restricted
    } else {
        RegurgitateHookCoverage::Missing
    }
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
) -> CodexHookInstallReport {
    CodexHookInstallReport {
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
    fn preview_does_not_create_config_or_lock() {
        let temp = tempdir().unwrap();
        let config = temp.path().join("codex/config.toml");

        let report = install_codex_hook(&config, false).unwrap();

        assert_eq!(report.status, InstallStatus::Planned);
        assert_eq!(report.changes, ["hooks.PostToolUse"]);
        assert!(!config.exists());
        assert!(!config.parent().unwrap().exists());
    }

    #[test]
    fn apply_preserves_unrelated_settings_hooks_and_comments() {
        let temp = tempdir().unwrap();
        let config = temp.path().join("config.toml");
        fs::write(
            &config,
            concat!(
                "# personal Codex config\n",
                "model = \"PRIVATE_MODEL\"\n",
                "[[hooks.SessionStart]]\n",
                "matcher = \"startup\"\n",
                "[[hooks.SessionStart.hooks]]\n",
                "type = \"command\"\n",
                "command = \"PRIVATE_COMMAND\"\n",
            ),
        )
        .unwrap();

        let report = install_codex_hook(&config, true).unwrap();

        assert_eq!(report.status, InstallStatus::Installed);
        let written = fs::read_to_string(config).unwrap();
        assert!(written.contains("# personal Codex config"));
        assert!(written.contains("model = \"PRIVATE_MODEL\""));
        assert!(written.contains("command = \"PRIVATE_COMMAND\""));
        assert!(written.contains(CODEX_HOOK_COMMAND));
        assert_eq!(
            inspect_codex_hook(temp.path().join("config.toml").as_path()).unwrap(),
            HookReadiness::Installed
        );
    }

    #[test]
    fn repeated_install_is_current_without_duplication() {
        let temp = tempdir().unwrap();
        let config = temp.path().join("config.toml");
        install_codex_hook(&config, true).unwrap();
        let before = fs::read_to_string(&config).unwrap();

        let repeated = install_codex_hook(&config, true).unwrap();

        assert_eq!(repeated.status, InstallStatus::AlreadyCurrent);
        assert!(repeated.changes.is_empty());
        assert_eq!(fs::read_to_string(config).unwrap(), before);
    }

    #[test]
    fn disabled_hooks_are_preserved_and_reported_as_conflicting() {
        let temp = tempdir().unwrap();
        let config = temp.path().join("config.toml");
        let original = "[features]\nhooks = false\n";
        fs::write(&config, original).unwrap();

        let error = install_codex_hook(&config, true).unwrap_err();

        assert!(error.to_string().contains("lifecycle hooks are disabled"));
        assert_eq!(fs::read_to_string(&config).unwrap(), original);
        assert_eq!(
            inspect_codex_hook(&config).unwrap(),
            HookReadiness::Conflicting
        );
    }

    #[test]
    fn restricted_existing_regurgitate_hook_is_preserved_and_reported_as_conflicting() {
        let temp = tempdir().unwrap();
        let config = temp.path().join("config.toml");
        let original = concat!(
            "[[hooks.PostToolUse]]\n",
            "matcher = \"^Bash$\"\n",
            "[[hooks.PostToolUse.hooks]]\n",
            "type = \"command\"\n",
            "command = \"regurgitate record-hook --agent codex\"\n",
        );
        fs::write(&config, original).unwrap();

        let error = install_codex_hook(&config, true).unwrap_err();

        assert!(error.to_string().contains("restricted by a matcher"));
        assert_eq!(fs::read_to_string(&config).unwrap(), original);
        assert_eq!(
            inspect_codex_hook(&config).unwrap(),
            HookReadiness::Conflicting
        );
    }

    #[cfg(unix)]
    #[test]
    fn new_config_and_lock_are_owner_only() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempdir().unwrap();
        let config = temp.path().join("codex/config.toml");

        install_codex_hook(&config, true).unwrap();

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
        use std::os::unix::fs::{PermissionsExt, symlink};

        let temp = tempdir().unwrap();
        let codex_directory = temp.path().join("codex");
        let dotfiles_directory = temp.path().join("dotfiles");
        fs::create_dir(&codex_directory).unwrap();
        fs::create_dir(&dotfiles_directory).unwrap();
        let target = dotfiles_directory.join("codex.toml");
        fs::write(&target, "# managed in dotfiles\n").unwrap();
        let config = codex_directory.join("config.toml");
        symlink(&target, &config).unwrap();

        install_codex_hook(&config, true).unwrap();

        assert!(
            fs::symlink_metadata(&config)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert!(
            fs::read_to_string(target)
                .unwrap()
                .contains(CODEX_HOOK_COMMAND)
        );
        let lock = codex_directory.join(CONFIG_LOCK_FILENAME);
        assert_eq!(
            fs::metadata(lock).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    #[test]
    fn snippet_is_valid_and_matches_the_installer() {
        let document = CODEX_CONFIG_SNIPPET.parse::<DocumentMut>().unwrap();
        let rendered = document.to_string();
        assert!(rendered.contains(CODEX_HOOK_COMMAND));
        assert!(rendered.contains("timeout = 5"));
    }
}
