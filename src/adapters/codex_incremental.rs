use std::io::{Read, Seek, SeekFrom};

use anyhow::{Context, Result, anyhow};
use serde_json::from_slice;
use sha2::{Digest, Sha256};

use crate::{
    application::{CURRENT_CURSOR_VERSION, IngestionCursor, PendingEvent},
    core::{
        AgentKind, CURRENT_SCHEMA_VERSION, HistoryEvent, Outcome, classify_tool,
        classify_tool_response,
    },
};

use super::codex::{TranscriptRecord, stable_event_id};

pub struct IncrementalEvents {
    pub events: Vec<HistoryEvent>,
    pub next_cursor: IngestionCursor,
    pub source_reset: bool,
}

/// Parse only complete JSONL records after the last authenticated checkpoint.
/// A changed or truncated committed prefix discards the old cursor and safely
/// reparses the replacement source from byte zero.
pub fn normalize_transcript_since<R: Read + Seek>(
    mut reader: R,
    session_id: &str,
    cursor: Option<&IngestionCursor>,
) -> Result<IncrementalEvents> {
    let initial_length = reader
        .seek(SeekFrom::End(0))
        .context("could not inspect Codex transcript length")?;
    let (base, source_reset) = validated_cursor(&mut reader, initial_length, cursor)?;

    reader
        .seek(SeekFrom::Start(base.committed_offset))
        .context("could not seek to the Codex transcript checkpoint")?;
    let mut tail = Vec::new();
    reader
        .read_to_end(&mut tail)
        .context("could not read new Codex transcript records")?;
    let observed_length = base
        .committed_offset
        .checked_add(u64::try_from(tail.len()).context("Codex transcript is too large")?)
        .context("Codex transcript length overflow")?;
    let complete_len = tail
        .iter()
        .rposition(|byte| *byte == b'\n')
        .map_or(0, |index| index + 1);

    let mut pending = base
        .pending_events
        .into_iter()
        .map(|event| (event.event_id, event))
        .collect::<std::collections::HashMap<_, _>>();
    let mut events = Vec::new();

    for (index, line) in tail[..complete_len]
        .split(|byte| *byte == b'\n')
        .enumerate()
    {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        if line.is_empty() {
            continue;
        }
        let record: TranscriptRecord = from_slice(line).with_context(|| {
            format!(
                "invalid new transcript record at relative line {}",
                index + 1
            )
        })?;
        if record.record_type != "response_item" {
            continue;
        }

        match record.payload.payload_type.as_deref() {
            Some("function_call" | "custom_tool_call") => {
                let (Some(call_id), Some(tool_name)) =
                    (record.payload.call_id, record.payload.name)
                else {
                    continue;
                };
                let event_id = stable_event_id(session_id, &call_id);
                let (capability, operation) = classify_tool(&tool_name);
                pending.insert(
                    event_id,
                    PendingEvent {
                        event_id,
                        timestamp: record.timestamp.unwrap_or_else(chrono::Utc::now),
                        agent: AgentKind::Codex,
                        capability,
                        operation,
                    },
                );
            }
            Some("function_call_output" | "custom_tool_call_output") => {
                let Some(call_id) = record.payload.call_id else {
                    continue;
                };
                let event_id = stable_event_id(session_id, &call_id);
                let Some(call) = pending.remove(&event_id) else {
                    continue;
                };
                let (outcome, error_class) = record
                    .payload
                    .output
                    .as_ref()
                    .map(classify_tool_response)
                    .unwrap_or((Outcome::Unknown, None));
                events.push(HistoryEvent {
                    id: call.event_id,
                    timestamp: call.timestamp,
                    session_id: Some(session_id.to_owned()),
                    project_id: None,
                    agent: Some(call.agent),
                    capability: call.capability,
                    operation: call.operation,
                    strategy: None,
                    outcome,
                    duration_ms: None,
                    error_class,
                    schema_version: CURRENT_SCHEMA_VERSION,
                });
            }
            _ => {}
        }
    }

    let committed_offset = base
        .committed_offset
        .checked_add(u64::try_from(complete_len).context("Codex transcript is too large")?)
        .context("Codex transcript offset overflow")?;
    let committed_digest = digest_prefix(&mut reader, committed_offset)?;
    let mut pending_events: Vec<_> = pending.into_values().collect();
    pending_events.sort_by_key(|event| event.event_id);
    events.sort_by_key(|event| event.timestamp);

    Ok(IncrementalEvents {
        events,
        next_cursor: IngestionCursor {
            committed_offset,
            source_length: observed_length,
            committed_digest,
            pending_events,
            schema_version: CURRENT_CURSOR_VERSION,
        },
        source_reset,
    })
}

