---
name: regurgitate-recall
description: Recall prior experience for a task and record one bounded experience capsule after a verified milestone or rejected approach. Use before exploring a non-trivial task and before finishing verified work.
---

# Regurgitate Recall

Treat Regurgitate evidence as historical, never as current truth.

## Recall once before exploring

Recall once after understanding a non-trivial task, before exploring (skip
if the host already injected a brief). Pass controlled metadata when
confident; `--query` is a short non-secret category hint:

```bash
regurgitate recall --project "$PWD" --task <task> --phase <phase> \
  --ecosystem <eco> --query "<category>" --token-budget 300
```

Flags after `--project` are optional. `experiences` lists ranked lessons with
`posterior`, `effective_evidence`, `guidance` (absent when evidence is
limited), and a `ref`; `omitted` counts lessons cut by the budget.
`hook_summary` is tool telemetry, not correctness. Continue normally if empty
or unavailable. Never put prompts, source, commands, paths, URLs, or secrets
in `--query`.

## Confirm what you used

Lessons become trusted only through confirmation. If a recalled or injected
lesson was applied, report whether it held, by its `ref`:

```bash
regurgitate experience confirm --match <ref> --outcome <success|failure> [--failure-reason <r>]
```

Prefer this over re-recording a paraphrase.

## Record one new capsule per milestone

Before finishing a verified milestone or after abandoning an approach,
record one experience when a procedure materially affected the result and
no existing lesson covers it:

```bash
regurgitate experience record --project "$PWD" --task <task> \
  --situation "<when the lesson applies>" --lesson "<what to do>" \
  --caveat "<boundary>" --procedure <dim>[,<dim>] --steps <step>[,<step>] \
  --outcome <success|failure> [--failure-reason --phase --artifact --ecosystem]
```

Write each text as one impersonal notebook sentence (240/320/160 chars);
code, commands, paths, URLs, secrets, payloads, and conversation are
rejected. Outcome means the procedure produced a correct result, not that a
tool exited zero: a mutation that corrupts a file failed; a verification
that exposes a broken change succeeded. Skip tool-by-tool activity and
ambiguous results. `--procedure`/`--steps` take a fixed generic vocabulary
(`record --help`); domain specifics go in `--lesson`, never in new labels.
`regurgitate status` shows health; there are no other agent commands.

After a meaningful failed procedure, one `recall --failures` is allowed.

## Sandboxed hosts

If the credential store is sandbox-blocked, retry once with approval scoped
to the exact `regurgitate recall` or `regurgitate experience` prefix; never
a shell wrapper, broader access, or credential changes.

## Preserve the boundary

Never inspect or export databases, keys, cursors, identifiers, or events;
there is no raw history. `list/challenge/obsolete/supersede` are for humans.
