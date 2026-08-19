# Praxis

Praxis is a local-first, provider-neutral procedural memory layer for AI coding
agents. It is designed to preserve small, structured observations about what
agents tried and whether it worked, without preserving prompts, responses,
commands, source content, tool arguments, or tool output.

The core storage, query, and application boundaries are agent-agnostic. Native
hook adapters currently cover Codex and Claude Code, while transcript ingestion
targets Agent of Empires-managed Codex sessions on Linux. The current
implementation provides:

- a strict, controlled normalized event model;
- Codex and Claude Code hook normalizers that drop non-allowlisted input;
- conservative parsing of existing Codex JSONL sessions for reconnaissance and
  migration;
- AoE session discovery without embedding AoE concerns in the core model;
- per-record CBOR + XChaCha20-Poly1305 encrypted SQLite storage;
- HKDF-separated event keys and a Linux Secret Service master-key provider;
- encrypted project identity mappings and incremental ingestion cursors;
- separate modular services for cursor-based transcript ingestion and
  cursor-free native hook recording;
- project-scoped, task-ranked aggregate recall with hard count and token
  budgets;
- non-mutating key-store and history-database health reporting;
- preview-first, project-scoped encrypted history forgetting;
- preview-first global age/count retention in bounded transactions;
- a small provider-neutral agent skill that uses only the recall CLI; and
- privacy regression tests with adversarial fixture content.

This is an early implementation. Codex and Claude Code can record tool
completions directly through native hooks, while AoE status hooks provide a
Codex transcript fallback. Task-specific bounded recall is available to any
agent that can invoke the CLI. Automatic host-path discovery is not
implemented.

## Installation

Praxis currently publishes a static Linux x86-64 musl binary because Linux
Secret Service is a runtime requirement. Download and verify the latest release
with the GitHub CLI, then install it somewhere on `PATH`:

```bash
release="$(gh release view --repo vomitselfie/aoe-praxis --json tagName --jq .tagName)"
version="${release#v}"
archive="praxis-${release}-x86_64-unknown-linux-musl.tar.gz"
gh release download "$release" --repo vomitselfie/aoe-praxis \
  --pattern "$archive" --pattern SHA256SUMS
sha256sum --check --ignore-missing SHA256SUMS
tar -xzf "$archive"
install -Dm755 "praxis-${release}-x86_64-unknown-linux-musl/praxis" \
  "$HOME/.local/bin/praxis"
praxis --version
```

The archive also contains this README and the MIT license. The `install-skill`
command installs the provider-neutral recall skill embedded in the binary, so
the separate source tree is not required at runtime.

## CLI usage

Praxis currently requires Linux Secret Service to be available and unlocked.
It never falls back to a plaintext key or plaintext history database.

```bash
cargo test
cargo run -- debug-hook < tests/fixtures/codex/post-tool-use-success.json
cargo run -- debug-hook --agent claude < tests/fixtures/claude/post-tool-use-success.json
cargo run -- record-hook --agent codex < tests/fixtures/codex/post-tool-use-success.json
cargo run -- record-hook --agent claude < tests/fixtures/claude/post-tool-use-success.json
cargo run -- debug-parse --session <aoe-session-id>
cargo run -- ingest --session <aoe-session-id>
cargo run -- recall --project "$PWD"
cargo run -- recall --project "$PWD" --query "fix failing tests" --token-budget 600
cargo run -- status
cargo run -- status --aoe-config /path/to/aoe/config.toml --codex-config /path/to/codex/config.toml --claude-config /path/to/claude/settings.json
cargo run -- forget --project "$PWD"
cargo run -- forget --project "$PWD" --apply
cargo run -- prune --older-than-days 90
cargo run -- prune --older-than-days 90 --apply
cargo run -- prune --keep-recent 10000
cargo run -- prune --keep-recent 10000 --apply
cargo run -- print-aoe-config
cargo run -- print-codex-config
cargo run -- print-claude-config
cargo run -- install-aoe-hook --config /path/to/aoe/config.toml
cargo run -- install-aoe-hook --config /path/to/aoe/config.toml --apply
cargo run -- install-codex-hook --config /path/to/codex/config.toml
cargo run -- install-codex-hook --config /path/to/codex/config.toml --apply
cargo run -- install-skill --target /path/to/agent/skills
cargo run -- install-skill --target /path/to/agent/skills --apply
```

The debug commands print only the sanitized event projection. They never print
the hook payload, transcript payload, command arguments, or tool results.
`record-hook` intentionally writes nothing to stdout on success so provider
hook protocols cannot interpret a report as control output. `ingest` prints
only aggregate counts. Both recording paths store encrypted events under
`$XDG_DATA_HOME/praxis/history.db` (or `~/.local/share/praxis/history.db`).
The data directory is owner-only on Unix and the database file is created with
mode `0600`.

`status` is read-only. It reports `ready`, `not_configured`, or `degraded` for
the overall installation and controlled readiness values for the key store and
history database. The only data measurement it exposes is the aggregate event
count. It does not create a missing key or database, migrate or repair an
existing database, return paths or identifiers, or surface raw backend errors.
Optional `--aoe-config`, `--codex-config`, and `--claude-config` arguments add
non-mutating hook checks for those exact files. Their output is limited to the
provider name and `installed`, `not_installed`, `conflicting`, or `unavailable`;
Praxis does not guess host paths or return config paths and command strings.
When both the AoE fallback and native Codex hook are found, the two are reported
as `conflicting` because current source event IDs cannot be safely joined.

