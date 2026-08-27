# Architecture

Regurgitate uses a ports-and-adapters layout so host integrations, application
coordination, and storage can evolve independently.

```text
CLI/runtime composition
    ├── AoE discovery + Codex transcript adapter
    ├── Codex / Claude native-hook adapters
    ├── AoE JSON-RPC plugin worker + guided agent setup
    ├── cursor-based ingestion or cursor-free recording service
    └── encrypted SQLite adapter + operating-system key provider

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
IDs are not guaranteed to match transcript call IDs, so Regurgitate does not claim
cross-source deduplication. A deployment should select native recording or AoE
transcript fallback for a given Codex session rather than enable both.

Codex exposes `PostToolUse` for successful and non-zero local tool completions.
Its adapter classifies only explicit structural response metadata and otherwise
keeps the outcome `unknown`. `print-codex-config` emits a matcherless native
hook, while `install-codex-hook` can add that group to an explicit user config.

Claude exposes distinct `PostToolUse` and `PostToolUseFailure` events. Its
adapter therefore derives outcome from the event name and does not inspect raw
response or failure content. The `record-hook` runtime path is silent on
success. `print-claude-config` still emits a fragment for manual use, while
`install-claude-hook` safely adds both events to an explicit user settings file.
The installer preserves unrelated settings and personal hook groups, and
refuses malformed, disabled, or matcher-restricted Regurgitate configurations.

## Experience flow (schema v4)

Provider hooks report execution status, not whether an approach produced a
correct result. `regurgitate experience record` supplies that semantic
boundary as an **experience capsule**: a controlled task, a compositional
`Procedure` (mutation / verification / execution / research / integration
dimensions plus up to six ordered steps), a semantic `success` or `failure`
with an optional controlled `FailureReason`, controlled applicability tags
(artifact kind, phase, ecosystem, tool family, risk shapes), a lifecycle
state, and a bounded evidence log. Each evidence entry carries its own minimal
environment fingerprint (tool family, major version, host class), controlled
source/verification/attestation, and optional opaque cohort and confirmation
receipt digest.

The only natural language in the vault is three deliberately authored
sentences: `situation` (≤ 240 chars), `lesson` (≤ 320), and `caveat`
(≤ 160). `BoundedText<N>` enforces the cap and a conservative structural
admission check at construction *and* at deserialization, rejecting anything
that resembles URLs, paths, file names, commands, code, serialized payloads,
credentials, opaque identifiers, or conversation. False rejections are
accepted by design; a rejected sentence is rephrased, a wrongly admitted one
would be archived.

Capsules live in a separate `experiences` table. The plaintext columns are an
opaque id, an HMAC scope token, an HMAC origin (project) token, two
timestamps, and version numbers; everything else is inside an
XChaCha20-Poly1305 envelope whose associated data binds those columns, so a
row cannot be moved between scopes, origins, or times. Storage never gains a
semantic plaintext column. The versioned encrypted codec reads schema v3 and
v4 but writes only v4. It upgrades v3 environments onto their evidence entries
in memory; read-only recall never rewrites a row, while its next real mutation
lazily re-encrypts the same identity as v4. The encryption envelope remains
version 1.

Before inserting, the experience service loads a bounded window from the
target scope, decrypts it in memory, and looks for a capsule with the same
controlled identity (task + procedure + applicability) whose situation text
is equivalent by ephemeral token similarity. An equivalent capsule is
**confirmed** (evidence appended, `last_confirmed_at` advanced) instead of
duplicated. An equivalent situation with a dissimilar lesson marks both
capsules **challenged**. Recovery requires three distinct supporting
post-challenge cohorts; opposing evidence resets progress. `experience
supersede` resolves such conflicts explicitly, and `obsolete` retires a lesson.
Superseded and obsolete capsules
never enter normal recall; challenged ones surface flagged and down-weighted.
No similarity vectors or decrypted working sets are retained after the call.

`regurgitate learn --task --strategy --outcome` is kept as a compatibility
shorthand: it records a text-free project-scoped capsule whose procedure is
the `Strategy -> Procedure` migration mapping. Pre-v0.8 `LearnedPractice`
rows are never rewritten; recall materializes them as legacy one-step
capsules without context, which rank below context-bearing capsules of
equal relevance but still contribute posterior evidence.

Scope is a relevance prior, not access control: project, workspace (parent
directory identity), ecosystem, machine, and global buckets each get their own
HMAC token. Forgetting a project deletes every capsule recorded from it in
any scope, and a forgotten identity's tombstone rejects late capsule writes
exactly as it rejects late events. Age-based retention prunes capsules by
their last confirmation alongside old events.

## Recall flow

Recall is two-stage and read-only. Stage one normalizes the task
ephemerally—explicit controlled metadata (`--task`, `--phase`, `--artifact`,
`--ecosystem`, `--tool-family`, `--tool-major`, `--risk`) outranks keyword
inference from `--query`, which outranks controlled project defaults inferred
from allowlisted marker presence. Query text is zeroized. Detection never
reads marker or source contents and never invokes a project tool. Recall then loads a bounded candidate window from
the project scope (capsules plus legacy rows). Only if fewer than a handful of
applicable active project capsules exist does it expand outward to workspace,
ecosystem, machine, and global buckets, and broader capsules must clear a
higher applicability bar to surface.

Stage two reranks the decrypted window. Every ranking constant lives in one
`RankingPolicy`:

- applicability `a = 0.35·task + 0.25·artifact + 0.20·ecosystem/tool +
  0.10·phase + 0.10·environment`, where a silent context cannot penalize a
  dimension and an untagged capsule scores half on a dimension the context
  names;
- per-evidence weight `w = W_scope · exp(−ln2 · age / H_scope) · a ·
  W_lifecycle · W_version · W_provenance` with scope priors 1.00 / 0.85 / 0.65 / 0.55 / 0.35, half-lives
  of 120 / 120 / 180 / 60 / 180 days, and lifecycle weights active 1,
  challenged 0.35, otherwise 0;
- explicit cohorts are capped at their strongest observation; unattributed
  evidence is grouped by UTC day plus controlled provenance and environment;
- a Beta(1 + ΣS, 1 + ΣF) posterior with an 80 % credible interval computed
  from a regularized incomplete beta (no external numeric dependency), with
  effective evidence bounded by cohort-level Kish size and absolute mass;
- guidance only when `n_eff ≥ 2.5`: `prefer` if the interval's lower bound is
  ≥ 0.65, `avoid` if its upper bound is ≤ 0.35, `mixed` if the mean sits
  between the thresholds, otherwise no label (the posterior stays visible);
- equivalent capsules cluster by controlled identity before aggregation, and
  the final order is `0.45·applicability + 0.25·guidance strength +
  0.15·confidence + 0.10·recency + 0.05·scope prior`, plus a small bonus for
  context-bearing capsules.

Broader scopes require situation and lesson context. Risk-sensitive requests
move their best applicable failed or challenged lesson ahead of trimming. The
serialized brief carries no identifiers or timestamps and is trimmed
from the bottom until it fits the token budget; the hard maximum is still
enforced before storage is queried.

## Preflight injection

`RecallBroker` is a host-neutral port that returns a plain-text experience
brief for a task context and token budget, or nothing at all when no lesson
is relevant. `regurgitate preflight --project … --query …` exposes it for
any host with a startup instruction channel. For Claude Code,
`regurgitate preflight --agent claude` reads the `UserPromptSubmit` payload
(only `cwd`, `hook_event_name`, and a transient `prompt` are deserialized),
classifies the prompt ephemerally, and answers with `additionalContext`; an
irrelevant prompt produces no output and therefore no context overhead.
Preflight is stricter than `recall` on purpose: once a project has lessons
with moderate or strong evidence (`n_eff ≥ 2.5`) it injects only those. When
nothing that strong exists yet it bootstraps with at most two capsules tagged
`unconfirmed`, so evidence can start accumulating at all. Every recalled
capsule carries an authenticated, stateless `ref`; `experience confirm
--match <ref>` appends one evidence entry to exactly that capsule in whatever
scope it lives. Replaying one receipt is idempotent, and issuing it leaves the
read-only database untouched. A budget-trimmed brief
ends with an "N more omitted" line. The
Claude config printer and installer add that hook next to the two recording
hooks. Preflight never fails a host prompt: a missing database or key simply
yields an empty brief, and it never creates state. Codex and AoE do not yet
expose a stable pre-task lifecycle hook, so they stay on the skill-driven
(Tier B) path; the core is not distorted to fake automation those hosts
cannot provide.

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

The repository root also carries an AoE API-v12 manifest. Its release-asset
template selects Linux x86-64, Apple Silicon, or Intel macOS using AoE's host
OS and architecture substitutions. Every archive exposes the same root
`regurgitate` path, so packaging stays separate from worker dispatch. AoE launches
that binary with no arguments plus the exact `AOE_PLUGIN_ID`; the binary enters
worker mode only for that combination, while every ordinary invocation
continues through the CLI. The plugin protocol and UI projection live in their
own `aoe_plugin` modules, separate from transcript discovery and the controlled
event model.

The worker speaks newline-delimited JSON-RPC over stdio, keeps stdout exclusive
to protocol messages, and exits when the host closes stdin. It uses the normal
read-only health service to publish a global status segment and settings page.
It also offers explicit Codex and Claude Code setup actions through the
settings page and contributed commands. Plugin installation itself never
mutates another program's configuration because AoE has no post-install setup
contract and such a mutation should require a user action.

The setup service resolves the plugin's own release binary and writes that
absolute executable path into the selected agent's hook and generated skill.
Plugin-only users therefore do not need a second download or a `PATH` change.
Before writing either destination it previews both, preserves existing user
settings and hooks, and refuses unsafe conflicts. The manifest declares
`fs.read` and `fs.write` for this narrowly scoped user-config operation. Setup
results, like health, are reduced to controlled states; backend errors, paths,
identifiers, and event data cannot enter the AoE view.

AoE currently supervises plugin workers only from `aoe serve`. Its plugin API
does not provide a normalized provider tool-completion feed, so the plugin is a
packaging, setup, and operational surface rather than an ingestion path by
itself. The native hook installed by the setup action—or the AoE idle/error
transcript fallback—records into the same encrypted store.

## Agent recall integration

The `skills/regurgitate-recall` package is a thin consumer of the public CLI.
Its `SKILL.md` recalls once only when prior experience could materially change
non-trivial work, confirms a lesson only when it influenced the result, and
records at most one capsule only for a novel reusable verified lesson. Simple
questions, mechanical edits, formatting, injected briefs, `no_matches`, and
`unavailable` results cause no additional memory workflow. It does not
depend on AoE, provider transcript formats, SQLite, or key management.

An agent command sandbox may deny access to the operating-system credential
store even when the user's desktop session has unlocked it. In that case the
skill permits one retry through the host's standard per-command approval path,
scoped to the exact `regurgitate recall` or `regurgitate experience record` prefix. The binary cannot
and does not escape the sandbox itself, and the workflow never approves a shell
wrapper or changes credentials, storage, filesystem, or network policy.

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
rejected. The AoE setup service can render one additional instruction that
pins skill commands to the worker executable; the tracked provider-neutral
skill remains unchanged. Host path discovery remains outside the installer.

The same module can conservatively add hooks to explicit AoE, Codex, and Claude config
files. Provider-specific parsers and policies remain separate, while a small
shared config-file module owns adjacent locking, symlink-safe atomic writes,
permission preservation, and directory handling. The AoE installer rejects
occupied `on_idle` or `on_error` slots and refuses to enable a table containing
other dormant hooks. The Codex installer preserves existing matcher groups and
refuses invalid hook structures or explicitly disabled lifecycle hooks. The
Claude installer preserves existing JSON settings and hook groups while adding
both terminal events and the `UserPromptSubmit` preflight hook. All installers re-read under the provider's adjacent
lock before applying.

## Project identity

A project is the repository, not the directory an agent happens to run
from. The project resolver canonicalizes the supplied path, then walks up to
the nearest initialized `.git` directory (identified by its regular `HEAD`): a subdirectory resolves to its checkout root, and a
linked git worktree (whose `.git` is a file naming
`<main>/.git/worktrees/<name>`) resolves to the main checkout. A path with no
enclosing `.git` keeps its own identity, so scratch directories stay
isolated rather than polluting a real project. Resolution never shells out
or reads repository content, and the resolved path is only ever an input to
the keyed lookup token and the encrypted project record. Forks and separate
clones remain distinct projects; `workspace` and `ecosystem` scopes exist
for lessons that should cross that line.

## Privacy boundary

Raw records are short-lived adapter inputs. Prompts, responses, reasoning,
commands, arguments, tool output, URLs, environment values, and source contents
are neither members of `HistoryEvent` nor accepted by the application or event
storage interfaces. A path is carried only by a non-serializable
`ProjectLocator` to the private identity resolver.

SQLite receives an event UUID, a key-derived project lookup token,
authenticated envelope metadata, a random nonce, and ciphertext. The event UUID,
timestamp, and controlled `hook_execution`/`learned_practice` discriminator are
structural metadata. That discriminator permits bounded per-kind retrieval and
aggregate health counts without exposing the learned task or strategy. Session
ID, project ID, agent type, task, strategy, operation, and outcome remain inside
the encrypted payload.
Project and cursor tables expose only
HMAC-SHA-256 lookup tokens, version numbers, nonces, and ciphertext. Their
paths, session IDs, cursor offsets, digests, and pending state are encrypted.
The master key is held by Linux Secret Service or macOS Keychain and is never
stored beside the database.

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

Recall retrieves capsules, legacy practice rows, and hook executions through
separate bounded queries, so high-volume telemetry cannot evict useful
lessons from the candidate window. `status` is `matches`, `no_matches`, or
`unavailable`; the latter two reduce to one compact object. `experiences` lists ranked lessons (see
"Recall flow") with their posterior, interval, effective evidence, and, when
the evidence supports it, `prefer`, `avoid`, or `mixed` guidance.
`--failures` keeps only capsules with failed evidence. When the result limit
or the token budget drops matching lessons, `omitted` reports how many, so a
caller can re-query with a larger budget or a narrower context.

Provider-reported execution counts appear under the separate `hook_summary`
key and never participate in guidance. They describe a bounded recent sample
and make no claim that an approach was correct. Recall has no event-level mode
and rejects observation limits above 20. Identifiers and timestamps remain
absent from the result.

Optional task text is ephemeral input to a deterministic classifier. It reduces
the query to controlled task, capability, and operation hints, then drops the
normalized text. A supplied query filters lessons to matching controlled
tasks; a query with no recognized task yields no lessons rather than generic
telemetry. The query is neither stored nor returned.
After ranking and the result limit, the result is serialized and
trimmed from lowest priority until its conservative four-bytes-per-token
estimate fits the requested budget. The output records that estimate for later
evaluation. Budgets above 1,000 tokens are rejected before storage is queried.

The agent-facing instruction bundle is part of the same context boundary. A
regression test keeps the embedded `SKILL.md` at or below 3,000 bytes. The
current bundle, including semantic-learning and sandbox guidance, is 2,779
bytes, down from the initial 4,819. Using the CLI's conservative
four-bytes-per-token estimate, that is approximately 695 tokens instead of
1,205. Together with the skill's 240-token recall budget, the
Regurgitate-controlled ceiling for an activated recall is roughly 935 tokens
instead of 1,805, a 48% reduction. Successful recording hooks remain
silent and therefore add no Regurgitate output to agent context. AoE-rendered skills
also include one instruction containing the local worker path, whose length is
host-dependent. These figures describe the tracked skill and Regurgitate-owned
serialized payloads; provider tokenization and host-added tool-call scaffolding
are outside this boundary.

## Health boundary

The `status` command composes two narrow read-only probes. The operating-system
credential-store probe checks for an existing, correctly sized master key
without entering the create path. The database probe checks only an existing
regular file, opens it with SQLite read-only flags, runs a bounded integrity
check, and returns total, hook-execution, and learned-practice counts. It does
not create directories, initialize tables, migrate schema, change permissions,
decrypt event payloads, or repair damage.

The application service reduces probe results to `ready`, `not_configured`, or
`unavailable` component states and an overall status. Backend errors are not
included in the report, so keyring messages, database paths, and damaged bytes
cannot reach CLI JSON.

`experience metrics` is a separate read-only aggregate projection over one
project's encrypted capsules. It reports lifecycle totals, evidence count, and
successful or failed authenticated confirmations without lesson text or IDs.
Recall hit-rate and latency remain explicit paired-benchmark measurements;
recall itself writes no telemetry.

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
path-to-project mapping, deletes the project's indexed events, and
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
- fixed-vocabulary task and strategy learning for semantic practice outcomes;
- content-free strategy derivation for unambiguous patch/write tool identities;
- Codex transcript normalization;
- AoE-managed Codex session discovery;
- encrypted, idempotent event persistence;
- encrypted path-to-project UUID mapping with keyed lookup tokens;
- encrypted incremental cursors with append, incomplete-line, truncation, and
  replacement handling;
- Linux Secret Service and macOS Keychain key retrieval/creation;
- manual session ingestion;
- identifier-only AoE status-hook ingestion and non-mutating config generation;
- an AoE API-v12 release-binary plugin with supervised JSON-RPC health and
  explicit Codex/Claude setup UI;
- native Codex hook config generation and a preview-first explicit-path
  installer;
- Claude Code hook config generation and a preview-first explicit-path
  installer;
- non-mutating aggregate key-store and database health reporting;
- explicit-path, non-mutating AoE, Codex, and Claude hook readiness reporting;
- preview-first transactional project forgetting with race-safe tombstones;
- preview-first age/count retention with bounded deletion transactions;
- project-scoped aggregate recall with operation/failure filters and a hard
  observation limit;
- separate hook telemetry and semantic practice recall;
- task-filtered query matching and explicit serialized-output token budgets;
- a provider-neutral agent recall skill with optional Codex metadata;
- a preview-first skill package installer with explicit atomic replacement;
- locked, atomic, conflict-refusing AoE, Codex, and Claude hook config
  installers; and
- adversarial privacy, authentication, filesystem-mode, and idempotency tests.

Not yet implemented:

- automatic host-specific installation-path discovery outside the AoE worker;
- human-only inspection and key-maintenance commands.
