//! Codex's sibling hooks.json representation. Never rewrites config.toml.
use super::{
    CodexHookInstallReport, InstallStatus,
    config_file::{acquire_config_lock, atomic_write_config, containing_directory, read_config},
    hook_command::same_stock_hook,
};
use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use std::{fs, path::Path};

pub(super) fn prepare(content: &str, command: &str) -> Result<(String, bool)> {
    let mut doc: Value = if content.trim().is_empty() {
        json!({})
    } else {
        serde_json::from_str(content).context("Codex hooks.json is not valid JSON")?
    };
    let root = doc
        .as_object_mut()
        .context("Codex hooks.json must be an object")?;
    let hooks = root
        .entry("hooks")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .context("Codex hooks must be an object")?;
    let groups = hooks
        .entry("PostToolUse")
        .or_insert_with(|| json!([]))
        .as_array_mut()
        .context("Codex PostToolUse must be an array")?;
    let mut found = false;
    let mut changed = false;
    for group in groups.iter_mut() {
        let group = group
            .as_object_mut()
            .context("Codex hook group must be an object")?;
        let unrestricted = match group.get("matcher") {
            None => true,
            Some(Value::String(s)) => s.is_empty() || s == "*",
            _ => bail!("Codex hook matcher must be a string"),
        };
        let handlers = group
            .get_mut("hooks")
            .and_then(Value::as_array_mut)
            .context("Codex handlers must be an array")?;
        for handler in handlers {
            let handler = handler
                .as_object_mut()
                .context("Codex handler must be an object")?;
            if handler.get("type").and_then(Value::as_str) != Some("command") {
                continue;
            }
            let Some(existing) = handler.get("command").and_then(Value::as_str) else {
                continue;
            };
            if existing != command
                && !same_stock_hook(existing, "regurgitate record-hook --agent codex")
            {
                continue;
            }
            if !unrestricted {
                bail!("Regurgitate Codex hook is restricted by a matcher");
            }
            if found {
                bail!("Duplicate Regurgitate Codex hooks require manual review");
            }
            found = true;
            if existing != command && command != "regurgitate record-hook --agent codex" {
                handler.insert("command".into(), json!(command));
                changed = true;
            }
        }
    }
    if !found {
        groups.push(json!({"hooks": [{"type":"command", "command":command, "timeout":5}]}));
        changed = true;
    }
    Ok((
        format!("{}\n", serde_json::to_string_pretty(&doc)?),
        changed,
    ))
}

pub(super) fn install(path: &Path, command: &str, apply: bool) -> Result<CodexHookInstallReport> {
    let (_, changed) = prepare(&read_config(path, "Codex")?, command)?;
    let mut status = if changed {
        InstallStatus::Planned
    } else {
        InstallStatus::AlreadyCurrent
    };
    if changed && apply {
        fs::create_dir_all(containing_directory(path))?;
        let _lock = acquire_config_lock(containing_directory(path), "hooks.json.lock", "Codex")?;
        let (content, changed) = prepare(&read_config(path, "Codex")?, command)?;
        if changed {
            atomic_write_config(path, content.as_bytes(), "Codex")?;
        }
        status = if changed {
            InstallStatus::Installed
        } else {
            InstallStatus::AlreadyCurrent
        };
    }
    Ok(CodexHookInstallReport {
        status,
        config: path.into(),
        changes: if matches!(status, InstallStatus::Planned | InstallStatus::Installed) {
            vec!["hooks.PostToolUse"]
        } else {
            vec![]
        },
    })
}