`forget --project` previews only an aggregate event count. It opens existing
history read-only and does not initialize missing state. Repeating with
`--apply` transactionally deletes that project's encrypted events and encrypted
path mapping. The report contains only `planned`, `forgotten`, or `not_found`
plus a count. A keyed tombstone blocks a concurrently delivered hook that still
holds the deleted project identity; existing ingestion cursors remain so old
transcripts are not replayed. The project path itself may still be retained by
the user's shell history when supplied on a command line.

`prune` accepts exactly one global policy: delete events strictly older than
`--older-than-days`, or keep only the newest `--keep-recent` events. It previews
an aggregate candidate count by default without modifying or initializing
history. A keep count of zero selects all events. `--apply` deletes at most 500
rows per immediate transaction, making an interrupted run safe to retry.
Reports contain only `planned`, `pruned`, or `no_changes` and an event count.
Project mappings, forgetting tombstones, and ingestion cursors are retained.

`recall` returns at most 20 fixed-schema aggregate observations and defaults to
an approximate 600-token serialized-output budget. It supports a controlled
`--operation` filter, `--failures`, and ephemeral task-query ranking. It never
returns event, session, or project identifiers, timestamps, paths, query text,
or historical content. Query text is not persisted by Praxis, though text
provided on a command line may still be retained by the user's shell history.

For automatic Codex recording, install `praxis` somewhere available in the
Codex process's `PATH`, then preview and apply the native hook to the explicit
user config:

```bash
codex_config="${CODEX_HOME:-$HOME/.codex}/config.toml"
praxis install-codex-hook --config "$codex_config"
praxis install-codex-hook --config "$codex_config" --apply
praxis status --codex-config "$codex_config"
```

The installer adds one matcherless `PostToolUse` group, preserving other
settings and hook groups. It reloads under Codex's adjacent config lock and
writes atomically. It refuses invalid hook structures or a config that
explicitly disables lifecycle hooks. Non-managed Codex hooks require review;
open `/hooks` in Codex and trust the Praxis command before testing it. Native
recording uses the documented hook contract and can derive a controlled outcome
only when the tool response contains explicit structural metadata. Current
Codex shell hooks often omit exit status, so `unknown` is expected and is safer
than inferring success from output text. Praxis never stores raw input or
output.

AoE's integration remains the transcript-based alternative. Choose native
Codex recording or the Praxis AoE fallback for a given session, not both:
current Codex hook IDs are not guaranteed to match transcript call IDs, so the
two sources cannot be safely deduplicated. If the Praxis AoE hook is already
installed, leave the Codex hook uninstalled unless those AoE entries are first
removed. To use the fallback, run `praxis print-aoe-config` and manually merge
the snippet into a global/profile config, or pass the explicit global config
file to `install-aoe-hook`. The install command previews by default and writes
only with `--apply`. It preserves unrelated TOML and existing active hook slots,
uses AoE's adjacent global-config lock, and refuses conflicting hooks or a
change that would activate dormant personal hooks. AoE does not honor status
hooks from repository configuration.

The installed AoE entries invoke `praxis aoe-hook` on stable idle/error
transitions. The handler reads only `AOE_SESSION_ID`, `AOE_PROFILE`, and
`AOE_TOOL`; repeated deliveries from that same source are safe. Unsupported
agent types are ignored successfully.

For Claude Code, run `praxis print-claude-config` and manually merge the JSON
fragment into user or project settings without replacing existing hooks. It
registers the same silent `praxis record-hook --agent claude` command for both
`PostToolUse` and `PostToolUseFailure`. The adapter determines success from the
hook event, never from raw tool response or error text, and duplicate hook
deliveries are safe.

## Agent-facing recall

The tracked [`praxis-recall`](skills/praxis-recall/SKILL.md) skill tells a fresh
agent to wait until the task is known, request one bounded task-specific
aggregate, and validate any remembered pattern against current state. Its
workflow is provider-neutral and calls only `praxis recall`. The optional
`agents/openai.yaml` file contains Codex discovery metadata; other agent hosts
can ignore it and register the same `SKILL.md` through their own skill-loading
mechanism.

Pass the selected agent host's skills directory to `install-skill`. The command
previews its destination and two packaged files by default without touching the
filesystem; repeat it with `--apply` to install. A repeated install reports
`already_current`. Praxis refuses to replace a changed file, non-directory, or
symlinked skill destination, and does not guess or modify an agent's config.

See [Architecture](docs/architecture.md) for module boundaries, data flow, and
the current implementation limits, and [Roadmap](docs/roadmap.md) for the next
integration slices. Upstream source-format assumptions are recorded in
[Source format notes](docs/source-format-notes.md).

## Privacy invariant

Only controlled events, cursors, and the deliberately non-serializable
`HookObservation` may enter application services. A project path crosses in a
non-serializable `ProjectLocator` whose only consumer is the encrypted identity
resolver; the path never enters an event, report, or plaintext database column.
Events and private metadata are serialized to CBOR and encrypted in memory
before SQLite receives any payload bytes. The master key is stored separately
through Linux Secret Service; the implementation has no automatic
plaintext-key or plaintext-event fallback.

## Verification

```bash
cargo fmt --all -- --check
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
```

The same checks run in GitHub Actions for pull requests and pushes to `master`.
A passing push to `master` publishes the version declared in `Cargo.toml` when
that version does not already have a GitHub release. The workflow builds with
the declared minimum Rust version, creates a versioned binary archive and
`SHA256SUMS`, tags the tested commit, and generates release notes. Existing
release tags and assets are never replaced by a later commit.

The first such push publishes `v0.1.0`. For each later release, update the
package version in `Cargo.toml`, refresh `Cargo.lock`, run the verification
commands above, and push the tested commit to `master`.
