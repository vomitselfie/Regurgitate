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

#[cfg(test)]
mod tests {
    use serde_json::Value;

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
}
