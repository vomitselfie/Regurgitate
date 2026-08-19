# Roadmap

Regurgitate is being built in small vertical slices. Each slice keeps host-specific
discovery, application policy, query logic, and persistence behind separate
module boundaries.

## Working now

- AoE-managed Codex session discovery and strict normalization;
- encrypted per-event SQLite storage with a separate Secret Service or Keychain
  master key;
- encrypted project mappings and append-safe ingestion cursors;
- manual, interruption-safe session ingestion;
- project-scoped aggregate recall;
- fixed-vocabulary learning for meaningful, directly verified practice
  outcomes;
- strategy confidence and `prefer`/`avoid` guidance that discounts unknown
  activity volume;
- transient task-query ranking with no query persistence;
- hard observation and approximate serialized-token budgets;
- identifier-only AoE idle/error hook ingestion with generated configuration;
- an installable AoE API-v12 release-binary plugin with aggregate health,
  status UI, and explicit one-action Codex/Claude setup;
- native Codex `PostToolUse` recording with generated configuration and a
  preview-first explicit-path installer;
- native Claude Code success/failure hook recording with generated
  configuration and a preserving installer;
- a shared, sanitized native-hook observation boundary and cross-adapter
  conformance fixtures;
- read-only key-store and database health with aggregate-only output;
- explicit-path AoE, Codex, and Claude hook readiness with controlled output;
- preview-first project forgetting with transactional encrypted deletion;
- preview-first age/count retention with fixed-size deletion transactions;
- a provider-neutral recall skill with isolated Codex discovery metadata;
- a preview-first, no-overwrite installer for an explicit agent skills path;
- locked, atomic installers for empty AoE idle/error slots, an additive Codex
  matcher group, and additive Claude terminal events;
- version-gated CI releases with verified Linux x86-64, Apple Silicon, and
  Intel macOS archives plus combined checksums;
  and
- adversarial privacy, authentication, migration, project-isolation, and
  source-replacement tests.

## Completed integration slice

Claude Code was added without coupling its payload to the core:

1. Codex and Claude fixtures exercise one shared conformance assertion;
2. Claude's documented tool hooks provide explicit success/failure events;
3. native adapters reuse encrypted event and project ports through a separate,
   cursor-free recording service; and
4. Claude payload types and its working-directory locator remain confined to
   the adapter boundary.

## Completed native Codex slice

Native Codex recording now complements the AoE transcript fallback:

1. one additive matcherless `PostToolUse` group records supported local tools;
2. the installer preserves unrelated TOML and hook groups, locks and writes
   atomically, and refuses disabled or malformed lifecycle-hook structures;
3. explicit-path health inspection reuses the same non-mutating parser; and
4. documentation makes the source-selection boundary explicit because current
   native hook IDs cannot be safely joined to transcript call IDs.

## Completed operational slice

The first status surface adds operational safety without opening the event-level
privacy boundary:

1. the health projection contains controlled states and one aggregate event
   count;
2. key-store and database readiness expose no paths or identifiers;
3. the probes never create a key, create or migrate a database, or attempt a
   repair; and
4. unavailable-key-store and damaged-database behavior is covered by tests;
5. optional hook checks inspect only explicitly supplied configs and return no
   paths or command strings.

## Completed forgetting slice

Project forgetting was added without adding event-level inspection:

1. a project-scoped deletion port returns only an optional aggregate count;
2. `forget` previews by default and requires `--apply` for deletion;
3. encrypted events and the private project mapping are removed in one
   immediate transaction;
4. keyed tombstones reject late writes using the deleted identity, while
   retained cursors prevent transcript resurrection; and
5. reports contain no event IDs, project IDs, or paths.

## Completed retention slice

Bounded retention was added on top of the same privacy boundary:

1. age and newest-count policies are validated before storage access;
2. preview returns one aggregate count without mutating or initializing state;
3. apply uses immediate transactions capped at 500 rows and can be retried after
   interruption;
4. selection uses structural envelope metadata without decrypting event
   payloads; and
5. output contains no retained or deleted event details.

## Completed actionable-recall slice

Recall now distinguishes activity from evidence:

1. adapters derive patch/write strategies only from controlled tool identity;
2. `learn` accepts no free text and records only a fixed strategy plus an
   explicit known outcome after a meaningful verification;
3. guidance is withheld until two known outcomes exist, then reports bounded
   confidence and deterministic `prefer`, `avoid`, or `mixed` advice;
4. unknown counts do not contribute evidence score or outrank a verified
   strategy merely through volume; and
5. recall uses only existing read-only key/database paths and leaves missing
   local state untouched.

The controlled learning vocabulary also covers research workflows through a
shared `research` / `analyze` classification and explicit
`reproduce_then_compare`, `per_subject_streaming`, and `resource_cap_first`
strategies.

## Completed AoE plugin slice

Regurgitate now participates in AoE without moving AoE concerns into the memory
core:

1. the repository manifest installs the existing static release asset through
   AoE's native plugin manager;
2. the binary enters worker mode only for AoE's exact plugin identity and
   otherwise remains the normal provider-neutral CLI;
3. isolated protocol and view modules publish only aggregate health to the
   status bar and settings page;
4. explicit setup actions install the selected agent's native hook and recall
   skill using the AoE-managed executable, without requiring a PATH install;
5. existing user configuration is preserved and unsafe conflicts fail closed;
   and
6. recording stays on native provider hooks or the AoE transcript fallback
   because the plugin API is not a normalized tool-completion source.

## Next integration slice

Add human-only aggregate inspection without creating an event-export surface:

1. define a fixed-schema summary of counts by controlled capability, operation,
   outcome, and agent;
2. enforce global row/group and token budgets before output;
3. require an explicit human CLI command that is not used by the recall skill;
   and
4. exclude identifiers, timestamps, paths, and event-level records.

Host config installers require explicit paths instead of guessing them and are
preview-only without `--apply`. The AoE installer preserves unrelated TOML but
refuses occupied status slots because upstream supports one command string per
transition rather than a composable command list.

## Later slices

- human-only inspection and key maintenance;
- additional native agent adapters;
- measured evaluation of recall cost versus avoided retries; and
- deeper AoE lifecycle integration if the host publishes a stable,
  provider-neutral tool-completion event stream.
