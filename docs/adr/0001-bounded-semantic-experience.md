# ADR 0001 — Retain bounded semantic experience, never raw work history

Status: accepted · Date: 2026-08-21 · Release: v0.8

## Context

Through v0.7 the only learned record was `LearnedPractice`: one `TaskKind`,
one `Strategy`, one `success`/`failure`. Recall aggregated those into counts
and emitted `prefer`/`avoid` after two observations. That design has a clean
privacy story (no natural language is ever stored) but it cannot express *why*
an approach worked, so the lesson rarely transfers: "targeted verification
usually worked" is not "targeted verification worked because parser
acceptance was weaker than the native tool's check."

## Decision

Regurgitate learns **experience capsules** instead of strategy tallies.

A capsule is a deliberately authored, bounded summary: a `situation`
(≤ 240 chars), a `lesson` (≤ 320), an optional `caveat` (≤ 160), a
compositional `Procedure`, a semantic outcome, controlled applicability tags,
a minimal environment fingerprint, a lifecycle state, and a bounded evidence
log. It is encrypted before it reaches SQLite; the only plaintext is an
opaque id, an HMAC scope token, timestamps, and version numbers.

The product invariant becomes:

> Retain bounded semantic experience. Never retain raw work history.

Concretely, the following remain outside the vault: prompts, responses,
reasoning traces, source, commands, arguments, tool output, URLs, paths,
environment values, provider payloads. Capsule text is admitted only after
structural checks that reject material resembling any of those; false
negatives are preferred to storing arbitrary text.

## What does not change

- Encrypted local persistence, OS credential-store key, project identity
  indirection, idempotent hook recording.
- `HookExecution` telemetry is untouched and never becomes semantic evidence.
- Recall stays read-only and bounded by a hard token budget.
- Preview-first forgetting and retention; both now also cover capsules.
- No event-level export surface.

## Consequences

- `Strategy` becomes a compatibility input mapped onto `Procedure`.
- Guidance is uncertainty-aware: Beta posterior, credible interval, and Kish
  effective sample size replace count buckets. Two successes no longer yield
  `prefer`.
- Scope becomes a relevance prior (project → workspace → ecosystem → machine
  → global); encryption remains the access boundary.
- A new threat, semantic memory poisoning, is mitigated by semantic-outcome
  verification, minimum evidence, `challenged` state, recency decay, bounded
  scope, explicit supersession, and the existing forget path.
- The release is gated on a paired cold/warm benchmark, not on the memory
  looking richer.
