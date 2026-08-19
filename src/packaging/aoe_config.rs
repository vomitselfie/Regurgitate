use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use serde::Serialize;
use toml_edit::{DocumentMut, Table, table, value};

use crate::application::HookReadiness;

use super::{
    InstallStatus,
    config_file::{acquire_config_lock, atomic_write_config, containing_directory, read_config},
};

const AOE_HOOK_COMMAND: &str = "praxis aoe-hook";
const CONFIG_LOCK_FILENAME: &str = ".config.lock";
const OTHER_STATUS_HOOKS: [&str; 4] = ["on_starting", "on_running", "on_waiting", "on_change"];

pub const AOE_CONFIG_SNIPPET: &str = r#"# Merge into a global or profile AoE config.
# The handler reads only AOE_SESSION_ID, AOE_PROFILE, and AOE_TOOL.
[status_hooks]
enabled = true
on_idle = "praxis aoe-hook"
on_error = "praxis aoe-hook""#;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AoeHookInstallReport {
    pub status: InstallStatus,
    pub config: PathBuf,
    pub changes: Vec<&'static str>,
}

/// Inspect one explicit AoE config without acquiring a write lock or changing
/// the file. Invalid structure and occupied single-command slots are reported
/// as controlled conflicts; read failures remain backend errors for the health
/// service to sanitize.
pub fn inspect_aoe_hook(config: &Path) -> Result<HookReadiness> {
    let content = read_config(config, "AoE")?;
    Ok(match prepare_config(&content) {
        Ok(prepared) if prepared.changes.is_empty() => HookReadiness::Installed,
        Ok(_) => HookReadiness::NotInstalled,
        Err(_) => HookReadiness::Conflicting,
    })
}

struct PreparedConfig {
    content: String,
    changes: Vec<&'static str>,
}

/// Preview or conservatively add Praxis to one explicit global AoE config.
/// Existing hook commands are never composed or replaced.
pub fn install_aoe_hook(config: &Path, apply: bool) -> Result<AoeHookInstallReport> {
    let prepared = prepare_config(&read_config(config, "AoE")?)?;
    if prepared.changes.is_empty() {
        return Ok(report(
            InstallStatus::AlreadyCurrent,
            config,
            prepared.changes,
        ));
    }
    if !apply {
        return Ok(report(InstallStatus::Planned, config, prepared.changes));
    }

    let config_directory = containing_directory(config);
    fs::create_dir_all(config_directory).with_context(|| {
        format!(
            "could not create AoE config directory at {}",
            config_directory.display()
        )
    })?;
    let _lock = acquire_config_lock(config_directory, CONFIG_LOCK_FILENAME, "AoE")?;

    // Load and validate again under AoE's adjacent config lock. The preview is
    // informational; only this fresh document may be written.
    let prepared = prepare_config(&read_config(config, "AoE")?)?;
    if prepared.changes.is_empty() {
        return Ok(report(
            InstallStatus::AlreadyCurrent,
            config,
            prepared.changes,
        ));
    }
    atomic_write_config(config, prepared.content.as_bytes(), "AoE")?;
    Ok(report(InstallStatus::Installed, config, prepared.changes))
}

fn prepare_config(content: &str) -> Result<PreparedConfig> {
    let mut document = if content.trim().is_empty() {
        DocumentMut::new()
    } else {
        content
            .parse::<DocumentMut>()
            .context("AoE config is not valid TOML")?
    };

    if document.get("status_hooks").is_none() {
        document["status_hooks"] = table();
    }
    let status_hooks = document["status_hooks"]
        .as_table_mut()
        .context("AoE status_hooks must be a TOML table")?;

    let enabled = status_hooks
        .get("enabled")
        .map(|item| {
            item.as_bool()
                .context("AoE status_hooks.enabled must be a boolean")
        })
        .transpose()?;
    let add_idle = hook_needs_install(status_hooks, "on_idle")?;
    let add_error = hook_needs_install(status_hooks, "on_error")?;

    if enabled != Some(true)
        && OTHER_STATUS_HOOKS
            .iter()
            .any(|key| status_hooks.contains_key(key))
    {
        bail!("refusing to enable existing dormant AoE status hooks; enable them explicitly first");
    }

    let mut changes = Vec::with_capacity(3);
    if enabled != Some(true) {
        changes.push("status_hooks.enabled");
    }
    if add_idle {
        changes.push("status_hooks.on_idle");
    }
    if add_error {
        changes.push("status_hooks.on_error");
    }

    if changes.contains(&"status_hooks.enabled") {
        set_enabled(status_hooks);
    }
    if add_idle {
        status_hooks.insert("on_idle", value(AOE_HOOK_COMMAND));
    }
    if add_error {
        status_hooks.insert("on_error", value(AOE_HOOK_COMMAND));
    }

    Ok(PreparedConfig {
        content: document.to_string(),
        changes,
    })
}

fn hook_needs_install(status_hooks: &Table, key: &str) -> Result<bool> {
    let Some(item) = status_hooks.get(key) else {
        return Ok(true);
    };
    match item.as_str() {
        Some(AOE_HOOK_COMMAND) => Ok(false),
        Some(_) => bail!("AoE {key} is already configured; refusing to replace it"),
        None => bail!("AoE {key} must be a string; refusing to replace it"),
    }
}

fn set_enabled(status_hooks: &mut Table) {
    let Some(existing) = status_hooks.get_mut("enabled") else {
        status_hooks.insert("enabled", value(true));
        return;
    };

    let decor = existing
        .as_value()
        .map(|current| current.decor().clone())
        .unwrap_or_default();
    let mut replacement = value(true);
    if let Some(value) = replacement.as_value_mut() {
        *value.decor_mut() = decor;
    }
    *existing = replacement;
}

