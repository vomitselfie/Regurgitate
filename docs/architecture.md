# Architecture

Praxis uses a ports-and-adapters layout so host integrations, application
coordination, and storage can evolve independently.

```text
CLI/runtime composition
    ├── AoE discovery + Codex transcript adapter
    ├── Codex / Claude native-hook adapters
    ├── cursor-based ingestion or cursor-free recording service
    └── encrypted SQLite adapter + Secret Service key provider

application service ── depends on ──> controlled HistoryEvent model
adapters/storage ────── implement ───> narrow application ports
```

The binary entry point contains no business logic. `runtime` selects concrete
adapters, `application` coordinates them, and `core` contains the controlled
event vocabulary. AoE and Codex details are confined to `adapters`; encryption,
SQLite, and credential-store details are confined to `storage`.

## Native hook flow

Codex and Claude hook payloads enter separate adapter modules. Each adapter
deserializes only an allowlist of structural fields and returns the same
`HookObservation`: one controlled `HistoryEvent` plus a non-serializable
`ProjectLocator`. Provider request data, errors, transcript paths, and other
unknown fields are omitted. Codex transiently classifies explicit response
metadata into a controlled outcome; Claude does not deserialize its response or
failure text at all. No raw provider value crosses the adapter boundary.
Both adapters derive `structured_patch` or `direct_text_mutation` only from an
unambiguous controlled tool name; no arguments or content are consulted.

The recording application service resolves the locator through encrypted
project identity storage, adds the resulting UUID to the event, and appends it
idempotently. It has no transcript or cursor dependency, so a native delivery
cannot advance or corrupt the AoE/Codex transcript checkpoint. The stable event
ID makes a repeated native delivery safe within that source. Native Codex hook
IDs are not guaranteed to match transcript call IDs, so Praxis does not claim
cross-source deduplication. A deployment should select native recording or AoE
transcript fallback for a given Codex session rather than enable both.

Codex exposes `PostToolUse` for successful and non-zero local tool completions.
Its adapter classifies only explicit structural response metadata and otherwise
keeps the outcome `unknown`. `print-codex-config` emits a matcherless native
hook, while `install-codex-hook` can add that group to an explicit user config.

Claude exposes distinct `PostToolUse` and `PostToolUseFailure` events. Its
adapter therefore derives outcome from the event name and does not inspect raw
response or failure content. The `record-hook` runtime path is silent on
success. `print-claude-config` emits a fragment for manual merging instead of
mutating hook arrays that may contain personal commands.

## Explicit learning flow

Some provider contracts cannot reliably distinguish semantic success from
failure. `praxis learn` fills that gap without opening a free-text memory path.
It accepts only a project locator, one fixed strategy enum, and an explicit
`success` or `failure`. Each learnable strategy maps to one canonical
capability/operation pair in the core model; arbitrary labels and `unknown`
outcomes are rejected by the CLI/application boundary.

Research methods use the shared `research` capability and `analyze` operation,
with `reproduce_then_compare`, `per_subject_streaming`, or
`resource_cap_first` carrying the procedural distinction. They are
explicit-only because neither a provider tool name nor private tool arguments
can safely establish which analysis method was used.

The learning application service creates a controlled event with no session or
agent identity, then reuses the same encrypted project resolver and recording
port as native hooks. The agent-facing skill permits this only after a
meaningful result is directly established by validation or user confirmation,
and at most once per milestone. Tool-by-tool activity and ambiguous outcomes
are deliberately not learned.

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
in the cursor only as its deterministic UUID, timestamp, agent, capability,
operation, and optional controlled strategy. A changed or truncated committed
prefix causes a safe reset and
reparse. If event persistence fails, the cursor does not advance; retrying is
safe because already committed event UUIDs are ignored.

## AoE host integration

AoE status hooks are the first automatic recording surface. The `aoe-hook`
command reads only the session ID, resolved profile, and agent type from its
environment. It deliberately ignores the hook's project path, session title,
group, old/new status, and transition timestamp; normal AoE session discovery
remains the single source of project and transcript association.

`print-aoe-config` emits a non-mutating snippet for `on_idle` and `on_error`.
The user must merge it into a global or profile config, preserving existing
personal hooks. Repeated or overlapping delivery reuses the normal cursor and
stable-event idempotency path. SQLite waits briefly for a concurrent local
writer rather than immediately failing with a busy error.

