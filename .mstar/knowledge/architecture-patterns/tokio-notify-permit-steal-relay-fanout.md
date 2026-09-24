---
module: connect-peer-tools, nexus-core-node (native close), nexus-core (execution owner)
date: 2026-08-27
problem_type: architecture_pattern
category: architecture-patterns
severity: high
applies_when:
  - "Multiple async consumers must observe the same tokio::sync::Notify shutdown/refresh signal"
  - "A lane spawns helper tasks (watcher, relay, supervisor) that select on a caller-owned Notify"
  - "Debugging a hang where a shutdown or refresh signal stops arriving after adding a second listener"
  - "Reviewing Notify fan-out for lost-wakeup or permit-stealing windows"
  - "A waiter on a broadcast completion signal (`notify_waiters`) is structured as check-then-await"
tags:
  - tokio-notify
  - permit-steal
  - notify-waiters
  - register-before-check
  - shutdown-fanout
  - relay-task
  - lost-wakeup
  - peer-tools
last_updated: 2026-09-24
created: 2026-08-27
status: active
---

# tokio::sync::Notify permit-steal — one Notify, one consumer; fan out via relay

## Context

`tokio::sync::Notify::notify_one` stores **at most one permit**. When two tasks `select` on the same `Notify`, whichever registers first (or resumes first after storage) consumes the stored permit — the other waits forever. This is not a race you can fix with ordering; permit storage is the documented semantics.

V1.179 P1 (DF-92 peer config hot reload) hit this exactly: the peer-tools lane's accept loop owned a shutdown `Notify`. T1 spawned a config-watch task that `select!`ed on that same Notify alongside the accept loop. The watcher "stole" the shutdown permit — `authz_hello` lane tests hung at the change and passed at BASE, reproducing the hazard deterministically.

## Guidance

**A shared Notify may have exactly one waiter.** To wake N consumers:

1. Keep the **caller's Notify single-consumer** (invariant carried from p0's `boot.rs` shutdown wiring).
2. Spawn a **one-shot relay task** owned by the lane handle:
   - relay `select`s on the caller's Notify (the only waiter);
   - on fire, `notify_one` on each **per-child Notify**;
   - relay task ends (one-shot, not a leak loop).
3. Children `select` on **their own** Notify (plus their real work). Permit storage closes both lost-wakeup windows:
   - caller fires before relay polls → permit stored on caller's Notify → relay completes immediately and fans out;
   - relay `notify_one`s before a child registers → permit stored on the child's Notify → the child's first `notified()` consumes it.

Do **not** abort-on-drop helper tasks as the shutdown mechanism (aborting the supervisor still leaves a detached relay/inner task parked); keep cooperative exit via the relayed Notify, and keep lane-drop semantics identical to the existing `boot.rs drop(handle.task)` precedent.

### The second hazard: `notify_waiters` stores no permit, so a waiter must register before it checks

The permit-storage rule above covers `notify_one`. A **broadcast completion** signal is different: `notify_waiters` wakes only the waiters registered *at the moment it fires* and leaves **no permit** behind. A waiter written as check-then-await can therefore park forever on a signal that already fired:

```rust
// WRONG — a settlement landing between the check and the registration is lost
if !*state.close_in_flight.lock().await {
    return published_report();
}
state.close_notify_settled.notified().await;   // never woken
```

The waiter must pin and **enable** its `Notified` future *first*, then re-check the condition, then await — and loop, because the condition can become true between the check and the await:

```rust
loop {
    let notified = state.close_notify_settled.notified();
    tokio::pin!(notified);
    notified.as_mut().enable();                            // register HERE
    if !*state.close_in_flight.lock().await {              // then check
        return state.settled_close.lock().await.clone()
            .unwrap_or_else(|| interrupted_report(vec![])); // never a fabricated confirmed close
    }
    notified.await;
}
```

The publisher's half of the contract is an ordering: publish the state, then the boundary/flag, then `notify_waiters` — so the value a woken waiter reads is the value the signal was about.

This was found as a confirmed defect in the native close path (an initial concurrent closer could park forever while the settlement had already published): the fix is register-then-check on the waiter side plus report→flag→notify on the publisher side.

## Why This Matters

The failure mode is a **silent hang**, not an error: the accept loop simply never observes shutdown, tests time out, and the culprit looks like "flaky tests" rather than a signal-plumbing defect. Permit-stealing also inverts causality — adding a *listener* (the watcher) is what breaks the *original* consumer, so git-level inspection of the accept loop finds nothing wrong.

## When to Apply

- Spawning any helper task that must react to a lane/process shutdown or config-refresh signal
- Reviewing diffs that add a second `notified()` on an existing Notify
- Designing lane handles that own child tasks (`watch_task`, relays) — decide the exit contract (cooperative Notify vs abort) at handle creation and document it at the spawn site
- Waiting on a broadcast completion (`notify_waiters`) such as a settle/close report: register before checking, and consider the wait loop's exit condition a contract rather than an optimization

## Examples

V1.179 P1 T1 (`crates/nexus-daemon-runtime/src/connect/watch.rs` + `accept.rs`): `PeerToolsLaneHandle` gained a relay task (`_shutdown_relay`) plus per-child Notify; `spawn_peer_config_watch` selects on the child Notify. Post-fix, `authz_hello` 7/7 and the spawned-watcher e2e (real reload without restart) both pass; reviewer verdict on the relay: "no remaining lost-wakeup window on this seam." Red-green evidence: hang reproduced at the naive wiring, pass at BASE, pass after relay.

V1.195 P0 (`crates/nexus-core-node/src/lifecycle.rs` `await_settled_close`; the same shape in `crates/nexus-core/src/execution/lifecycle.rs` `ExecutionHandle::await_settled`): the close-report waiter registered after checking the in-flight flag, so a settlement could publish between the check and the registration and strand the caller; the confirmed fix is register (`pin!` + `enable()`) then check then loop, with the publisher ordering report → boundary → `notify_waiters`. The whole close contract that depends on it — [cancellation-safe-confirmed-close-settlement.md](cancellation-safe-confirmed-close-settlement.md).

## Prevention

House rule for lane handles: **caller Notify stays single-consumer; every additional consumer gets a per-child Notify + relay**. Document the exit contract (cooperative Notify, never abort-on-drop) at every spawn site that touches a shared Notify. When reviewing, search the diff for `.notified()` — more than one registration on the same Notify instance is a finding.

House rule for waiters: **register before you check, then loop.** For a `notify_waiters` broadcast, pin + `enable()` the future first, re-check the condition, and await inside a loop; the publisher publishes state → flag/boundary → `notify_waiters`, in that order. A `check … then .notified().await` shape is a finding even when the flag and the notify are both correct in isolation.
