# Roadmap

Praxis is being built in small vertical slices. Each slice keeps host-specific
discovery, application policy, query logic, and persistence behind separate
module boundaries.

## Working now

- AoE-managed Codex session discovery and strict normalization;
- encrypted per-event SQLite storage with a separate Secret Service master key;
- encrypted project mappings and append-safe ingestion cursors;
- manual, interruption-safe session ingestion;
- project-scoped aggregate recall;
- transient task-query ranking with no query persistence;
- hard observation and approximate serialized-token budgets;
- identifier-only AoE idle/error hook ingestion with generated configuration;
- native Claude Code success/failure hook recording with generated
  configuration;
- a shared, sanitized native-hook observation boundary and cross-adapter
  conformance fixtures;
- read-only key-store and database health with aggregate-only output;
- explicit-path AoE and Claude hook readiness with controlled output;
- preview-first project forgetting with transactional encrypted deletion;
- preview-first age/count retention with fixed-size deletion transactions;
- a provider-neutral recall skill with isolated Codex discovery metadata;
- a preview-first, no-overwrite installer for an explicit agent skills path;
- a locked, atomic installer for empty AoE idle/error hook slots; and
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

## Next integration slice

Add human-only aggregate inspection without creating an event-export surface:

1. define a fixed-schema summary of counts by controlled capability, operation,
   outcome, and agent;
2. enforce global row/group and token budgets before output;
3. require an explicit human CLI command that is not used by the recall skill;
   and
4. exclude identifiers, timestamps, paths, and event-level records.

Both installers require explicit host paths instead of guessing them and are
preview-only without `--apply`. The AoE installer preserves unrelated TOML but
refuses occupied status slots because upstream supports one command string per
transition rather than a composable command list.

## Later slices

- human-only inspection and key maintenance;
- additional native agent adapters;
- measured evaluation of recall cost versus avoided retries; and
- optional AoE plugin packaging once it materially improves installation or
  lifecycle integration.
