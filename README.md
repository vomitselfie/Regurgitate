# Praxis

Praxis is a local-first procedural memory layer for AI coding agents. It is
designed to preserve small, structured observations about what agents tried and
whether it worked, without preserving prompts, responses, commands, source
content, tool arguments, or tool output.

The first development slice targets Agent of Empires-managed Codex sessions on
Linux. It provides:

- a strict, controlled normalized event model;
- a Codex `PostToolUse` hook normalizer that drops non-allowlisted input;
- conservative parsing of existing Codex JSONL sessions for reconnaissance and
  migration;
- AoE session discovery without embedding AoE concerns in the core model;
- per-record CBOR + XChaCha20-Poly1305 encrypted SQLite storage;
- HKDF-separated event keys and a Linux Secret Service master-key provider;
- encrypted project identity mappings and incremental ingestion cursors;
- a modular, interruption-safe `ingest` application service and CLI path; and
- privacy regression tests with adversarial fixture content.

This is an early implementation. Manual ingestion is wired for AoE-managed
Codex sessions. Bounded recall and automatic installation are not implemented
yet.

## Manual ingestion

Praxis currently requires Linux Secret Service to be available and unlocked.
It never falls back to a plaintext key or plaintext history database.

```bash
cargo test
cargo run -- debug-hook < tests/fixtures/codex/post-tool-use-success.json
cargo run -- debug-parse --session <aoe-session-id>
cargo run -- ingest --session <aoe-session-id>
```

Both debug commands print only the sanitized event projection. They never print
the hook payload, transcript payload, command arguments, or tool results.
`ingest` prints only aggregate counts and stores encrypted events under
`$XDG_DATA_HOME/praxis/history.db` (or `~/.local/share/praxis/history.db`).
The data directory is owner-only on Unix and the database file is created with
mode `0600`.

See [Architecture](docs/architecture.md) for module boundaries, data flow, and
the current implementation limits. Upstream source-format assumptions are
recorded in [Source format notes](docs/source-format-notes.md).

## Privacy invariant

Only controlled event and cursor types may enter the application service. A
project path crosses in a non-serializable `ProjectLocator` whose only consumer
is the encrypted identity resolver; the path never enters an event, report, or
plaintext database column. Events and private metadata are serialized to CBOR
and encrypted in memory before SQLite receives any payload bytes. The master
key is stored separately through Linux Secret Service; the implementation has
no automatic plaintext-key or plaintext-event fallback.

## Verification

```bash
cargo fmt --all -- --check
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
```