fn validated_cursor<R: Read + Seek>(
    reader: &mut R,
    source_length: u64,
    cursor: Option<&IngestionCursor>,
) -> Result<(IngestionCursor, bool)> {
    let Some(cursor) = cursor else {
        return Ok((IngestionCursor::empty(), false));
    };
    let structurally_valid = cursor.schema_version == CURRENT_CURSOR_VERSION
        && cursor.committed_offset <= source_length
        && cursor.source_length <= source_length;
    if structurally_valid
        && digest_prefix(reader, cursor.committed_offset)? == cursor.committed_digest
    {
        return Ok((cursor.clone(), false));
    }
    Ok((IngestionCursor::empty(), true))
}

fn digest_prefix<R: Read + Seek>(reader: &mut R, length: u64) -> Result<[u8; 32]> {
    reader
        .seek(SeekFrom::Start(0))
        .context("could not verify the Codex transcript checkpoint")?;
    let mut hasher = Sha256::new();
    let mut remaining = length;
    let mut buffer = [0_u8; 8192];
    while remaining > 0 {
        let requested = usize::try_from(remaining.min(buffer.len() as u64))
            .expect("bounded read length fits usize");
        let read = reader
            .read(&mut buffer[..requested])
            .context("could not hash the Codex transcript checkpoint")?;
        if read == 0 {
            return Err(anyhow!("Codex transcript ended before its checkpoint"));
        }
        hasher.update(&buffer[..read]);
        remaining -= u64::try_from(read).expect("read length fits u64");
    }
    Ok(hasher.finalize().into())
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use crate::core::Outcome;

    use super::*;

    const CALL: &str = "{\"timestamp\":\"2026-08-19T12:00:00Z\",\"type\":\"response_item\",\"payload\":{\"type\":\"function_call\",\"name\":\"exec_command\",\"call_id\":\"call-1\",\"arguments\":\"SECRET_ARGUMENT\"}}\n";
    const OUTPUT: &str = "{\"timestamp\":\"2026-08-19T12:00:01Z\",\"type\":\"response_item\",\"payload\":{\"type\":\"function_call_output\",\"call_id\":\"call-1\",\"output\":{\"exit_code\":0,\"output\":\"SECRET_OUTPUT\"}}}\n";

    #[test]
    fn carries_sanitized_pending_calls_across_incremental_reads() {
        let first = normalize_transcript_since(Cursor::new(CALL), "session-1", None).unwrap();
        assert!(first.events.is_empty());
        assert_eq!(first.next_cursor.pending_events.len(), 1);

        let source = format!("{CALL}{OUTPUT}");
        let second = normalize_transcript_since(
            Cursor::new(source.as_bytes()),
            "session-1",
            Some(&first.next_cursor),
        )
        .unwrap();
        assert_eq!(second.events.len(), 1);
        assert_eq!(second.events[0].outcome, Outcome::Success);
        assert!(second.next_cursor.pending_events.is_empty());
        assert!(!second.source_reset);

        let encoded = serde_json::to_string(&second.next_cursor).unwrap();
        assert!(!encoded.contains("SECRET_ARGUMENT"));
        assert!(!encoded.contains("SECRET_OUTPUT"));
    }

    #[test]
    fn leaves_an_incomplete_last_record_uncommitted() {
        let partial = CALL.trim_end_matches('\n');
        let first = normalize_transcript_since(Cursor::new(partial), "session-1", None).unwrap();
        assert_eq!(first.next_cursor.committed_offset, 0);
        assert!(first.next_cursor.pending_events.is_empty());

        let complete = format!("{partial}\n{OUTPUT}");
        let second = normalize_transcript_since(
            Cursor::new(complete.as_bytes()),
            "session-1",
            Some(&first.next_cursor),
        )
        .unwrap();
        assert_eq!(second.events.len(), 1);
    }

    #[test]
    fn resets_after_committed_source_replacement() {
        let first_source = format!("{CALL}{OUTPUT}");
        let first =
            normalize_transcript_since(Cursor::new(first_source.as_bytes()), "session-1", None)
                .unwrap();

        let replacement = format!(
            "{}{}",
            CALL.replace("call-1", "call-2"),
            OUTPUT.replace("call-1", "call-2")
        );
        let second = normalize_transcript_since(
            Cursor::new(replacement.as_bytes()),
            "session-1",
            Some(&first.next_cursor),
        )
        .unwrap();
        assert!(second.source_reset);
        assert_eq!(second.events.len(), 1);
        assert_ne!(second.events[0].id, first.events[0].id);
    }
}
