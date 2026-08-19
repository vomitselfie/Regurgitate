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
- a provider-neutral recall skill with isolated Codex discovery metadata; and
- adversarial privacy, authentication, migration, project-isolation, and
  source-replacement tests.

## Next integration slice

Package the existing integration surfaces without coupling them to the core:

1. provide an explicit, previewable installation path for the recall skill;
2. keep host-specific discovery metadata outside the provider-neutral workflow;
3. package AoE hook configuration without silently replacing personal hooks;
4. add adapter conformance fixtures before supporting another agent source; and
5. keep installation code separate from query, ingestion, and storage policy.

The skill, hook handler, and configuration generator are implemented. A future
installer may merge personal configuration only after an explicit preview and
approval; the current repository never edits it.

## Later slices

- safe status and health reporting;
- retention and project forgetting;
- human-only inspection and key maintenance;
- additional native agent adapters;
- measured evaluation of recall cost versus avoided retries; and
- optional AoE plugin packaging once it materially improves installation or
  lifecycle integration.
