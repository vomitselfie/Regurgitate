---
name: praxis-recall
description: Recall bounded, privacy-safe procedural evidence for the current project. Use after the task is understood when prior local strategies could prevent repeated exploration, after a meaningful failure, or before declaring an approach blocked. Never use it to recover raw history or secrets.
---

# Praxis Recall

Treat Praxis aggregates as advisory evidence, never current truth.

## Recall once

After the task is understood, recall only when a procedural pattern could avoid
repeated exploration:

```bash
praxis recall --project "$PWD" --query "<short non-secret category>" --token-budget 300
```

Use generic terms such as `test failure` or `dependency update`; never place
prompts, source, commands, paths, credentials, or private values in `--query`.
Continue normally if Praxis is unavailable or empty. Do not change credentials
or privacy settings to make recall work. Validate relevant evidence against the
current repository and constraints before acting.

## Record a verified practice

After a meaningful approach is directly verified by a test, validation,
provider result, or user confirmation, record at most one outcome:

```bash
praxis learn --project "$PWD" --strategy <controlled-strategy> --outcome <success|failure>
```

Use only an exact `praxis learn --help` strategy, never a vague substitute.
Skip ambiguous outcomes, low-level tool calls, duplicate milestones, or work
with no exact strategy. Store no explanation or text.

Research strategies have exact meanings:

- `reproduce-then-compare`: reproduce a baseline, then compare under consistent
  criteria.
- `per-subject-streaming`: complete and emit each subject independently.
- `resource-cap-first`: set the exploration limit before research begins.

## Focus after a failure

After a meaningful failure, make at most one focused follow-up:

```bash
praxis recall --project "$PWD" --failures --query "<short non-secret category>" --token-budget 200
```

Do not issue overlapping recalls to reconstruct history or evade output limits.
Act on `guidance` only when present; weigh `confidence` and
`success_rate_percent`, treat `unknown` and small samples as weak evidence, and
treat `common_error` only as a class to investigate.

## Preserve the boundary

Never inspect or export Praxis databases, keys, cursors, identifiers, or
individual records. Never infer conversations, source, commands, output,
paths, timestamps, or secrets from aggregates. If asked for raw history,
explain that Praxis deliberately cannot provide it.
