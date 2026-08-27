# ADR 0002 — Weight independent evidence, not repeated assertions

Status: accepted · Date: 2026-08-27 · Release: v0.9

## Context

Schema v3 stored outcome and time per evidence entry but kept environment on
the capsule. Ranking therefore treated repeated, potentially correlated agent
confirmations as separate trials. A challenged capsule could also reactivate
from evidence that predated the challenge, and a short capsule selector could
be confirmed repeatedly after one recall.

## Decision

Schema v4 keeps the capsule as a semantic claim and records controlled
provenance, environment, verification, attestation, optional cohort, and an
optional confirmation-receipt digest on every evidence entry. Agent evidence
is self-reported even when its controlled procedure says targeted, full, or
native verification; only trusted host or human entry points may create
stronger attestations.

Recall groups explicit cohorts together. Evidence without a cohort is grouped
conservatively by UTC day and controlled source, agent family, and environment.
A cohort contributes at most its strongest observation. Effective evidence is
the lesser of cohort-level Kish sample size and absolute weighted evidence
mass, so many stale, incompatible, or self-reported observations cannot earn a
strong label through count alone. Raw successes and failures remain visible.

Agent-facing `ref` values are stateless authenticated receipts derived from the
local master key. Their encrypted digest makes one receipt idempotent; issuing
a receipt does not write to storage. Human maintenance retains short selectors.

A challenge records its start and supporting outcome. Only distinct,
supporting evidence after that point advances recovery; opposing evidence
resets progress. Broader-scope capsules require both situation and lesson.

Storage reads v3 and v4, upgrades v3 in memory, and writes only v4. A read never
rewrites a row; the next actual mutation performs the lazy migration in place.
No new plaintext database column is introduced.

## Consequences

- Repetition no longer implies independence or unsolicited preflight trust.
- Legacy evidence remains usable but is conservatively unattributed.
- Risk shapes and tool major versions participate in recall, and risky recalls
  reserve an applicable failure before result trimming.
- Receipt replay is harmless, but a fresh recall may issue a new receipt; the
  cohort policy remains the independence boundary.
- Active task ownership, leases, and agent messaging remain out of scope and
  belong in a separate ephemeral coordination subsystem.
