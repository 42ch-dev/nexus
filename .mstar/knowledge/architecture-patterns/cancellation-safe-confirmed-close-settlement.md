---
module: nexus-core (execution ownership, workspace commit authority) + nexus-core-node (native close)
date: 2026-09-24
problem_type: architecture_pattern
category: architecture-patterns
severity: high
plan_id: 2026-09-22-v1.195-p0-workflow-control
applies_when:
  - "Closing or restarting a hosted runtime owner that holds an engine, an effect/commit authority and OS-level owner leases"
  - "A close must distinguish confirmed cleanup from a retained/unconfirmed one, and a caller may drop or time out mid-close"
  - "A later close or reopen must settle what an earlier interrupted close retained"
  - "Reviewing a shutdown path for stranded in-flight state, lost wakeups or a lease that outlives its owner"
related_components:
  - nexus-core
  - nexus-core-node
  - nexus-service
tags:
  - shutdown
  - confirmed-close
  - cancellation-safety
  - retained-owner
  - owner-lease
  - durable-commit-drain
  - lost-wakeup
  - reference-cycle
last_updated: 2026-09-25
status: active
---

# Cancellation-safe confirmed close — one retained settlement owner settles the drain set

## Context

The current host has one **hosted execution owner** per workspace: the core `ExecutionHandle` (engine, coordinator, scheduler, run rings, workspace commit/recovery authority) reachable through the native `EnvState` (one Host, JS provider sessions, the core handle) and published by the TS service. Closing it is a contract, not a `drop`: a close must end as either **confirmed** (every retained owner released the workspace lease and the authority, so a fresh owner may take the home) or **interrupted** (owners retained, lease held, a pending-operation list reports exactly what is retained, and a later close retries them).

Everything hostile to that contract arrives on this path: the native caller runs the cleanup under a ~5 s budget whose timeout **cancels the future**, the process may be killed, a panic can unwind inside the cleanup, and several callers (service shutdown, a second close, a reopen of an interrupted environment) can race. Four defects found here — each reproduced before its fix, and each a *different* way the same two promises (settle exactly once; never fabricate `confirmed`) can break — are the reason for every rule below:

1. a caller dropped after it had claimed the boundary stranded the in-flight flag, so every later closer and every interrupted-open waiter blocked forever;
2. a retry of a retained verdict ran as a rival owner, so it could read the emptied core slot as "already closed" and publish a **confirmed** report over a cleanup that was still running;
3. a waiter that checked its flag and *then* registered on the completion signal could never be woken by `notify_waiters`;
4. a drain that joined only the drive loops released the workspace lease while a durable commit admitted just before shutdown was still applying — and a settled owner's engine was additionally pinned alive by a strong reference cycle, so a drop-based release never freed the home.

## Guidance

### 1. One claim, one owner, and a published report

The claim and the handoff live in **one synchronous region** of the boundary lock: the flag is set and the retained settlement task is spawned with nothing awaited in between. Cancellation cannot land between claim and handoff, because there is no await point to land on. The settlement itself then runs in its own spawned task, so:

- the caller that claimed the boundary may be cancelled, dropped or time out without cancelling the settlement;
- a panic inside cleanup is observed as a failed join and publishes an **unconfirmed interrupted** close (the runtime has already unwound the guards that hand every claimed owner back) — never a fabricated confirmed one, and never a stranded boundary.

Publication order is fixed: **report first, boundary second, waiters last.** A waiter re-checks the boundary before it reads the report, so the report is visible by the time the boundary is released.

### 2. Waiters register before they re-check

`notify_waiters` stores **no permit**: a settlement that fires between a waiter's flag check and its registration is lost, and the waiter parks forever on an environment that has already settled. Pin the `Notified` future and enable it *before* re-reading the boundary, then await it in a loop:

```rust
let notified = state.close_notify_settled.notified();
tokio::pin!(notified);
notified.as_mut().enable();          // registration happens here
if !*state.close_in_flight.lock().await { return published_report(); }
notified.await;                      // a settlement in between is still delivered
```

The same register-then-check shape is required on the core side (`ExecutionHandle::await_settled`), so both halves of the close agree about what "already settled" means.

### 3. The drain set is more than the drive loops

A close that only joins the scheduler and fires the drive cancellation tokens is incomplete. The ordered contract is:

