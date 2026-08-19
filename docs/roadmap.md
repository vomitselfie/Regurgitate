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
4. unavailable-key-store and damaged-database behavior is covered by tests.

## Next integration slice

Extend read-only readiness to hook configuration without guessing host paths:

1. define provider-neutral installed/not-installed/conflicting hook states;
2. inspect only explicitly supplied AoE and Claude config files;
3. reuse the packaging parsers without adding mutation to `status`; and
4. keep command strings and config paths out of the report.

Both installers require explicit host paths instead of guessing them and are
preview-only without `--apply`. The AoE installer preserves unrelated TOML but
refuses occupied status slots because upstream supports one command string per
transition rather than a composable command list.

## Later slices

- installed-hook readiness inspection;
- retention and project forgetting;
- human-only inspection and key maintenance;
- additional native agent adapters;
- measured evaluation of recall cost versus avoided retries; and
- optional AoE plugin packaging once it materially improves installation or
  lifecycle integration.