## Agent recall integration

The `skills/praxis-recall` package is a thin consumer of the public recall CLI.
Its `SKILL.md` waits for task context, requests a bounded aggregate, and tells
the agent to verify observations against current state. It does not depend on
AoE, Codex transcript formats, SQLite, or key management.

Provider discovery remains an edge concern. Codex-specific UI metadata lives
under the skill's optional `agents/` directory and can be ignored by Claude,
Hermes, or another host. Future hosts should reuse the workflow and CLI while
keeping native event normalization in a separate ingestion adapter.

The `packaging` module embeds the tracked skill files in release binaries and
installs them only beneath a caller-supplied skills directory. Installation is
preview-only unless `--apply` is explicit. It stages a new package before
renaming it into place and is idempotent for identical files. Different
content is preserved unless `--replace` is explicit; replacement swaps the
whole staged directory through a private backup and restores the old directory
if installation fails. Non-directory and symlinked destinations are always
rejected. Host path discovery and host configuration mutation remain outside
the skill installer.

The same module can conservatively add hooks to explicit AoE and Codex config
files. Provider-specific parsers and policies remain separate, while a small
shared config-file module owns adjacent locking, symlink-safe atomic writes,
permission preservation, and directory handling. The AoE installer rejects
occupied `on_idle` or `on_error` slots and refuses to enable a table containing
other dormant hooks. The Codex installer preserves existing matcher groups and
refuses invalid hook structures or explicitly disabled lifecycle hooks. Both
installers re-read under the provider's adjacent lock before applying.

## Privacy boundary

Raw records are short-lived adapter inputs. Prompts, responses, reasoning,
commands, arguments, tool output, URLs, environment values, and source contents
are neither members of `HistoryEvent` nor accepted by the application or event
storage interfaces. A path is carried only by a non-serializable
`ProjectLocator` to the private identity resolver.

SQLite receives an event UUID, a key-derived project lookup token,
authenticated envelope metadata, a random nonce, and ciphertext. The event UUID
and timestamps are structural metadata; the session ID, project ID, agent type,
operation, outcome, and other event fields are inside the encrypted payload.
Project and cursor tables expose only
HMAC-SHA-256 lookup tokens, version numbers, nonces, and ciphertext. Their
paths, session IDs, cursor offsets, digests, and pending state are encrypted.
The master key is held by Linux Secret Service and is never stored beside the
database.

Debug commands expose only `DebugEvent`, which omits identifiers and timestamps.
The ingestion command exposes only aggregate counts; the native recording
command emits no success output.

## Recall boundary

The recall runtime opens only an existing database and existing key, both
through read-only paths. Missing history returns an empty bounded result without
creating a directory, key, database, schema, or project mapping. Events carry a
separate key-derived project token in SQLite so the storage adapter can select
a project without putting its UUID or path in plaintext. The decrypted project
UUID is verified before an event enters aggregation.

Recall groups events by controlled capability, operation, and strategy. Its
output contains attempt/success/failure/unknown counts and, when present, the
most common controlled error class. Two or more known outcomes add a rounded
success percentage, sample-count confidence, and deterministic `prefer`,
`avoid`, or `mixed` guidance. One-off and all-unknown groups omit those fields.
Ranking considers task relevance and verified guidance before raw sample count,
so high-volume unknown activity cannot bury a smaller actionable strategy. It
has no event-level output mode and rejects limits above 20 before querying
storage. Identifiers and timestamps are used internally for scoping and recency
ranking but are absent from the result.

Optional task text is ephemeral input to a small deterministic classifier. The
classifier keeps only controlled capability and operation hints, then drops the
normalized text. Those hints affect ranking but are not stored or returned.
After ranking and the observation-count limit, the result is serialized and
trimmed from lowest priority until its conservative four-bytes-per-token
estimate fits the requested budget. The output records that estimate for later
evaluation. Budgets above 1,000 tokens are rejected before storage is queried.

## Health boundary

The `status` command composes two narrow read-only probes. The Secret Service
probe checks for an existing, correctly sized master key without entering the
create path. The database probe checks only an existing regular file, opens it
with SQLite read-only flags, runs a bounded integrity check, and returns an
aggregate event count. It does not create directories, initialize tables,
migrate schema, change permissions, decrypt event payloads, or repair damage.

