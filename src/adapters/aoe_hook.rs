use std::{env, ffi::OsString};

use anyhow::{Context, Result};

use crate::core::AgentKind;

use super::aoe::validate_identifier;

/// Identifier-only input supplied by an AoE status hook. Project paths,
/// titles, branches, group names, and status timestamps are not read.
pub struct AoeHookContext {
    pub session_id: String,
    pub profile: String,
    pub agent: AgentKind,
}

pub fn current_context() -> Result<AoeHookContext> {
    context_from_values(
        env::var_os("AOE_SESSION_ID"),
        env::var_os("AOE_PROFILE"),
        env::var_os("AOE_TOOL"),
    )
}

fn context_from_values(
    session_id: Option<OsString>,
    profile: Option<OsString>,
    tool: Option<OsString>,
) -> Result<AoeHookContext> {
    let session_id = unicode_value(session_id, "AOE_SESSION_ID")?;
    let profile = unicode_value(profile, "AOE_PROFILE")?;
    let tool = unicode_value(tool, "AOE_TOOL")?;
    validate_identifier(&session_id, "AoE hook session id")?;
    validate_identifier(&profile, "AoE hook profile")?;

    Ok(AoeHookContext {
        session_id,
        profile,
        agent: match tool.as_str() {
            "codex" => AgentKind::Codex,
            "claude" => AgentKind::Claude,
            _ => AgentKind::Other,
        },
    })
}

fn unicode_value(value: Option<OsString>, name: &str) -> Result<String> {
    value
        .with_context(|| format!("AoE hook did not provide {name}"))?
        .into_string()
        .map_err(|_| anyhow::anyhow!("AoE hook provided non-Unicode {name}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_only_the_identifier_fields_needed_for_ingestion() {
        let context = context_from_values(
            Some("session-1".into()),
            Some("default".into()),
            Some("codex".into()),
        )
        .unwrap();
        assert_eq!(context.agent, AgentKind::Codex);
        assert_eq!(context.session_id, "session-1");
    }

    #[test]
    fn rejects_missing_or_traversing_identifiers() {
        assert!(context_from_values(None, Some("default".into()), Some("codex".into())).is_err());
        assert!(
            context_from_values(
                Some("session-1".into()),
                Some("../other".into()),
                Some("codex".into())
            )
            .is_err()
        );
    }

    #[test]
    fn unsupported_agents_are_safe_to_ignore() {
        let context = context_from_values(
            Some("session-1".into()),
            Some("default".into()),
            Some("hermes".into()),
        )
        .unwrap();
        assert_eq!(context.agent, AgentKind::Other);
    }

    #[test]
    fn identifies_claude_without_treating_it_as_codex() {
        let context = context_from_values(
            Some("session-1".into()),
            Some("default".into()),
            Some("claude".into()),
        )
        .unwrap();
        assert_eq!(context.agent, AgentKind::Claude);
    }
}
