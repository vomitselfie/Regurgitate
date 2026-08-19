# Praxis

Praxis is a local-first, provider-neutral procedural memory layer for AI coding
agents. It is designed to preserve small, structured observations about what
agents tried and whether it worked, without preserving prompts, responses,
commands, source content, tool arguments, or tool output.

The core storage, query, and application boundaries are agent-agnostic. The
first ingestion adapter targets Agent of Empires-managed Codex sessions on
Linux. The current implementation provides:

- a strict, controlled normalized event model;
- a Codex `PostToolUse` hook normalizer that drops non-allowlisted input;
- conservative parsing of existing Codex JSONL sessions for reconnaissance and
  migration;
- AoE session discovery without embedding AoE concerns in the core model;
- per-record CBOR + XChaCha20-Poly1305 encrypted SQLite storage;
- HKDF-separated event keys and a Linux Secret Service master-key provider;
- encrypted project identity mappings and incremental ingestion cursors;
- a modular, interruption-safe `ingest` application service and CLI path;
- project-scoped, task-ranked aggregate recall with hard count and token
  budgets;
- a small provider-neutral agent skill that uses only the recall CLI; and
- privacy regression tests with adversarial fixture content.

This is an early implementation. Manual and AoE status-hook ingestion are wired
for Codex sessions, and task-specific bounded recall is available to any agent
that can invoke the CLI. Automatic installation is not implemented yet.

## CLI usage

Praxis currently requires Linux Secret Service to be available and unlocked.
It never falls back to a plaintext key or plaintext history database.

```bash
cargo test
cargo run -- debug-hook < tests/fixtures/codex/post-tool-use-success.json
cargo run -- debug-parse --session <aoe-session-id>
cargo run -- ingest --session <aoe-session-id>
cargo run -- recall --project "$PWD"
cargo run -- recall --project "$PWD" --query "fix failing tests" --token-budget 600
cargo run -- print-aoe-config
```

Both debug commands print only the sanitized event projection. They never print
the hook payload, transcript payload, command arguments, or tool results.
`ingest` prints only aggregate counts and stores encrypted events under
`$XDG_DATA_HOME/praxis/history.db` (or `~/.local/share/praxis/history.db`).
The data directory is owner-only on Unix and the database file is created with
mode `0600`.

`recall` returns at most 20 fixed-schema aggregate observations and defaults to
an approximate 600-token serialized-output budget. It supports a controlled
`--operation` filter, `--failures`, and ephemeral task-query ranking. It never
returns event, session, or project identifiers, timestamps, paths, query text,
or historical content. Query text is not persisted by Praxis, though text
provided on a command line may still be retained by the user's shell history.

For automatic recording, install the release binary somewhere available in the
AoE host process's `PATH`, run `praxis print-aoe-config`, and manually merge the
snippet into the desired global or profile AoE config. It invokes
`praxis aoe-hook` on stable idle/error transitions. The handler reads only
`AOE_SESSION_ID`, `AOE_PROFILE`, and `AOE_TOOL`; duplicate deliveries are safe.
Unsupported agent types are ignored successfully.

## Agent-facing recall

The tracked [`praxis-recall`](skills/praxis-recall/SKILL.md) skill tells a fresh
agent to wait until the task is known, request one bounded task-specific
aggregate, and validate any remembered pattern against current state. Its
workflow is provider-neutral and calls only `praxis recall`. The optional
`agents/openai.yaml` file contains Codex discovery metadata; other agent hosts
can ignore it and register the same `SKILL.md` through their own skill-loading
mechanism.

Install or link the skill directory through the selected agent host. Praxis
does not modify agent configuration automatically.

See [Architecture](docs/architecture.md) for module boundaries, data flow, and
the current implementation limits, and [Roadmap](docs/roadmap.md) for the next
integration slices. Upstream source-format assumptions are recorded in
[Source format notes](docs/source-format-notes.md).

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
