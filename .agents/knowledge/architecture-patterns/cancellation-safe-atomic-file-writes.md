---
title: Cancellation-safe atomic file writes behind a shared sync kernel
category: architecture-patterns
track: knowledge
last_updated: 2026-11-22
created: 2026-11-22
date: 2026-11-22
status: active
problem_type: architecture_pattern
module: nexus-core / nexus-local-db
severity: medium
symptoms:
  - "a test proves cancellation leaves the target untouched but passes on a design that renames anyway"
  - "moving an async atomic write onto spawn_blocking quietly removes the caller's ability to stop it"
  - "an advisory lock held by the cancelled caller protects the write only until its TTL, not until the write finishes"
root_cause: "Cancellation in Rust futures only stops work at await points. A blocking task outlives the future that spawned it, so any state transition it performs after the drop happens without a caller who can report or abort it."
resolution_type: workflow_improvement
---

# Cancellation-safe atomic file writes behind a shared sync kernel

## Context

`nexus-core` had three atomic-write implementations. Consolidating them behind one synchronous kernel meant routing the async `content::fsync_write_atomic` through `spawn_blocking`. That looked like a pure refactor; it was not.

The wrapper used to perform write → fsync → rename itself, so a dropped future stopped at an await point before the rename. With the whole sequence in a blocking task, dropping the future no longer stops it: the task finishes and renames the temp file into place. A caller that believed it had cancelled could still have its bytes become the target.

## Guidance

**Split the kernel by cancellation point, not by code shape.** Keep one kernel with two exported stages:

- `prepare_temp(tmp, content)` — write the caller's temp file and fsync it. This is the long, cancellable part; run it on the blocking pool.
- `commit(target, tmp, fsync_final)` — rename, optional final-file fsync, parent-directory fsync. Run it from the still-live caller, via a blocking task only so it does not stall a runtime worker.

The invariant that matters: **no `?` and no state transition between the temp file existing and the commit decision**. Once the caller's await returns, it commits; if it was cancelled, nothing is committed. Keep a `prepare + commit` convenience function for synchronous callers that have no cancellation point at all — do not force them into two calls.

**Do not justify the old boundary with a lock.** An advisory lock with a TTL measured from acquisition does not cover a task that outlives its caller: the tail only has to cross the takeover point, not the whole TTL. If the lock has no `Drop` release (an explicit `release(self)` only), a cancelled caller keeps it until expiry and the window is real.

**Proving it in a test.** Asserting "the temp file has new bytes" is not proof that the task exited — a regression can rename microseconds later and the test never sees it. Use the blocking pool itself as the ordering device: with `max_blocking_threads(1)`, queued tasks dispatch in submission order, so park the single slot, cancel the write, release the slot, then await a no-op sentinel `spawn_blocking`. When the sentinel resolves the cancelled task has left the pool, and only then assert the target still holds its old bytes. State in the comment that FIFO is an implementation property of the current runtime, not a public guarantee.

**Document what a panic means.** If the commit task panics after `rename`, the caller gets a join error while the new target is already visible. That is not a rollback, and the doc comment should say so rather than implying all-or-nothing.

## Why This Matters

The failure is silent and rare: it needs a cancellation during a write, a later writer, and a lock whose lease ends between them. No test fails, no log line appears, and the last writer wins by mistake. Splitting on the cancellation point costs one extra function boundary and removes the whole class.

## When to Apply

- Consolidating async and sync file-writing helpers into one kernel.
- Moving any awaited operation into `spawn_blocking` where the tail performs an irreversible state transition.
- Writing a cancellation test for anything whose side effect is a filesystem or external-state change.
