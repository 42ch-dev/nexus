---
module: local-api, daemon-runtime, web-ui
date: 2026-07-01
problem_type: knowledge
category: architecture-patterns
severity: medium
plan_id: 2026-07-01-v1.80-memory-review-reliability
tags: [drain-loop, has-more, bounded-processing, partial-progress, deadline, in-flight-serialization, local-api, reliability, nexus-contracts]
applies_when: designing a bounded/serialized/deadline-aware synchronous processing endpoint whose client must drain a queue via repeated calls
---

# Bounded drain-completion contract — `has_more` must reflect queue advancement, not rows attempted

## Context

Nexus has local-only endpoints that **process** a queue rather than merely **list** rows — e.g. `POST /v1/local/memory/review` classifies each pending-review row and promotes/fragments/drops it (deleting the row on success). Under the local-only / single-active-creator / small-queue threat model, the V1.80 REL-01 rewrite converted this from an unbounded synchronous whole-queue loop into a **bounded** (N rows/call), **per-creator serialized** (in-process mutex map on `WorkspaceState`), **deadline-aware** (5s partial-progress) pipeline that keeps its synchronous request/response shape (no async job infrastructure).

The client (`useReviewMemory`) **drains** by repeating the call while `has_more === true`, up to a max-iterations cap, with a zero-progress guard.

This is the **drain pattern** — distinct from cursor pagination (read-side) or background jobs (async-side). It has its own completion-contract trap.

## Guidance

### The invariant (the part that is easy to get wrong)

> **`has_more` (drain-completion) must reflect whether pending rows REMAIN, i.e. queue *advancement*, NOT how many rows were *attempted/inspected*.**

"Advancement" = a row was actually completed (promoted/fragmented/dropped → deleted from the pending queue). "Attempted" = the loop looked at the row, regardless of whether its action succeeded, timed out, or failed.

If `processed` counts attempts and `has_more` is derived from `processed < batch_size`, then a row that was **attempted but not completed** (timeout mid-action; action returned zero counts; unimplemented action) leaves the row pending while `processed` advances. When that row is the only or final row in the fetched slice, `processed == batch_size` and `more_in_db == false` → `has_more = false` → the client terminates with a **false "Review complete"** while a pending row remains. The client's `processed === 0 && has_more === true` zero-progress guard cannot detect this because `processed ≥ 1` for the failed attempt.

### How to derive `has_more` correctly

Track a separate flag — e.g. `any_row_remained_pending` — set in **every** non-completion arm of the processing loop:

- `Ok(action_counts)` where `promoted + fragmented + dropped == 0` (side-effect failure or unimplemented action → row NOT deleted).
- `Err(_elapsed)` from a row-level `timeout_at` (deadline hit mid-action → row NOT deleted).

Then:

```
has_more = more_in_db                 // over-fetch row existed (batch was full)
       || deadline_stopped            // loop exited before consuming the full slice
       || any_row_remained_pending    // a fetched row was attempted but NOT completed
```

Contract comment to leave in the handler: *"`has_more = true` means the queue may not be fully drained; the client should re-request."*

Keep the public `processed` field as "rows attempted/inspected" if clients use it for progress UX — but do **not** derive drain completion from it. The two concerns (progress display vs. completion signal) are separate.

### Alternative: authoritative EXISTS check

A `SELECT EXISTS(SELECT 1 FROM <pending> WHERE creator_id = ?)` after the batch is unambiguously correct (`has_more = pending_exists`) at the cost of one extra query per call. Prefer the in-process flag when the completion information is already available from the loop; prefer the EXISTS check when the loop's view of "did this row advance?" is unreliable.

## Why This Matters

This trap directly defeated the REL-01 reliability goal. `R-V178P0-QC3-003` (V1.78 QC3) named three axes for the rewrite: chunked processing, per-creator in-flight serialization, and **client uncertain-completion handling**. The first two were straightforward; the third was implemented as a drain loop but shipped with the attempt-vs-advancement accounting bug (V1.80 qc3 W-QC3-001). The bug was only caught by the performance/reliability reviewer tracing the `has_more` derivation end-to-end — the bounded-drain-walk test passed because all rows in the walk succeeded (the failure path was untested).

The lesson: **a drain loop that "terminates" is not the same as a drain loop that "drains."** Completion-correctness must be tested against the *failure* paths (timeout, action-failure, perpetually-failing head row), not just the happy path.

## When to Apply

- Any endpoint that processes a queue in bounded batches and signals the client to re-request via `has_more` / a drain flag.
- Any place a "processed N rows" counter feeds a "are we done?" signal — audit whether `processed` means *attempted* or *advanced*.
- Whenever adding a deadline/timeout to a processing loop: the partial-progress return must distinguish "I stopped early" from "I finished the batch but nothing actually drained."

This is **not** needed for pure read-side cursor pagination (there, `has_more` = "another page exists," which the over-fetch row answers directly — see `pagination-cursor-without-total-count-labels.md`).

## Examples

### The V1.80 bug (anti-pattern)

```rust
// WRONG — processed counts attempts; has_more derived from it
for row in slice {
    let outcome = timeout_at(deadline, process(row)).await;
    batch.processed += 1;          // <-- counted BEFORE checking outcome
    match outcome {
        Ok(c) if c.advanced() => { delete_pending(row); }
        Ok(_) => { /* action failed; row stays pending */ }
        Err(_) => { /* timeout; row stays pending */ }
    }
}
let has_more = more_in_db || batch.processed < slice.len();
// If the only row timed out: processed=1, slice.len()=1, more_in_db=false
// -> has_more = false  BUT the row is still pending. Client falsely stops.
```

### The V1.80 fix

```rust
// CORRECT — track advancement separately from attempts
let mut any_remained_pending = false;
for row in slice {
    batch.processed += 1;          // still "rows attempted" for progress UX
    match timeout_at(deadline, process(row)).await {
        Ok(c) if c.advanced() => { delete_pending(row); }
        Ok(_) | Err(_) => { any_remained_pending = true; }  // row NOT advanced
    }
}
let has_more = more_in_db || batch.processed < slice.len() || any_remained_pending;
```

### Regression tests that catch it (the V1.80 fix-wave)

1. **One-row timeout/failure**: seed exactly 1 pending row whose action fails → assert `has_more == true` + row remains.
2. **Final-row failure**: a batch where the last-processed row fails (others complete) → assert `has_more == true` + failed row remains + completed rows gone.
3. **Perpetually-failing head row**: an unprocessable row across repeated calls → assert `has_more == true` every call, row never drains, client hits cap with "still draining" (not a false "complete").

All three must **FAIL** on the anti-pattern code and **PASS** on the fix — verify by temporarily reverting the production fix and re-running.

## See also

- [`pagination-cursor-without-total-count-labels.md`](pagination-cursor-without-total-count-labels.md) — the read-side `has_more` cousin (cursor pagination count labels; no `total`). Related but distinct: that one is about *displaying* a count over a list; this one is about *draining* a processing queue.
- `crates/nexus-daemon-runtime/src/api/handlers/memory.rs` — `process_review_batch` + the `any_row_remained_pending` flag (V1.80 REL-01 reference implementation).
- `apps/web/src/api/queries.ts` `useReviewMemory` — the client drain loop + max-iterations cap + zero-progress guard.