The application service reduces probe results to `ready`, `not_configured`, or
`unavailable` component states and an overall status. Backend errors are not
included in the report, so keyring messages, database paths, and damaged bytes
cannot reach CLI JSON.

Optional hook readiness follows the same reduction. The runtime inspects only
AoE, Codex, and Claude config paths explicitly supplied to `status`. AoE and
Codex reuse their comment-preserving, conflict-aware preview parsers. Claude
uses a typed view of only the two relevant hook arrays; unrelated settings are
not represented, and personal command strings are transient comparison inputs.
If both the AoE fallback and native Codex hook are installed, the service marks
both as conflicting to enforce the source-selection boundary. The health report
retains only a controlled provider and readiness enum.

## Forgetting boundary

The project forgetting service accepts only a non-serializable
`ProjectLocator`, an explicit apply decision, and a narrow storage port. Preview
opens an existing database read-only and returns an aggregate event count.
Missing history stays missing: neither a key, directory, database, nor project
mapping is created.

Apply begins an immediate SQLite transaction, authenticates the encrypted
path-to-project mapping, deletes both indexed and legacy unindexed events, and
removes the encrypted mapping. It intentionally retains ingestion cursors so a
later AoE status transition cannot replay already-forgotten transcript history.
Before deletion, it adds the old key-derived event-project token to a tombstone
table. Appends consult that table, preventing a concurrent hook that resolved
the old project UUID from recreating an orphan event after the mapping is gone.
A later new hook resolves a fresh project identity and can record new work.

The report contains only a controlled status and count. Event IDs, project IDs,
paths, sessions, and tombstone tokens never leave storage.

## Retention boundary

Retention is global and accepts exactly one controlled policy: a validated age
in whole days or a validated newest-event count. Policy validation happens
before key or database access. Preview opens only existing history read-only
and returns one candidate count; missing state is not initialized.

The storage adapter selects age candidates solely from structural
`created_at_ms` envelope metadata, which is also included in event associated
data for authenticated reads. Count retention uses deterministic
`created_at_ms` and event-ID ordering to preserve the newest requested number.
Neither policy decrypts event payloads or selects on private event fields.
Apply deletes no more than 500 rows in each immediate transaction and repeats
until no candidates remain. A failure between batches can leave a safe partial
result; rerunning the same policy continues from current state.

Retention removes event envelopes only. It retains encrypted project mappings,
ingestion cursors, and project-forgetting tombstones. Output contains a
controlled status and total count, never event details or identifiers.

## Current boundaries and limits

Implemented:

- controlled event schema and conservative outcome classification;
- Codex and Claude native-hook normalization with shared conformance fixtures;
- cursor-free, encrypted, idempotent native-hook recording;
- fixed-vocabulary explicit learning for directly verified practice outcomes;
- content-free strategy derivation for unambiguous patch/write tool identities;
- Codex transcript normalization;
- AoE-managed Codex session discovery;
- encrypted, idempotent event persistence;
- encrypted path-to-project UUID mapping with keyed lookup tokens;
- encrypted incremental cursors with append, incomplete-line, truncation, and
  replacement handling;
- Linux Secret Service key retrieval/creation;
- manual session ingestion;
- identifier-only AoE status-hook ingestion and non-mutating config generation;
- native Codex hook config generation and a preview-first explicit-path
  installer;
- non-mutating Claude Code hook config generation;
- non-mutating aggregate key-store and database health reporting;
- explicit-path, non-mutating AoE, Codex, and Claude hook readiness reporting;
- preview-first transactional project forgetting with race-safe tombstones;
- preview-first age/count retention with bounded deletion transactions;
- project-scoped aggregate recall with operation/failure filters and a hard
  observation limit;
- confidence/guidance scoring that excludes unknown events from evidence;
- ephemeral task-query ranking and explicit serialized-output token budgets;
- a provider-neutral agent recall skill with optional Codex metadata;
- a preview-first skill package installer with explicit atomic replacement;
- locked, atomic, conflict-refusing AoE and Codex hook config installers; and
- adversarial privacy, authentication, filesystem-mode, and idempotency tests.

Not yet implemented:

- automatic host-specific installation-path discovery;
- human-only inspection and key-maintenance commands.
