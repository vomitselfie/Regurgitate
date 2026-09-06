use std::path::Path;

use super::quote_agent_executable;

/// Recognize only our stock commands, including the exact shell quoting emitted
/// by our installers. Never evaluate a command or accept wrappers/extra flags.
pub(super) fn same_stock_hook(command: &str, expected: &str) -> bool {
    stock_action(command).is_some_and(|action| Some(action) == stock_action(expected))
}

fn stock_action(command: &str) -> Option<&'static str> {
    for action in [
        " record-hook --agent codex",
        " record-hook --agent claude",
        " preflight --agent claude",
    ] {
        let Some(executable) = command.strip_suffix(action) else {
            continue;
        };
        if executable == "regurgitate" {
            return Some(action);
        }
        let Some(quoted) = executable
            .strip_prefix('\'')
            .and_then(|s| s.strip_suffix('\''))
        else {
            continue;
        };
        let decoded = quoted.replace("'\"'\"'", "'");
        let path = Path::new(&decoded);
        if path.is_absolute()
            && path.file_name().is_some_and(|name| name == "regurgitate")
            && quote_agent_executable(path).is_ok_and(|quoted| quoted == executable)
        {
            return Some(action);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_only_exact_stock_commands() {
        let expected = "regurgitate record-hook --agent codex";
        for path in ["/tools/regurgitate", "/user's tools/regurgitate"] {
            let command = format!(
                "{} record-hook --agent codex",
                quote_agent_executable(Path::new(path)).unwrap()
            );
            assert!(same_stock_hook(&command, expected));
        }
        for command in [
            "sh -c 'regurgitate record-hook --agent codex'",
            "'/tools/other' record-hook --agent codex",
            "'relative/regurgitate' record-hook --agent codex",
            "regurgitate record-hook --agent codex --custom",
            "regurgitate record-hook --agent claude",
            "regurgitate preflight --agent claude",
            "'/tools/regurgitate'; echo 'x' record-hook --agent codex",
        ] {
            assert!(!same_stock_hook(command, expected), "{command}");
        }
    }
}