fn report(
    status: InstallStatus,
    config: &Path,
    changes: Vec<&'static str>,
) -> AoeHookInstallReport {
    AoeHookInstallReport {
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
    fn preview_reports_changes_without_writing() {
        let temp = tempdir().unwrap();
        let config = temp.path().join("aoe/config.toml");

        let report = install_aoe_hook(&config, false).unwrap();

        assert_eq!(report.status, InstallStatus::Planned);
        assert_eq!(
            report.changes,
            [
                "status_hooks.enabled",
                "status_hooks.on_idle",
                "status_hooks.on_error"
            ]
        );
        assert!(!config.exists());
        assert!(!config.parent().unwrap().exists());
    }

    #[test]
    fn apply_preserves_comments_and_active_unrelated_hooks() {
        let temp = tempdir().unwrap();
        let config = temp.path().join("config.toml");
        fs::write(
            &config,
            concat!(
                "# personal settings\n",
                "[status_hooks]\n",
                "enabled = true # keep enabled\n",
                "on_waiting = \"notify-send waiting\"\n",
            ),
        )
        .unwrap();

        let report = install_aoe_hook(&config, true).unwrap();

        assert_eq!(report.status, InstallStatus::Installed);
        assert_eq!(
            report.changes,
            ["status_hooks.on_idle", "status_hooks.on_error"]
        );
        let written = fs::read_to_string(config).unwrap();
        assert!(written.contains("# personal settings"));
        assert!(written.contains("enabled = true # keep enabled"));
        assert!(written.contains("on_waiting = \"notify-send waiting\""));
        assert!(written.contains("on_idle = \"praxis aoe-hook\""));
        assert!(written.contains("on_error = \"praxis aoe-hook\""));
    }

    #[test]
    fn conflicting_hook_is_preserved_and_rejected() {
        let temp = tempdir().unwrap();
        let config = temp.path().join("config.toml");
        let original = "[status_hooks]\nenabled = true\non_idle = \"notify-send idle\"\n";
        fs::write(&config, original).unwrap();

        let error = install_aoe_hook(&config, true).unwrap_err();

        assert!(error.to_string().contains("on_idle is already configured"));
        assert_eq!(fs::read_to_string(config).unwrap(), original);
    }

    #[test]
    fn dormant_unrelated_hooks_are_not_activated() {
        let temp = tempdir().unwrap();
        let config = temp.path().join("config.toml");
        let original = "[status_hooks]\nenabled = false\non_waiting = \"notify-send waiting\"\n";
        fs::write(&config, original).unwrap();

        let error = install_aoe_hook(&config, true).unwrap_err();

        assert!(error.to_string().contains("dormant AoE status hooks"));
        assert_eq!(fs::read_to_string(config).unwrap(), original);
    }

    #[test]
    fn repeated_install_is_current() {
        let temp = tempdir().unwrap();
        let config = temp.path().join("config.toml");

        assert_eq!(
            install_aoe_hook(&config, true).unwrap().status,
            InstallStatus::Installed
        );
        let repeated = install_aoe_hook(&config, true).unwrap();

        assert_eq!(repeated.status, InstallStatus::AlreadyCurrent);
        assert!(repeated.changes.is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn new_config_and_lock_are_owner_only() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempdir().unwrap();
        let config = temp.path().join("aoe/config.toml");

        install_aoe_hook(&config, true).unwrap();

        let config_mode = fs::metadata(&config).unwrap().permissions().mode() & 0o777;
        let lock_mode = fs::metadata(config.parent().unwrap().join(CONFIG_LOCK_FILENAME))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(config_mode, 0o600);
        assert_eq!(lock_mode, 0o600);
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_config_is_preserved() {
        use std::os::unix::fs::symlink;

        let temp = tempdir().unwrap();
        let aoe_directory = temp.path().join("aoe");
        let dotfiles_directory = temp.path().join("dotfiles");
        fs::create_dir(&aoe_directory).unwrap();
        fs::create_dir(&dotfiles_directory).unwrap();
        let target = dotfiles_directory.join("aoe.toml");
        fs::write(&target, "# managed in dotfiles\n").unwrap();
        let config = aoe_directory.join("config.toml");
        symlink(&target, &config).unwrap();

        install_aoe_hook(&config, true).unwrap();

        assert!(
            fs::symlink_metadata(&config)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        let written = fs::read_to_string(target).unwrap();
        assert!(written.contains("# managed in dotfiles"));
        assert!(written.contains("on_idle = \"praxis aoe-hook\""));
    }

    #[test]
    fn readiness_distinguishes_missing_installed_and_conflicting() {
        let temp = tempdir().unwrap();
        let config = temp.path().join("config.toml");
        assert_eq!(
            inspect_aoe_hook(&config).unwrap(),
            HookReadiness::NotInstalled
        );

        fs::write(
            &config,
            AOE_CONFIG_SNIPPET
                .lines()
                .skip(2)
                .collect::<Vec<_>>()
                .join("\n"),
        )
        .unwrap();
        assert_eq!(inspect_aoe_hook(&config).unwrap(), HookReadiness::Installed);

        fs::write(
            &config,
            "[status_hooks]\nenabled = true\non_idle = \"PRIVATE_COMMAND\"\n",
        )
        .unwrap();
        assert_eq!(
            inspect_aoe_hook(&config).unwrap(),
            HookReadiness::Conflicting
        );
    }
}
