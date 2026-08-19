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
- hard observation and approximate serialized-token budgets; and
- identifier-only AoE idle/error hook ingestion with generated configuration;
  and
- adversarial privacy, authentication, migration, project-isolation, and
  source-replacement tests.

## Next integration slice

Make bounded recall automatically available to fresh managed agents:

1. ship a small agent-facing skill that advertises bounded recall;
2. query only after the user's task is known;
3. default to the project and a conservative token budget;
4. instruct the agent to validate recalled patterns against current state; and
5. package installation separately from query and storage code.

The hook handler and configuration generator are implemented. A future
installer may merge personal AoE configuration only after an explicit preview
and approval; the current generator never edits it.

## Later slices

- safe status and health reporting;
- retention and project forgetting;
- human-only inspection and key maintenance;
- additional native agent adapters;
- measured evaluation of recall cost versus avoided retries; and
- optional AoE plugin packaging once it materially improves installation or
  lifecycle integration.
