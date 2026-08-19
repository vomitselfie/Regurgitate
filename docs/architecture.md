# Architecture

Praxis uses a ports-and-adapters layout so host integrations, application
coordination, and storage can evolve independently.

```text
CLI/runtime composition
    ├── AoE discovery + Codex normalization adapter
    ├── ingestion application service
    └── encrypted SQLite adapter + Secret Service key provider

application service ── depends on ──> controlled HistoryEvent model
adapters/storage ────── implement ───> narrow application ports
```

The binary entry point contains no business logic. `runtime` selects concrete
adapters, `application` coordinates them, and `core` contains the controlled
event vocabulary. AoE and Codex details are confined to `adapters`; encryption,
SQLite, and credential-store details are confined to `storage`.

## Ingestion flow

Manual ingestion currently follows this path:

1. Resolve an opaque AoE session identifier in the selected profile.
2. Locate the linked Codex session record.
3. Deserialize only allowlisted structural fields needed to pair tool calls and
   results.
4. Reduce each pair to a `HistoryEvent` containing controlled enums and opaque
   local identifiers.
5. Encrypt the complete event in memory with a fresh XChaCha20-Poly1305 nonce.
6. Insert the authenticated envelope into SQLite using its deterministic event
   UUID as the primary key.
7. Return aggregate observed/inserted/already-present counts only.

Stable event IDs make a repeated or interrupted full-session ingest safe: an
already committed event is ignored on the next run. A persisted source cursor
is still needed to avoid rescanning an ever-growing transcript and to detect
source replacement or rotation.

## Privacy boundary

Raw records are short-lived adapter inputs. Prompts, responses, reasoning,
commands, arguments, tool output, paths, URLs, environment values, and source
contents are neither members of `HistoryEvent` nor accepted by the application
or storage interfaces.

SQLite receives an event UUID, authenticated envelope metadata, a random nonce,
and ciphertext. The event UUID and timestamps are structural metadata; the
session ID, project ID, agent type, operation, outcome, and other event fields
are inside the encrypted payload. The master key is held by Linux Secret
Service and is never stored beside the database.

Debug commands expose only `DebugEvent`, which omits identifiers and timestamps.
The ingestion command exposes only aggregate counts.

## Current boundaries and limits

Implemented:

- controlled event schema and conservative outcome classification;
- Codex `PostToolUse` and transcript normalization;
- AoE-managed Codex session discovery;
- encrypted, idempotent event persistence;
- Linux Secret Service key retrieval/creation;
- manual session ingestion; and
- adversarial privacy, authentication, filesystem-mode, and idempotency tests.

Not yet implemented:

- encrypted path-to-project identity mapping;
- persisted ingestion cursors and rotation detection;
- project-scoped queries and bounded recall;
- automatic AoE lifecycle integration; and
- retention, inspection, and forgetting commands.
