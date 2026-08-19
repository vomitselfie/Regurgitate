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
- a provider-neutral recall skill with isolated Codex discovery metadata;
- a preview-first, no-overwrite installer for an explicit agent skills path;
- a locked, atomic installer for empty AoE idle/error hook slots; and
- adversarial privacy, authentication, migration, project-isolation, and
  source-replacement tests.

## Next integration slice

Add another agent source without coupling it to the core:

1. define adapter conformance fixtures around the controlled event boundary;
2. select the next native source based on stable hook/transcript support;
3. reuse the existing ingestion service and encrypted storage ports; and
4. keep native payload types and paths confined to the new adapter.

Both installers require explicit host paths instead of guessing them and are
preview-only without `--apply`. The AoE installer preserves unrelated TOML but
refuses occupied status slots because upstream supports one command string per
transition rather than a composable command list.

## Later slices

- safe status and health reporting;
- retention and project forgetting;
- human-only inspection and key maintenance;
- additional native agent adapters;
- measured evaluation of recall cost versus avoided retries; and
- optional AoE plugin packaging once it materially improves installation or
  lifecycle integration.
