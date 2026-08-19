use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::core::{AgentKind, Capability, Operation, Strategy};

pub const CURRENT_CURSOR_VERSION: u32 = 1;

/// Sanitized state for a tool call whose result has not appeared yet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PendingEvent {
    pub event_id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub agent: AgentKind,
    pub capability: Capability,
    pub operation: Operation,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strategy: Option<Strategy>,
}

/// Adapter-neutral checkpoint for an append-oriented session source.
///
/// The digest covers every committed source byte, but it is stored only inside
/// an encrypted cursor record. Pending state contains controlled vocabulary and
/// deterministic UUIDs—never call arguments, output, or raw source fragments.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IngestionCursor {
    pub committed_offset: u64,
    pub source_length: u64,
    pub committed_digest: [u8; 32],
    pub pending_events: Vec<PendingEvent>,
    pub schema_version: u32,
}

impl IngestionCursor {
    pub fn empty() -> Self {
        Self {
            committed_offset: 0,
            source_length: 0,
            committed_digest: empty_digest(),
            pending_events: Vec::new(),
            schema_version: CURRENT_CURSOR_VERSION,
        }
    }
}

fn empty_digest() -> [u8; 32] {
    use sha2::{Digest, Sha256};

    Sha256::digest([]).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_cursor_uses_the_current_schema_and_sha256_empty_digest() {
        let cursor = IngestionCursor::empty();
        assert_eq!(cursor.schema_version, CURRENT_CURSOR_VERSION);
        assert_eq!(
            cursor.committed_digest,
            [
                0xe3, 0xb0, 0xc4, 0x42, 0x98, 0xfc, 0x1c, 0x14, 0x9a, 0xfb, 0xf4, 0xc8, 0x99, 0x6f,
                0xb9, 0x24, 0x27, 0xae, 0x41, 0xe4, 0x64, 0x9b, 0x93, 0x4c, 0xa4, 0x95, 0x99, 0x1b,
                0x78, 0x52, 0xb8, 0x55,
            ]
        );
    }

    #[test]
    fn pending_events_without_strategy_remain_compatible() {
        let pending: PendingEvent = serde_json::from_value(serde_json::json!({
            "event_id": Uuid::nil(),
            "timestamp": "2026-08-19T12:00:00Z",
            "agent": "codex",
            "capability": "shell",
            "operation": "command"
        }))
        .unwrap();

        assert_eq!(pending.strategy, None);
    }
}
