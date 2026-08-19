use std::{
    env, fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::core::AgentKind;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionDescriptor {
    pub session_id: String,
    pub agent_kind: AgentKind,
    pub agent_session_id: String,
    pub project_path: PathBuf,
}

#[derive(Debug, Deserialize)]
struct RawAoeSession {
    id: String,
    project_path: PathBuf,
    tool: String,
    #[serde(default)]
    agent_session_id: Option<String>,
}

pub fn default_config_dir() -> Result<PathBuf> {
    if let Some(path) = env::var_os("XDG_CONFIG_HOME") {
        return Ok(PathBuf::from(path).join("agent-of-empires"));
    }
    let home = env::var_os("HOME").context("HOME is not set")?;
    Ok(PathBuf::from(home).join(".config/agent-of-empires"))
}

pub fn default_codex_home() -> Result<PathBuf> {
    if let Some(path) = env::var_os("CODEX_HOME") {
        return Ok(PathBuf::from(path));
    }
    let home = env::var_os("HOME").context("HOME is not set")?;
    Ok(PathBuf::from(home).join(".codex"))
}

pub fn find_session(
    config_dir: &Path,
    profile: &str,
    session_id: &str,
) -> Result<SessionDescriptor> {
    validate_identifier(profile, "profile")?;
    let sessions_path = config_dir
        .join("profiles")
        .join(profile)
        .join("sessions.json");
    let bytes = fs::read(&sessions_path).with_context(|| {
        format!(
            "could not read AoE session registry at {}",
            sessions_path.display()
        )
    })?;
    let sessions: Vec<RawAoeSession> = serde_json::from_slice(&bytes).with_context(|| {
        format!(
            "invalid AoE session registry at {}",
            sessions_path.display()
        )
    })?;

    let raw = sessions
        .into_iter()
        .find(|session| session.id == session_id)
        .with_context(|| {
            format!("AoE session {session_id:?} was not found in profile {profile:?}")
        })?;

    let agent_kind = match raw.tool.as_str() {
        "codex" => AgentKind::Codex,
        "claude" => AgentKind::Claude,
        _ => AgentKind::Other,
    };

    let agent_session_id = raw
        .agent_session_id
        .filter(|value| !value.is_empty())
        .context("AoE session has no linked agent session id")?;
    validate_identifier(&agent_session_id, "agent session id")?;

    Ok(SessionDescriptor {
        session_id: raw.id,
        agent_kind,
        agent_session_id,
        project_path: raw.project_path,
    })
}

pub fn find_codex_transcript(codex_home: &Path, agent_session_id: &str) -> Result<PathBuf> {
    validate_identifier(agent_session_id, "agent session id")?;
    let suffix = format!("{agent_session_id}.jsonl");
    let sessions_dir = codex_home.join("sessions");
    let mut pending = vec![sessions_dir.clone()];

    while let Some(directory) = pending.pop() {
        let entries = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                return Err(error).with_context(|| {
                    format!(
                        "could not inspect Codex sessions under {}",
                        sessions_dir.display()
                    )
                });
            }
        };

        for entry in entries {
            let entry = entry?;
            let file_type = entry.file_type()?;
            if file_type.is_dir() {
                pending.push(entry.path());
            } else if file_type.is_file() && entry.file_name().to_string_lossy().ends_with(&suffix)
            {
                return Ok(entry.path());
            }
        }
    }

    bail!("no Codex transcript is linked to the selected AoE session")
}

pub(super) fn validate_identifier(value: &str, label: &str) -> Result<()> {
    let valid = !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'));
    if !valid {
        bail!("invalid {label}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn discovers_a_codex_session_without_deserializing_extra_fields() {
        let temp = tempdir().unwrap();
        let profile_dir = temp.path().join("profiles/default");
        fs::create_dir_all(&profile_dir).unwrap();
        fs::write(
            profile_dir.join("sessions.json"),
            br#"[{"id":"aoe-1","project_path":"/private/project","tool":"codex","agent_session_id":"codex-1","command":"SECRET command"}]"#,
        )
        .unwrap();

        let session = find_session(temp.path(), "default", "aoe-1").unwrap();
        assert_eq!(session.agent_kind, AgentKind::Codex);
        assert_eq!(session.agent_session_id, "codex-1");
    }

    #[test]
    fn rejects_profile_traversal() {
        let error = find_session(Path::new("/unused"), "../private", "aoe-1").unwrap_err();
        assert!(error.to_string().contains("invalid profile"));
    }

    #[test]
    fn discovers_provider_kind_without_rejecting_non_codex_sessions() {
        let temp = tempdir().unwrap();
        let profile_dir = temp.path().join("profiles/default");
        fs::create_dir_all(&profile_dir).unwrap();
        fs::write(
            profile_dir.join("sessions.json"),
            br#"[{"id":"aoe-1","project_path":"/private/project","tool":"claude","agent_session_id":"claude-1"}]"#,
        )
        .unwrap();

        let session = find_session(temp.path(), "default", "aoe-1").unwrap();
        assert_eq!(session.agent_kind, AgentKind::Claude);
        assert_eq!(session.agent_session_id, "claude-1");
    }
}
