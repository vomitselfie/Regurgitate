---
name: praxis-recall
description: Retrieve bounded, privacy-preserving aggregate procedural history for the current project through the Praxis CLI. Use after the user's task is understood, before repeating expensive exploration, after a meaningful failed approach, or before declaring an approach blocked when prior local success and failure patterns could guide the next step. Works through the provider-neutral CLI for Codex, Claude, Hermes, and other coding agents; do not use it to recover conversations, source, commands, tool output, paths, or secrets.
---

# Praxis Recall

Use Praxis observations as advisory evidence, never as current truth.

## Decide whether to recall

Do not invoke Praxis when the request is to recover exact conversations,
commands, output, paths, timestamps, source content, or secrets. Explain that
Praxis deliberately cannot provide raw history. Continue to the recall workflow
only when a bounded procedural pattern could help with the current task.

## Recall a task pattern

1. Wait until the user's task is understood. Do not recall unconditionally at
   session startup or for casual conversation.
2. Run from the project root:

   ```bash
   praxis recall --project "$PWD" --query "<short non-secret task category>" --token-budget 600
   ```

   Paraphrase the task as a few generic terms such as `test failure` or
   `dependency update`. Never copy credentials, private values, source text, or
   the user's full prompt into `--query`; command-line text may enter shell or
   agent logs even though Praxis does not persist it.
3. Continue normally when Praxis is unavailable, returns an error, or returns
   no observations. Do not install software, alter credentials, or weaken
   privacy settings merely to obtain recall.
4. Compare relevant observations with the current repository, tool versions,
   and constraints before choosing an approach.
5. Prefer a repeatedly successful strategy and avoid a repeatedly failed one
   only when the current evidence confirms that the old pattern still applies.

## Record a verified practice

After a meaningful approach has a directly verified result, record at most one
controlled practice outcome:

```bash
praxis learn --project "$PWD" --strategy <controlled-strategy> --outcome <success|failure>
```

Use this only when all of the following are true:

- the strategy exactly matches one of the values accepted by `praxis learn
  --help`; do not fall back to a vague label;
- a test, validation command, explicit provider result, or user confirmation
  directly established success or failure;
- the observation represents a meaningful approach or milestone, not an
  individual low-level tool call; and
- the same milestone has not already been recorded.

For research and analysis work, use these strategies only with their exact
meanings:

- `reproduce-then-compare`: establish a baseline or reproduce a claim, then
  compare alternatives under consistent criteria;
- `per-subject-streaming`: complete and emit each subject independently rather
  than accumulating all subjects into one batch; and
- `resource-cap-first`: set the time, token, request, source, or similar
  exploration limit before research begins.

Skip learning when the outcome is ambiguous, when Praxis is unavailable, or
when no controlled strategy fits. Never translate commands, paths, source
content, errors, prompts, or user text into a strategy. `learn` accepts only a
fixed vocabulary and stores no note or explanation.

## Focus after a failure

After a meaningful local attempt fails, make at most one focused follow-up:

```bash
praxis recall --project "$PWD" --failures --query "<short non-secret category>" --token-budget 400
```

Use `--operation` only when an exact controlled operation such as `command`,
`apply-patch`, `search`, or `web-request` is relevant. Do not issue repeated or
overlapping recalls to reconstruct more history or work around output limits.

## Interpret the aggregate

- Treat multiple known outcomes as stronger evidence than one attempt.
- Treat `unknown` outcomes and small samples as weak evidence.
- Treat `guidance` as actionable only when it is present. Praxis omits guidance
  until at least two known outcomes exist.
- Use `confidence` and `success_rate_percent` to distinguish weak repetition
  from a durable local pattern.
- Treat `common_error` as a class to investigate, not a recovered error message.
- Validate every suggested strategy against current state before acting or
  reporting it to the user.

## Preserve the boundary

Read only the fixed aggregate returned by `praxis recall`. Never inspect or
export the Praxis database, keys, cursors, project identifiers, or individual
records. Never attempt to infer prompts, commands, output, paths, timestamps,
or source content from the aggregate.
