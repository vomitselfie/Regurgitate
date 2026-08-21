---
name: regurgitate-recall
description: Recall prior experience for a task and record one bounded experience capsule after a verified milestone or rejected approach. Use before exploring a non-trivial task and before finishing verified work.
---

# Regurgitate Recall

Treat Regurgitate evidence as historical, never as current truth.

## Recall once before exploring

Recall once after understanding a non-trivial task, before exploring, unless
a recent recall for the same intent was empty. Pass controlled metadata when
confident; `--query` is a short non-secret category hint:

```bash
regurgitate recall --project "$PWD" --task <task> --phase <phase> \
  --ecosystem <eco> --query "<category>" --token-budget 300
```

Flags after `--project` are optional. `experiences` lists ranked lessons with
`posterior`, `interval`, `effective_evidence`, and `guidance`
(`prefer`/`avoid`/`mixed`; absent when evidence is limited). `legacy: true`
is an old aggregate. `hook_summary` is tool telemetry, not correctness.
Continue normally if empty or unavailable. Never put prompts, source,
commands, paths, URLs, or secrets in `--query`.

## Record one capsule per milestone

Before finishing a verified milestone or after abandoning an approach,
record one experience when a procedure materially affected the result:

```bash
regurgitate experience record --project "$PWD" --task <task> \
  --situation "<when the lesson applies>" --lesson "<what to do>" \
  --caveat "<boundary>" --procedure <dim>[,<dim>] --steps <step>[,<step>] \
  --outcome <success|failure> [--failure-reason <r>] [--phase <p>] \
  [--artifact <a>] [--ecosystem <e>] [--tool-family <t>]
```

Write each text as one impersonal notebook sentence (240/320/160 chars);
code, commands, paths, URLs, secrets, payloads, and conversation are
rejected. Outcome means the procedure produced a correct result, not that a
tool exited zero: a mutation that corrupts a file failed; a verification
that exposes a broken change succeeded. An equivalent capsule is confirmed,
not duplicated; that is the common path.
Skip tool-by-tool activity, ambiguous results, and work with no exact
procedure. `--procedure` and `--steps` take a fixed generic vocabulary
(`record --help` lists it); domain specifics belong in `--lesson`, never in
new labels. `regurgitate learn` is a text-free shorthand.

After a meaningful failed procedure, one focused recall is allowed:
`regurgitate recall --project "$PWD" --failures --task <task> --token-budget 200`

## Sandboxed hosts

If the credential store is sandbox-blocked, retry once with approval scoped
to the exact `regurgitate recall` or `regurgitate experience record` prefix;
never a shell wrapper, broader access, or credential changes.

## Preserve the boundary

Never inspect or export databases, keys, cursors, identifiers, or events.
Queries are ephemeral; capsule text is bounded and encrypted; there is no
raw history. `experience list/challenge/obsolete/supersede` are for humans.