1. fence new drive admission (so no tick can still be inside an admission while the drives map drains) and **join the scheduler before** the drive drain;
2. **close durable-commit admission** — an admitted durable commit is spawned by its caller and **outlives that caller**, so a caller that passed admission just before shutdown, or whose HTTP waiter disconnected, is not covered by joining the drives;
3. fire every owned cancellation token and join the drive loops;
4. **wait for every already-admitted durable commit**;
5. **release the workspace commit/recovery authority**, then mark the handle settled so the per-DB registry admits a replacement.

Step 5 must happen in the close path, not on the last `Arc` drop. A drop-based release keeps the OS owner lease — and with it the whole home — fenced after the owner has already reported `closed`; without it, a confirmed same-process close followed by a new hosted owner refuses `AuthorityBusy` even though nothing is running. Releasing it is honest only because steps 1–4 already fenced everything this owner could still write through and every admitted commit has joined.

Each subscription this owner minted also ends here: a pull blocked on a silent run wakes with `closed` instead of hanging past the close, and no token survives the generation that minted it.

### 4. A retry is bounded and honest, and it reuses the same owner

A retry of a retained interrupted verdict must

- acquire the **same** single-flight boundary as a fresh close (a rival retry that runs concurrently can otherwise read the emptied core slot as "already closed" and publish a confirmed report over a cleanup that is still running);
- run under the **same outer budget** as the first close — an admitted durable commit may be applying, and the retained drain is bounded, so a retry that waits for it needs its own budget rather than an unbounded wait;
- on expiry, cancel **only that caller's** wait: the retained drain keeps running, the owners stay retained, the report stays interrupted with its pending list, and a later retry observes the settlement that actually happened.

A retry **retries**, it does not replay the cached verdict forever: a JS-provider session whose owned child was never released gets a fresh bounded release attempt before the retained owners are re-run. Claiming a confirmed close still requires every session to report released, so a retry that fails keeps the environment interrupted rather than upgrading an unconfirmed result.

### 5. Break retained-composition cycles at the seam that closes them

An owner that is otherwise dropped can still be pinned alive by a cycle. The verified instance: the coordinator held the schedule supervisor **strongly**, and the supervisor's `ScheduleRunStarter` walked back to the coordinator. After a confirmed close with every ordinary handle dropped, that cycle retained the settled engine, registry, workspace-state provider and prompt executor **per owner generation**. The seam fix is to make the back-reference a `Weak` edge, so the object that must die with the owner can never keep it alive.

### 6. Unconfirmed is a first-class result

`interrupted` carries the retained set (retained owners, unreleased JS provider sessions, `cleanup-owners-retained`) and keeps them claimed; it is actionable, and a later close settles it. Reporting it is always better than either alternative: a fabricated `confirmed` releases a lease over a still-applying commit, and a silent hang strands every later caller with no report at all.

### 7. Fence the admission your join bounds — and make the retry re-enter it

The rule in §3 ("fence new drive admission") has a second drain set and two easier ways to get it wrong, both found on the Actor-session path of the current host (plan `2026-09-24-v1.196-p0-character-execution`).

An **Actor-only quiesce** joins drains that are already registered. That is not a fence: an operation admitted before the quiesce can still register its drain after the zero-count check. The working composition is three steps in order — **freeze that authority's operation admission, join the operations admitted before the freeze, then cancel and join the drains** — and the freeze uses its own latch rather than the closing latch, so reads (and in-flight SSE pulls during the native close window) keep behaving exactly as before the teardown. The same accounting already existed for `close`; the quiesce only had to participate in it. Prefer that over a second bespoke latch: a fresh latch is a new invariant to keep true, not a fence.

The **native teardown** must then treat the quiesce outcome as a precondition rather than a step: an ordered close releases the attached owner only as its **last** step (admission/quiesce confirmed **and** core released **and** its settlement confirmed). When the order comes back unconfirmed, the correct move is to hand the whole authority back with the retained core — not to discard the owner and let a retry read "no attached owner" as proof the Actor side settled. A retry must **re-enter** the quiesce. Two shapes are wrong here and were both reachable: an independent owner discard that leaves the core retained, and a retry that skips the quiesce because the owner is absent.

A companion rule for the join itself: a broadcast completion must wake **every** waiter (`notify_waiters` plus register-before-read — see [tokio-notify-permit-steal-relay-fanout.md](tokio-notify-permit-steal-relay-fanout.md)); a single-permit handoff strands a second concurrent quiesce after the final admission retires.

