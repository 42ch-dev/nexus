---
module: nexus-core memory provider + nexus-memory-conformance fixture
date: 2026-11-10
problem_type: convention
category: conventions
severity: medium
plan_id: v230-n4r-local-reflection
tags:
  - reflection
  - memory-provider
  - receipts
  - staleness
  - replay
  - subject-fencing
applies_when:
  - "Adding a sub-capability family (reflection, transfer) to a memory provider"
  - "Touching pinned-basis, staleness, receipt or replay semantics"
  - "Authoring a second provider that must pass the same conformance selectors"
---

# Sub-capability families in a memory provider

## Context

SPOKE memory gates reflection and transfer behind sub-capabilities that a provider advertises only when it genuinely serves the whole family. Nexus ships two implementations: `LocalMemoryProvider` inside `nexus-core`, and the independently authored durable `FixtureMemoryProvider` in `nexus-memory-conformance`. Both now serve `spoke-memory-reflection` (`memory.reflections.state|list|generate`) and must agree on behavior without sharing storage, policy or business code.

`spoke-memory-transfer` remains deliberately unadvertised; the same rules apply when it lands.

## Guidance

**One switch drives advertisement and behavior.** A single construction-time flag feeds both `memory.describe` and the operation guards, so the provider can never advertise a family it refuses or refuse one it advertises. A deliberately unadvertised constructor exists only to exercise the refusal rows (C-003/C-073) and is test-hook gated, compiled out of production.

**Pinned basis, computed digest, per-row verdicts.** A generated envelope pins the source-record `(record_id, revision)` pairs and the profile revisions it was derived from. The state digest is composed at read time over the current authorized basis; nothing is eagerly invalidated. Staleness is recomputed for each row against **that row's own** pinned basis. A per-pin cache may hold the current live/tombstoned record sets, never a stale/excluded verdict computed against another reflection's pin — sharing a verdict silently marks an older envelope fresh.

**Tombstone exclusion counts back one revision.** Deletion is a terminal, single-revision bump, so the exclusion probe maps each tombstoned row back to its pre-tombstone `(record_id, revision - 1)` and compares that against the pin. Edits are not tombstones: an edited source breaks the count-back, so the envelope stays listed and honestly stale. A pin that mixes a tombstoned source with an added record remains listed and stale; strict exclusion for that mixed shape needs a membership table and is a separate decision.

**Receipts order: replay before freeze.** Transaction 1 resolves an authorized same-key replay, then applies the freeze/subject gates only to fresh work, then publishes the pending receipt. Transaction 2 writes exactly one terminal outcome — committed (row plus receipt carrying `reflection_id` and cost) or failed (retryable error, no cost). A committed generation whose response was lost must be recoverable with the identical key even while the space is frozen; a fresh key while frozen still answers `SPACE_FROZEN`.

**Subject fencing gates fresh work only.** A fenced subject rejects fresh generation with the protocol's fence refusal, while reads stay served and an already-admitted generation completes or replays above the gate.

**The request provenance filter belongs in the payload query.** A filtered `list`/`state` must restrict the returned envelopes and the `last_reflection_id` pick, not just the cursor binding.

**Provider independence is a test requirement.** The fixture is authored from the published protocol and its own schema; sharing local policy or storage would invalidate the cross-provider proof. Both providers exercise the same named selectors, which is the conformance evidence.

## Why This Matters

Each rule above was a real defect found by plan QC on the first integrated HEAD: a verdict cache keyed only by selection/filter, a tuple producer whose consumers read the receipt id as the journal state (changing established N3 read semantics), freeze evaluated before replay (unrecoverable lost responses), a payload query ignoring the request filter, and a fresh fixture generation bypassing subject fencing. They are cheap to get right when the ordering is written down and expensive to find later, because every one of them returns plausible output.

## When to Apply

Any change to a sub-capability family, pinned-basis computation, staleness reporting, receipt ordering, or a second provider that must pass the same conformance selectors. The matrix and row completion definitions live in `.mstar/specs/active/architecture/spoke-memory-conformance-staging.md`; provider binding semantics in `.mstar/specs/active/architecture/spoke-memory-provider-binding.md`.

## Examples

- Advertisement: `describe` and the three operation guards read the same flag; an unadvertised build serves the operations but omits the capability, so Connect negotiation answers `capability_missing`.
- Two generations under one selection: generate R1 (basis A), add a record, generate R2 (basis A+record). Listing reports R2 fresh and R1 stale; after the record is tombstoned, R2 is excluded and R1 is listed fresh again.
- Lost response: generate commits, the space freezes, the identical key replays the stored receipt with no second envelope; a different key answers `SPACE_FROZEN`.
