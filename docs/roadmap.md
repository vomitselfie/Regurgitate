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
- adversarial privacy, authentication, migration, project-isolation, and
  source-replacement tests.

## Next integration slice

Make recording automatic at the session boundary while preserving the existing
application ports:

1. add a host-side command that accepts identifiers from an AoE lifecycle or
   status hook;
2. ingest only the linked session and return aggregate status;
3. provide generated configuration or an installer with an explicit dry run;
4. make repeated hook delivery harmless; and
5. keep installation/configuration logic outside normalization and storage.

After recording is automatic, ship a small agent-facing skill that advertises
bounded recall and issues a task query only after the user's task is known.

## Later slices

- safe status and health reporting;
- retention and project forgetting;
- human-only inspection and key maintenance;
- additional native agent adapters;
- measured evaluation of recall cost versus avoided retries; and
- optional AoE plugin packaging once it materially improves installation or
  lifecycle integration.