## Why This Matters

These are all **silent** failure modes. A stranded boundary parks every later closer forever; a lost wakeup parks waiters on an environment that already settled; an incomplete drain exposes the creative root to a new owner while the old commit still applies; a retained composition keeps an engine, its provider and its home alive invisibly; a fabricated confirmed close hands out a lease whose previous holder is still writing. None of them raises an error — the symptoms are "restart hangs", "the service will not reopen its home" or "the new owner refuses `AuthorityBusy`", none of which points at the close path from the call site.

## When to Apply

- Designing or reviewing a close/shutdown/restart path for an owner that holds a lease, an effect authority, spawned tasks or spawned durable work.
- Any place a caller-supplied budget wraps a cleanup that owns state: check whether the inner future can be cancelled while the outer state stays claimed.
- A second `Arc` edge from a child back to its parent object (supervisor ↔ coordinator, handle ↔ factory) — decide `Weak` vs strong at the seam, not later.
- Reviewing waiters on a settled signal: search the diff for `.notified()` and confirm registration precedes the check.

## Examples

### Before — the outer future owns the boundary

```rust
async fn close_core(state: Arc<EnvState>) -> CoreCloseReport {
    *state.close_in_flight.lock().await = true;
    let report = cleanup_owners(state.clone()).await;   // 5s timeout cancels HERE
    *state.close_in_flight.lock().await = false;        // never runs on cancel
    state.close_notify_settled.notify_waiters();
    report
}
```

Dropping the caller strands `close_in_flight`: every later closer and every interrupted-open waiter blocks forever, and the inner budget does not protect the outer future.

### After — the boundary owns a retained settlement task

```rust
async fn claim_close(state: &Arc<EnvState>) {
    let mut in_flight = state.close_in_flight.lock().await;
    if !*in_flight {
        *in_flight = true;
        tokio::spawn(own_close(state.clone()));  // no await between claim and handoff
    }
}
```

`own_close` spawns the settlement body so a panic is a failed join, and publishes report → boundary → waiters in that order.

## Evidence

- Claim/handoff, retained owner, publication and waiter registration — `crates/nexus-core-node/src/lifecycle.rs` (`claim_close`, `own_close`, `settle_claimed`, `settle_retained`, `run_close`, `publish_settled_close`, `await_settled_close`, `cleanup_owners`, `close_core`).
- Sequential drain contract, retained drain task, `Weak` starter edge, run of the fence and commit drain — `crates/nexus-core/src/execution/lifecycle.rs` (`ExecutionHandle::close`, `run_close`, `await_settled`), `crates/nexus-core/src/execution/session.rs` (`close_commit_admission`, `wait_for_admitted_commits`), `crates/nexus-core/src/execution/workflow.rs` (`begin_shutdown`, `admission_fence`, `abort_all_drives`, `CoordinatorScheduleRunStarter.coordinator: Weak<WorkflowRunCoordinator>`).
- Regressions (native): `cancelled_outer_close_keeps_the_settlement_owner`, `concurrent_closes_share_one_settlement`, `settle_retained_retry_is_bounded_and_honest`, `panicking_settlement_owner_publishes_unconfirmed`, `close_timeout_retains_owners_settle_then_reopen`, `later_close_retries_retained_js_session_release`, `confirmed_close_releases_cleanup_ownership`, `unconfirmed_or_interrupted_close_retains_cleanup_ownership`, `interrupted_report_is_never_released` — all in `crates/nexus-core-node/src/lifecycle.rs`.
- Owner-slot and lease semantics this close path completes — [typed-authority-carriers-and-fences.md](typed-authority-carriers-and-fences.md); the Notify wakeup rule this path depends on — [tokio-notify-permit-steal-relay-fanout.md](tokio-notify-permit-steal-relay-fanout.md).

## Coverage gaps

- Literal in-flight `SIGKILL` and installed-deployment shutdown were **not** qualified; the covered cancellation shapes are a dropped/timed-out outer future, a panicking settlement and racing closes.
- The JS-provider release retry is bounded and observed, but a child that never reports released leaves the close interrupted by design rather than verified-`confirmed`.
- Provider-level cancellation is not the stop mechanism for the native provider: the confirmed path is the owned shutdown, not a provider cancel RPC.
