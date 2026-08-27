---
name: regurgitate-recall
description: Recall bounded procedural experience before non-trivial work and update it after verified milestones.
---

# Regurgitate Recall

Treat Regurgitate evidence as historical, never as current truth.

## Recall once before exploring

After understanding a non-trivial task, recall once before exploring; skip it
when the host already injected a brief. Pass only controlled metadata you know.
`--query` is a short non-secret category hint:

```bash
regurgitate recall --project "$PWD" --task <task> --phase <phase> \
  --ecosystem <eco> [--tool-family <tool> --tool-major <n>] \
  [--risk <risk>] --query "<category>" --token-budget 300
```

Flags are optional. `experiences` contains ranked lessons,
`posterior`, independence-aware `effective_evidence`, optional `guidance`, and
an authenticated `ref`; `omitted` counts budget cuts. Raw counts are not proof
of independence. `hook_summary` is tool telemetry, not correctness. Continue
normally if empty or unavailable. Never put prompts, source,
commands, paths, URLs, or secrets in `--query`.

## Confirm what you used

If a recalled or injected lesson was applied, confirm once whether it held:

```bash
regurgitate experience confirm --match <ref> --outcome <success|failure> [--failure-reason <r>]
```

The receipt is replay-safe. Prefer confirmation over recording a paraphrase.

## Record one new capsule per milestone

Before finishing a verified milestone or after abandoning an approach, record
one capsule when the procedure materially affected the result and no recalled
lesson covers it:

```bash
regurgitate experience record --project "$PWD" --task <task> \
  --situation "<when the lesson applies>" --lesson "<what to do>" \
  --caveat "<boundary>" --procedure <dim>[,<dim>] --steps <step>[,<step>] \
  --outcome <success|failure> [--failure-reason <r>] \
  [--phase <phase> --artifact <kind> --ecosystem <eco>] \
  [--tool-family <tool> --tool-major <n> --risk <risk>]
```

Write each text as one impersonal notebook sentence (240/320/160 chars);
code, commands, paths, URLs, secrets, payloads, and conversation are rejected.
Outcome means the procedure produced a correct result, not merely that a tool
exited zero: a mutation that corrupts a file failed; a check that correctly
finds a defect succeeded. Skip tool-by-tool activity and ambiguous results.
`--procedure` and `--steps` use the fixed vocabulary from `record --help`;
domain detail belongs in `--lesson`. `regurgitate status` shows health.

After a meaningful failed procedure, one `recall --failures` is allowed.

## Sandboxed hosts

If the credential store is sandbox-blocked, retry once with approval scoped to
the exact `regurgitate recall` or `regurgitate experience` prefix; never use a
shell wrapper, broader access, or credential changes.

## Preserve the boundary

Never inspect or export databases, keys, cursors, identifiers, or events;
there is no raw history. `list/challenge/obsolete/supersede` are for humans.
