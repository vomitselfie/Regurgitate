use std::{fs, io::ErrorKind, path::Path};

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::application::HookReadiness;

const CLAUDE_HOOK_COMMAND: &str = "praxis record-hook --agent claude";

/// Manual Claude Code settings fragment. Praxis intentionally does not edit
/// settings files because hook arrays must be merged with the user's existing
/// commands and matchers rather than replaced.
pub const CLAUDE_CONFIG_SNIPPET: &str = r#"{
  "hooks": {
    "PostToolUse": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "praxis record-hook --agent claude"
          }
        ]
      }
    ],
    "PostToolUseFailure": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "praxis record-hook --agent claude"
          }
        ]
      }
    ]
  }
}"#;

#[derive(Deserialize, Default)]
struct ClaudeSettings {
    #[serde(default)]
    hooks: ClaudeHooks,
}

#[derive(Deserialize, Default)]
struct ClaudeHooks {
    #[serde(rename = "PostToolUse", default)]
    post_tool_use: Vec<ClaudeHookGroup>,
    #[serde(rename = "PostToolUseFailure", default)]
    post_tool_use_failure: Vec<ClaudeHookGroup>,
}

#[derive(Deserialize)]
struct ClaudeHookGroup {
    #[serde(default)]
    hooks: Vec<ClaudeCommandHook>,
}

#[derive(Deserialize)]
struct ClaudeCommandHook {
    #[serde(rename = "type")]
    hook_type: Option<String>,
    command: Option<String>,
}

/// Inspect one explicit Claude settings file without changing it. Unknown
/// settings are never represented by these typed structs. Personal hook
/// commands are transient comparison inputs and never enter the result.
pub fn inspect_claude_hook(config: &Path) -> Result<HookReadiness> {
    let content = match fs::read_to_string(config) {
        Ok(content) => content,
        Err(error) if error.kind() == ErrorKind::NotFound => String::new(),
        Err(error) => {
            return Err(error).context("could not read Claude settings");
        }
    };
    let settings: ClaudeSettings = if content.trim().is_empty() {
        ClaudeSettings::default()
    } else {
        match serde_json::from_str(&content) {
            Ok(settings) => settings,
            Err(_) => return Ok(HookReadiness::Conflicting),
        }
    };
    let success = has_praxis_hook(&settings.hooks.post_tool_use);
    let failure = has_praxis_hook(&settings.hooks.post_tool_use_failure);
    Ok(if success && failure {
        HookReadiness::Installed
    } else {
        HookReadiness::NotInstalled
    })
}

fn has_praxis_hook(groups: &[ClaudeHookGroup]) -> bool {
    groups.iter().any(|group| {
        group.hooks.iter().any(|hook| {
            hook.hook_type.as_deref() == Some("command")
                && hook.command.as_deref() == Some(CLAUDE_HOOK_COMMAND)
        })
    })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use serde_json::Value;
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn snippet_is_valid_and_covers_both_terminal_tool_events() {
        let value: Value = serde_json::from_str(CLAUDE_CONFIG_SNIPPET).unwrap();
        let hooks = value["hooks"].as_object().unwrap();
        assert_eq!(hooks.len(), 2);
        for event in ["PostToolUse", "PostToolUseFailure"] {
            assert_eq!(
                hooks[event][0]["hooks"][0]["command"],
                "praxis record-hook --agent claude"
            );
        }
    }

    #[test]
    fn readiness_ignores_unrelated_settings_and_personal_hooks() {
        let temp = tempdir().unwrap();
        let config = temp.path().join("settings.json");
        fs::write(
            &config,
            r#"{
                "apiKeyHelper": "PRIVATE_SECRET",
                "hooks": {
                    "PostToolUse": [{"hooks": [{"type": "command", "command": "PRIVATE_COMMAND"}]}]
                }
            }"#,
        )
        .unwrap();
        let status = inspect_claude_hook(&config).unwrap();
        assert_eq!(status, HookReadiness::NotInstalled);
        let encoded = serde_json::to_string(&status).unwrap();
        assert!(!encoded.contains("PRIVATE_SECRET"));
        assert!(!encoded.contains("PRIVATE_COMMAND"));
    }

    #[test]
    fn readiness_recognizes_the_generated_snippet_and_invalid_structure() {
        let temp = tempdir().unwrap();
        let config = temp.path().join("settings.json");
        assert_eq!(
            inspect_claude_hook(&config).unwrap(),
            HookReadiness::NotInstalled
        );

        fs::write(&config, CLAUDE_CONFIG_SNIPPET).unwrap();
        assert_eq!(
            inspect_claude_hook(&config).unwrap(),
            HookReadiness::Installed
        );

        fs::write(&config, r#"{"hooks":"PRIVATE_INVALID_STRUCTURE"}"#).unwrap();
        assert_eq!(
            inspect_claude_hook(&config).unwrap(),
            HookReadiness::Conflicting
        );
    }
}
