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
3. Load the session's encrypted cursor and validate its committed byte-prefix
   digest against the current transcript.
4. Deserialize only complete, new JSONL records and only the allowlisted
   structural fields needed to pair tool calls and results.
5. Resolve the canonical local project path through a keyed lookup token and an
   encrypted path-to-UUID record.
6. Reduce each completed pair to a `HistoryEvent` containing controlled enums,
   opaque local identifiers, and the generated project UUID.
7. Encrypt the complete event in memory with a fresh XChaCha20-Poly1305 nonce.
8. Insert the authenticated envelope into SQLite using its deterministic event
   UUID as the primary key.
9. After every event append succeeds, encrypt and save the advanced cursor.
10. Return aggregate observed/inserted/already-present/reset counts only.

An incomplete final JSONL record remains uncommitted. A pending call is stored
in the cursor only as its deterministic UUID, timestamp, agent, capability, and
operation. A changed or truncated committed prefix causes a safe reset and
reparse. If event persistence fails, the cursor does not advance; retrying is
safe because already committed event UUIDs are ignored.

## Privacy boundary

Raw records are short-lived adapter inputs. Prompts, responses, reasoning,
commands, arguments, tool output, URLs, environment values, and source contents
are neither members of `HistoryEvent` nor accepted by the application or event
storage interfaces. A path is carried only by a non-serializable
`ProjectLocator` to the private identity resolver.

SQLite receives an event UUID, authenticated envelope metadata, a random nonce,
and ciphertext. The event UUID and timestamps are structural metadata; the
session ID, project ID, agent type, operation, outcome, and other event fields
are inside the encrypted payload. Project and cursor tables expose only
HMAC-SHA-256 lookup tokens, version numbers, nonces, and ciphertext. Their
paths, session IDs, cursor offsets, digests, and pending state are encrypted.
The master key is held by Linux Secret Service and is never stored beside the
database.

Debug commands expose only `DebugEvent`, which omits identifiers and timestamps.
The ingestion command exposes only aggregate counts.

## Current boundaries and limits

Implemented:

- controlled event schema and conservative outcome classification;
- Codex `PostToolUse` and transcript normalization;
- AoE-managed Codex session discovery;
- encrypted, idempotent event persistence;
- encrypted path-to-project UUID mapping with keyed lookup tokens;
- encrypted incremental cursors with append, incomplete-line, truncation, and
  replacement handling;
- Linux Secret Service key retrieval/creation;
- manual session ingestion; and
- adversarial privacy, authentication, filesystem-mode, and idempotency tests.

Not yet implemented:

- project-scoped queries and bounded recall;
- automatic AoE lifecycle integration; and
- retention, inspection, and forgetting commands.
