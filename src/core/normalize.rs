use serde_json::Value;

use super::{Capability, ErrorClass, Operation, Outcome, Strategy};

pub fn classify_tool(tool_name: &str) -> (Capability, Operation) {
    match tool_name {
        "Bash" | "exec" | "exec_command" => (Capability::Shell, Operation::Command),
        "write_stdin" => (Capability::Shell, Operation::ContinueCommand),
        "apply_patch" | "Edit" | "Write" | "NotebookEdit" => {
            (Capability::Patch, Operation::ApplyPatch)
        }
        "Read" | "read_file" | "read_mcp_resource" => (Capability::Filesystem, Operation::ReadFile),
        "write_file" => (Capability::Filesystem, Operation::WriteFile),
        "Glob" | "Grep" | "web__run" | "WebSearch" | "search" => {
            (Capability::Search, Operation::Search)
        }
        "WebFetch" => (Capability::Network, Operation::WebRequest),
        "view_image" => (Capability::Vision, Operation::InspectImage),
        "TodoWrite" | "update_plan" => (Capability::Other, Operation::UpdatePlan),
        "spawn_agent" | "Agent" => (Capability::Other, Operation::Delegate),
        "wait" | "wait_agent" => (Capability::Wait, Operation::Wait),
        name if name.starts_with("mcp__") => (Capability::Other, Operation::ToolCall),
        _ => (Capability::Other, Operation::ToolCall),
    }
}

/// Derive a controlled strategy only when the provider's tool identity is
/// sufficient. Never inspect arguments, commands, paths, or output text.
pub fn classify_strategy(tool_name: &str) -> Option<Strategy> {
    match tool_name {
        "apply_patch" => Some(Strategy::StructuredPatch),
        "Edit" | "Write" | "NotebookEdit" | "write_file" => Some(Strategy::DirectTextMutation),
        _ => None,
    }
}

/// Derive only low-cardinality outcome signals from a transient tool response.
/// No response text or arbitrary label is returned.
pub fn classify_tool_response(response: &Value) -> (Outcome, Option<ErrorClass>) {
    if let Some(exit_code) = find_integer_field(response, &["exit_code", "exitCode"], 0) {
        return from_exit_code(exit_code);
    }

    if let Some(is_error) = find_bool_field(response, &["is_error", "isError"], 0) {
        return if is_error {
            (Outcome::Failure, Some(ErrorClass::Unknown))
        } else {
            (Outcome::Success, None)
        };
    }

    if let Some(success) = find_bool_field(response, &["success", "ok"], 0) {
        return if success {
            (Outcome::Success, None)
        } else {
            (Outcome::Failure, Some(ErrorClass::Unknown))
        };
    }

    if let Some(status) = find_string_field(response, "status", 0) {
        match status {
            "success" | "succeeded" => return (Outcome::Success, None),
            "failure" | "failed" | "error" => {
                return (Outcome::Failure, Some(ErrorClass::Unknown));
            }
            _ => {}
        }
    }

    if let Some(exit_code) = find_process_exit_marker(response) {
        return from_exit_code(exit_code);
    }

    (Outcome::Unknown, None)
}

fn from_exit_code(exit_code: i64) -> (Outcome, Option<ErrorClass>) {
    if exit_code == 0 {
        (Outcome::Success, None)
    } else {
        (Outcome::Failure, Some(ErrorClass::NonzeroExit))
    }
}

fn find_integer_field(value: &Value, keys: &[&str], depth: usize) -> Option<i64> {
    if depth > 4 {
        return None;
    }

    match value {
        Value::Object(map) => {
            for key in keys {
                if let Some(value) = map.get(*key).and_then(Value::as_i64) {
                    return Some(value);
                }
            }
            map.values()
                .find_map(|value| find_integer_field(value, keys, depth + 1))
        }
        Value::Array(values) => values
            .iter()
            .find_map(|value| find_integer_field(value, keys, depth + 1)),
        _ => None,
    }
}

fn find_bool_field(value: &Value, keys: &[&str], depth: usize) -> Option<bool> {
    if depth > 4 {
        return None;
    }

    match value {
        Value::Object(map) => {
            for key in keys {
                if let Some(value) = map.get(*key).and_then(Value::as_bool) {
                    return Some(value);
                }
            }
            map.values()
                .find_map(|value| find_bool_field(value, keys, depth + 1))
        }
        Value::Array(values) => values
            .iter()
            .find_map(|value| find_bool_field(value, keys, depth + 1)),
        _ => None,
    }
}

fn find_string_field<'a>(value: &'a Value, key: &str, depth: usize) -> Option<&'a str> {
    if depth > 4 {
        return None;
    }

    match value {
        Value::Object(map) => map.get(key).and_then(Value::as_str).or_else(|| {
            map.values()
                .find_map(|value| find_string_field(value, key, depth + 1))
        }),
        Value::Array(values) => values
            .iter()
            .find_map(|value| find_string_field(value, key, depth + 1)),
        _ => None,
    }
}

fn find_process_exit_marker(value: &Value) -> Option<i64> {
    value.as_str().and_then(parse_process_exit_marker)
}

fn parse_process_exit_marker(text: &str) -> Option<i64> {
    const MARKER: &str = "Process exited with code ";
    let mut lines = text.lines();
    if !lines.next()?.starts_with("Chunk ID:") {
        return None;
    }

    for line in lines.take(8) {
        if line == "Final output:" {
            return None;
        }
        if let Some(tail) = line.strip_prefix(MARKER) {
            return tail.trim().parse().ok();
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn classifies_structured_exit_codes() {
        assert_eq!(
            classify_tool_response(&json!({"metadata": {"exit_code": 0}})),
            (Outcome::Success, None)
        );
        assert_eq!(
            classify_tool_response(&json!({"exitCode": 2})),
            (Outcome::Failure, Some(ErrorClass::NonzeroExit))
        );
    }

    #[test]
    fn classifies_legacy_codex_exit_marker() {
        let response =
            json!("Chunk ID: secret\nProcess exited with code 1\nFinal output:\nprivate");
        assert_eq!(
            classify_tool_response(&response),
            (Outcome::Failure, Some(ErrorClass::NonzeroExit))
        );
    }

    #[test]
    fn stays_unknown_without_an_explicit_signal() {
        assert_eq!(
            classify_tool_response(&json!({"text": "looks good"})),
            (Outcome::Unknown, None)
        );
    }

    #[test]
    fn classifies_claude_native_tools_without_provider_types() {
        assert_eq!(
            classify_tool("Read"),
            (Capability::Filesystem, Operation::ReadFile)
        );
        assert_eq!(
            classify_tool("Grep"),
            (Capability::Search, Operation::Search)
        );
        assert_eq!(
            classify_tool("WebFetch"),
            (Capability::Network, Operation::WebRequest)
        );
    }

    #[test]
    fn derives_strategy_only_from_unambiguous_tool_identity() {
        assert_eq!(
            classify_strategy("apply_patch"),
            Some(Strategy::StructuredPatch)
        );
        assert_eq!(
            classify_strategy("Write"),
            Some(Strategy::DirectTextMutation)
        );
        assert_eq!(classify_strategy("Bash"), None);
        assert_eq!(classify_strategy("mcp__private__tool"), None);
    }

    #[test]
    fn does_not_treat_arbitrary_output_as_wrapper_metadata() {
        let response = json!([{
            "type": "text",
            "text": "Script completed\nFinal output:\nProcess exited with code 1"
        }]);
        assert_eq!(classify_tool_response(&response), (Outcome::Unknown, None));
    }
}
