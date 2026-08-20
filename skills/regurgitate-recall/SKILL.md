---
name: regurgitate-recall
description: Recall project practice and record one controlled semantic outcome after meaningful success or failure. Use after understanding a task, rejecting an approach, or finishing verified work.
---

# Regurgitate Recall

Treat Regurgitate evidence as advisory, never as current truth.

## Recall relevant practice

Recall at most once after understanding a task when prior practice could help:

```bash
regurgitate recall --project "$PWD" --query "<short non-secret task category>" --token-budget 300
```

Use generic text such as `data import` or `integration debugging`; never include
prompts, source, commands, paths, credentials, or private values.
`observations` contains explicitly evaluated practice matched to the query.
`hook_summary` is tool execution telemetry, not approach correctness. Continue
if observations are empty or Regurgitate is unavailable, and verify guidance.

## Record the semantic outcome

Before finishing a milestone, or after abandoning an approach, record one
result when a controlled strategy materially affected it:

```bash
regurgitate learn --project "$PWD" --task <task> --strategy <strategy> --outcome <success|failure>
```

Outcome means whether the strategy worked, not whether its tool call executed.
A command can exit 0 while `direct-text-mutation` failed by corrupting a file.
A `targeted-verification` that exposes a broken change succeeded even though
the change failed. Record once per milestone; skip duplicates, low-level work,
ambiguous results, and work with no exact strategy.

Task values are `configuration`, `data-import`, `debugging`,
`dependency-update`, `documentation`, `feature-implementation`, `integration`,
`performance`, `refactoring`, `release`, `research`, `security`, and `testing`.
Use an exact strategy from `regurgitate learn --help`. Research strategies mean:

- `reproduce-then-compare`: reproduce a baseline, then compare consistently.
- `per-subject-streaming`: complete and emit each subject independently.
- `resource-cap-first`: set the exploration limit before research begins.

After a meaningful failed strategy, one focused recall is allowed:

```bash
regurgitate recall --project "$PWD" --failures --query "<short non-secret task category>" --token-budget 200
```

`--failures` selects failed learned practices, not tool calls. Use guidance only
when present and weigh confidence and sample size.

## Sandboxed hosts

If the credential store is sandbox-blocked, retry once with approval scoped to
the exact `regurgitate recall` or `regurgitate learn` prefix. Never approve a
shell wrapper, broaden access, or change credentials or policy.

## Preserve the boundary

Never inspect or export databases, keys, cursors, identifiers, or events.
Queries are ephemeral; learned values are controlled. There is no raw history.
