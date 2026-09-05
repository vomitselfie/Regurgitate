---
name: regurgitate-recall
description: Selectively recall procedural experience, confirm applied lessons, and record only novel verified lessons.
---

# Regurgitate Recall

Treat recalled lessons as historical evidence, never current truth. Memory must
improve the primary task without becoming a second task.

## Recall selectively

Recall once only when prior experience could materially change non-trivial
implementation, diagnosis, migration, deployment, or research. Skip simple
questions, mechanical edits, formatting, and tasks with an injected brief.

Context is inferred. Recall includes applicable shared lessons even in a new
project. Use flags only to correct inference:

```bash
regurgitate recall --query "<short non-secret category>" \
  --best-effort --token-budget 240
```

`matches` contains ranked lessons, evidence, optional `guidance`, and a `ref`.
`no_matches` and `unavailable` are terminal: continue without another recall or
troubleshooting. Never put prompts, source, commands, paths, URLs, identifiers,
or secrets in `--query`.

## Confirm only what was applied

If a recalled or injected lesson materially influenced the work, report once
whether it held:

```bash
regurgitate experience confirm --match <ref> \
  --outcome <success|failure> [--failure-reason <reason>]
```

Confirmation is replay-safe and preferable to a paraphrase. Failure never
blocks the task.

## Record rarely

After verified work, record at most one novel, reusable, specific lesson not
covered by recall:

```bash
regurgitate experience record --task <task> \
  --situation "<when it applies>" --lesson "<what to do>" \
  --procedure <dimension> --outcome <success|failure>
```

Add `--shared` for portable tool, verification, or host lessons; omit it for
project-specific behavior. Agents under the same OS account and data home share
this notebook. Never widen old lessons merely to fill it.

One procedure dimension suffices. Add `--caveat`, `--steps`, or `--failure-reason`
only when useful. Correct inferred tags if the lesson concerns a different tool
or ecosystem. Never invent verification or outcomes.

Text is impersonal (240/320/160 characters). Commands, paths, URLs, secrets,
payloads, and conversation are rejected. Success means a correct result.
`duplicate` and `rejected` are terminal; do not rewrite to force acceptance.
Skip routine activity, ambiguous results, and generic advice. Vocabulary is
available from `record --help`.

## Sandboxed hosts

If the credential store is sandbox-blocked, retry once with approval scoped to
the exact `regurgitate recall` or `regurgitate experience` prefix. Never use a
shell wrapper, broaden access, alter credentials, or block the primary task.

## Preserve the boundary

Never inspect or export databases, keys, cursors, identifiers, or events.
`status`, `metrics`, and lifecycle commands are for humans. There is no raw
history or agent messaging surface.
