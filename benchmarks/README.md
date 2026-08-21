# Paired cold/warm benchmark

The v0.8 release gate is measured, not eyeballed. Run the same task suite
twice per task—once with an empty Regurgitate history (**cold**) and once
with history seeded by an earlier session on the same project (**warm**)—and
feed the paired numbers to `regurgitate bench-report`.

## Protocol

1. Pick a representative suite. Include:
   - recurring project-specific traps where memory should clearly help;
   - unrelated tasks where recall should stay quiet;
   - changed-version tasks where stale memory should be down-weighted;
   - adversarial privacy fixtures (secrets, paths, URLs, command output,
     source text) to verify nothing enters semantic storage;
   - tasks sharing a `TaskKind` but with opposite best procedures, to detect
     over-aggregation.
2. Use exact fixtures and expected outputs so correctness is measurable.
3. Randomize task order. Repeat each task enough times to separate "the
   agent happened to solve it faster" from consistent benefit.
4. For each paired run, append one JSON object to a `.jsonl` file:

```json
{"task":"placement-drc","memory_relevant":true,
 "cold":{"total_tokens":9000,"tool_calls":30,"failed_actions":3,"seconds":120,"correctness":1.0},
 "warm":{"total_tokens":6000,"recall_tokens":200,"tool_calls":18,"failed_actions":1,"seconds":80,"correctness":1.0}}
```

| Field | Meaning |
| --- | --- |
| `total_tokens` | `T_total`: all agent tokens for the task |
| `recall_tokens` | `T_recall`: tokens contributed by Regurgitate recall plus its activation instruction (warm only) |
| `tool_calls` | `N_tool` |
| `failed_actions` | `N_failed`: rejected approaches / failed substantive actions |
| `seconds` | `τ`: wall-clock completion time |
| `correctness` | `Q` in `[0, 1]`: test score or artifact acceptance |
| `memory_relevant` | whether prior sessions should plausibly help this task |

5. Summarize:

```bash
regurgitate bench-report --runs runs.jsonl
```

## Metrics

- Token savings = `T_total,cold − T_total,warm − T_recall`
- Token ROI = savings / `max(T_recall, 1)` (net tokens avoided per recall token)
- Retry reduction = `(N_failed,cold − N_failed,warm) / max(N_failed,cold, 1)`
- Correctness delta = `Q_warm − Q_cold`
- Overhead on irrelevant tasks = mean `T_recall` where `memory_relevant` is false

## Release gate

`bench-report` reports `gate.passed` only when all of the following hold:

- no correctness regression (mean delta ≥ 0);
- positive median token savings on memory-relevant tasks;
- failed-action count not higher warm than cold;
- mean recall overhead on irrelevant tasks ≤ 40 tokens.

Privacy regression is covered separately by `cargo test` (adversarial
admission fixtures, encryption-boundary byte checks, project isolation).

## Baseline

`baseline-v0.7.jsonl` is the place to commit the numbers measured against
the last release before expanding adapters. It is not yet populated: the
harness and gate exist, the paired runs still need to be executed.
