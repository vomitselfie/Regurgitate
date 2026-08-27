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

Project and controlled context are inferred. Use explicit flags only to correct
an inference:

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

After a meaningful verified milestone or rejected approach, record at most one
capsule only when the lesson is novel, reusable, specific, evidence-backed, and
not covered by recalled experience:

```bash
regurgitate experience record --task <task> \
  --situation "<when it applies>" --lesson "<what to do>" \
  --caveat "<boundary>" --procedure <dimension>[,<dimension>] \
  --steps <step>[,<step>] --outcome <success|failure> \
  [--failure-reason <reason>]
```

Text is one impersonal notebook sentence (240/320/160 characters). Commands,
paths, URLs, secrets, payloads, and conversation are rejected. Outcome means
the procedure produced a correct result, not that a tool exited zero.
`duplicate` and `rejected` are terminal: do not rewrite or retry merely to make
a capsule exist. Skip routine activity, ambiguous results, and generic advice.
The fixed procedure and step vocabulary is available from `record --help`.

## Sandboxed hosts

If the credential store is sandbox-blocked, retry once with approval scoped to
the exact `regurgitate recall` or `regurgitate experience` prefix. Never use a
shell wrapper, broaden access, alter credentials, or block the primary task.

## Preserve the boundary

Never inspect or export databases, keys, cursors, identifiers, or events.
`status`, `metrics`, and lifecycle commands are for humans. There is no raw
history or agent messaging surface.
