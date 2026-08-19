use std::{fs::File, path::PathBuf};

use anyhow::{Context, Result};

use crate::application::{EventBatch, IngestionCursor, ProjectLocator, SessionEventSource};

use super::{aoe, codex_incremental};

/// Edge adapter that links AoE's session registry to Codex's transcript
/// adapter. AoE and Codex parsing remain in their own modules.
pub struct ManagedCodexSource {
    aoe_config_dir: PathBuf,
    profile: String,
    codex_home: PathBuf,
}

impl ManagedCodexSource {
    pub fn new(aoe_config_dir: PathBuf, profile: impl Into<String>, codex_home: PathBuf) -> Self {
        Self {
            aoe_config_dir,
            profile: profile.into(),
            codex_home,
        }
    }
}

impl SessionEventSource for ManagedCodexSource {
    fn events_for_session(
        &self,
        session_id: &str,
        cursor: Option<&IngestionCursor>,
    ) -> Result<EventBatch> {
        let descriptor = aoe::find_session(&self.aoe_config_dir, &self.profile, session_id)?;
        let transcript_path =
            aoe::find_codex_transcript(&self.codex_home, &descriptor.agent_session_id)?;
        let transcript = File::open(&transcript_path)
            .with_context(|| "could not open the linked Codex transcript")?;
        let normalized = codex_incremental::normalize_transcript_since(
            transcript,
            &descriptor.session_id,
            cursor,
        )?;
        Ok(EventBatch {
            events: normalized.events,
            next_cursor: normalized.next_cursor,
            project: ProjectLocator::new(descriptor.project_path),
            source_reset: normalized.source_reset,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn composes_aoe_discovery_and_codex_normalization_without_raw_content() {
        let temp = tempdir().unwrap();
        let config_dir = temp.path().join("aoe");
        let profile_dir = config_dir.join("profiles/default");
        fs::create_dir_all(&profile_dir).unwrap();
        fs::write(
            profile_dir.join("sessions.json"),
            br#"[{"id":"aoe-1","project_path":"/private/SECRET_PROJECT","tool":"codex","agent_session_id":"codex-1","command":"SECRET_COMMAND"}]"#,
        )
        .unwrap();

        let codex_home = temp.path().join("codex");
        let transcript_dir = codex_home.join("sessions/2026/08/19");
        fs::create_dir_all(&transcript_dir).unwrap();
        fs::write(
            transcript_dir.join("rollout-codex-1.jsonl"),
            concat!(
                "{\"timestamp\":\"2026-08-19T12:00:00Z\",\"type\":\"response_item\",\"payload\":{\"type\":\"function_call\",\"name\":\"exec_command\",\"call_id\":\"call-1\",\"arguments\":\"SECRET_ARGUMENT\"}}\n",
                "{\"timestamp\":\"2026-08-19T12:00:01Z\",\"type\":\"response_item\",\"payload\":{\"type\":\"function_call_output\",\"call_id\":\"call-1\",\"output\":{\"exit_code\":0,\"output\":\"SECRET_OUTPUT\"}}}\n"
            ),
        )
        .unwrap();

        let source = ManagedCodexSource::new(config_dir, "default", codex_home);
        let batch = source.events_for_session("aoe-1", None).unwrap();

        assert_eq!(batch.events.len(), 1);
        let encoded = serde_json::to_string(&batch.events).unwrap();
        for forbidden in [
            "SECRET_PROJECT",
            "SECRET_COMMAND",
            "SECRET_ARGUMENT",
            "SECRET_OUTPUT",
        ] {
            assert!(!encoded.contains(forbidden), "leaked {forbidden:?}");
        }
    }
}
