---
title: Cross-store journal protocol for provider-owned SQL+file effects
category: architecture-patterns
track: knowledge
source_iteration: v1.227-spoke-memory-n1
created: 2026-11-02
last_updated: 2026-11-02
status: active
problem_type: architecture
module: crates/nexus-core/src/memory_provider
date: 2026-11-02
severity: medium
applies_when:
  - A provider must keep SQL rows and derived markdown/file projections consistent when neither a single transaction nor a single filesystem op can cover both
  - Multiple provider processes (or instances) may target the same workspace concurrently
  - Cancellation, crash-between-phases, and operator-resolved conflicts must never lose committed data or strand receipts in a non-terminal state
---

# Cross-store journal protocol for provider-owned SQL+file effects

Landed in `crates/nexus-core/src/memory_provider/{journal.rs,storage.rs}` over seven review-fix rounds (v1.227). This is the implementation-level pattern; the protocol-level contract lives in `.agents/adr/implemented/architecture/2026-10-28-spoke-memory-adoption-plan.md` §5.2 (D-10).

## Invariants (the ones every fix round was fighting for)

1. **Journal-first, one transaction.** The intent record (targets, digests, receipt id) commits in the SAME guarded SQL transaction as the effect rows and the receipt — that commit is the point of no return. A crash before it leaves nothing visible; the idempotency key stays free.
2. **Publish after commit, settle after publish.** Staging → atomic rename → applied-mark happen after the SQL commit, each file effect inside a section that owns its locks. A crash anywhere leaves a recoverable state, never a half-published one claimed as clean.
3. **Receipts go terminal only when the file work is done.** Persist the receipt in the journal transaction but report/serve `committed` only at settlement. A pending-window replay or `receipts.get` answers `OUTCOME_UNCERTAIN`, never a premature terminal.
4. **Settlement scrubs content (MEM-R014).** The applied state, the payload scrub, and the receipt terminalization are one transaction. Quarantine is the exception: it KEEPS the payload (it is the recovery evidence) and issues a terminal `failed` receipt; a later re-drive replaces `failed` with `committed`.
5. **Ownership before bytes.** Every install verifies displaced bytes against the intent's own digests (absent / already-new / exactly-prior) — never against "whatever is at the path". Foreign bytes are preserved (rename-back no-clobber, or quarantine), never overwritten or silently deleted.

## Concurrency shape (the part reviews kept reopening)

- **Lock order is fixed: gate → flock → transaction.** In-process async mutex first (same-process publishers serialize without burning blocking threads), then a cross-process `flock(LOCK_EX)` on `<files_root>/.memory-publish.lock` (created once, never unlinked — unlinking would let the next holder lock a different inode), then the SQL transaction. No DB lock is held while acquiring either.
- **Cancellation safety comes from ownership placement, not flags.** The flock lease and the gate guard are moved BY VALUE into the `spawn_blocking` closure that runs the file section. A cancelled caller drops only its JoinHandle; the section runs to completion and releases the locks itself (point-of-no-return contract: a committed section finishes even if its caller is gone). A cancelled flock-wait also keeps the gate held by its wait task — otherwise repeated cancellations stack waiters until the blocking pool the holder needs is exhausted.
- **File effects use sync `std::fs` inside the blocking section.** `tokio::fs` work can outlive a dropped future; sync ops inside the section cannot escape the lease.
- **Advisory-lock boundary, stated honestly:** the flock coordinates provider processes only. A non-provider process writing under the file root bypasses both the lock and the ownership rule; the ownership checks keep the bytes they can see and refuse what they cannot.

## Recovery state machine

`pending → (deferred | quarantined | applied)`:

- **Bounded pass** (64/pass) with a durable per-space rotation cursor so persistent conflicts cannot starve resolved tails. Overflow marks entries `deferred` (terminal reads on their subjects), not silent-pending.
- **Quarantine** = target holds foreign bytes after ownership verification: keep payload, terminal `failed` receipt, subject reads a typed terminal `INTERNAL_ERROR`. An operator who fixes the target gets the intent re-driven on a later pass (`quarantined → applied`).
- **Terminal-first reads:** `unsettled` selection orders quarantined/deferred BEFORE pending, so a new pending intent can never mask an existing terminal diagnosis on the same subject.
- **Predecessor settlement:** a later op on the same target settles (or is refused by) earlier pending intents for that target INSIDE its own admission transaction, after authorization and dedup probe — never before them (pre-authority settlement leaks other ops' receipt identities to unauthorized callers).

## Traps that cost fix rounds (checklist for the next implementation)

- Clock sampled before `pool.begin()` (grant expiring during pool wait still authorizes) — sample after the connection is held, on reads AND writes.
- Check-then-remove/delete on paths — replace with rename-first displacement, then verify what the rename carried.
- `tokio::fs` in a section whose lock is owned by the caller's future — cancellation drops the future, the work outlives the lock.
- Swallowed cleanup errors after a successful link — a failed scratch removal must keep the intent recoverable, or deleted content survives on disk under an applied mark.
- docs claiming more than the mechanism proves (restart/backward-clock windows) — qualify to the exact boundary; a test that cannot honestly assert the guarantee should be `#[ignore]`-documented instead.
