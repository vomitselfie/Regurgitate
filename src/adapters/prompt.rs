//! Shared, bounded UserPromptSubmit contract supported by Codex and Claude.
use crate::application::ProjectLocator;
use anyhow::{Context, Result, bail};
use serde::Deserialize;
use std::{io::Read, path::PathBuf};
use zeroize::Zeroizing;

const MAX_INPUT_BYTES: u64 = 64 * 1024;

#[derive(Deserialize)]
struct Input {
    cwd: PathBuf,
    hook_event_name: String,
    prompt: Option<String>,
}

pub struct PreflightRequest {
    pub project: ProjectLocator,
    pub prompt: Zeroizing<String>,
}

pub fn normalize_prompt_submit(reader: impl Read) -> Result<PreflightRequest> {
    let mut bytes = Zeroizing::new(Vec::new());
    reader.take(MAX_INPUT_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_INPUT_BYTES {
        bail!("prompt hook exceeds input limit");
    }
    let input: Input = serde_json::from_slice(&bytes).context("invalid prompt hook JSON")?;
    if input.hook_event_name != "UserPromptSubmit" || !input.cwd.is_absolute() {
        bail!("expected a UserPromptSubmit event with an absolute working directory");
    }
    Ok(PreflightRequest {
        project: ProjectLocator::new(input.cwd),
        prompt: Zeroizing::new(input.prompt.unwrap_or_default()),
    })
}

pub fn preflight_response(brief: &str) -> Option<serde_json::Value> {
    (!brief.is_empty()).then(|| {
        serde_json::json!({"hookSpecificOutput": {
            "hookEventName":"UserPromptSubmit", "additionalContext":brief
        }})
    })
}
