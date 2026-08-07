---
module: nexus42-connect
date: 2026-08-08
problem_type: architecture_pattern
category: architecture-patterns
severity: medium
applies_when:
  - "Bridging a synchronous callback (libp2p/network event loop) to async orchestrators"
  - "Replacing block_in_place + block_on bridges with a bounded execution model"
tags: [async-bridge, spawn-blocking, semaphore, deadline, tokio, event-loop]
---

# Bounded sync→async bridge for event-loop handlers (Semaphore + spawn_blocking + deadline)

## Context

spoke-connect's `InvokeHandler` runs **synchronously on the node's single
network event loop**. V1.153 bridged to async orchestrators with
`block_in_place` + `Handle::block_on` — one tokio worker blocked per invoke,
no concurrency bound, no deadline, no payload cap. V1.154 replaced it with a
bounded executor.

## Guidance

1. **The wire path is serialized on the event loop.** The handler parks the
   loop for the whole invoke (up to the deadline). The `Semaphore`(8 permits)
   bounds BLOCKING-POOL concurrency — it does NOT make wire processing
   parallel. Document this honestly (specs must not claim wire parallelism).
2. **Shape:** per-process `Arc<Semaphore>` beside the handler singleton →
   bounded acquire → `tokio::task::spawn_blocking` closure (orchestrator runs
   there via `Handle::block_on`, legal: blocking-pool threads are outside any
   async context) → bounded join via `std::sync::mpsc` + `recv_timeout`.
3. **`Handle::block_on` panics inside a runtime context** ("Cannot start a
   runtime from within a runtime") — the acquire must poll the tokio semaphore
   future directly with a `ThreadWaker` + `thread::park_timeout` (no
   busy-spin, precise wake).
4. **Permit semantics:** the spawn_blocking closure cannot be force-cancelled
   safely — move the permit INTO the closure (`let _permit = permit;`), held
   until completion. The deadline bounds the WAIT (acquire + join share ONE
   budget); the late result is discarded via dropped rx.
5. **Response cap:** request-side caps alone are not enough — assembled
   responses can amplify. Enforce a response byte cap at the bridge boundary
   (post-orchestrator, pre-return) with a distinct envelope code; the peer's
   codec cap turns over-cap responses into hard failures otherwise.
6. **Panic containment:** closure panic → mpsc `Disconnected` → `bridge_fault`
   envelope; permit released on unwind; outer `catch_unwind` still wraps the
   sync handler. No panic crosses the invoke boundary.
7. **Injectability:** `BridgeLimits { max_concurrent, deadline_ms,
   max_entries, max_bytes, max_response_bytes }` with `Default` = production
   locks; a test seam returns the `Arc<Semaphore>` so tests can saturate the
   lane deterministically.

```rust
// Skeleton (conceptually):
let permit = semaphore.acquire_bounded(deadline)?;   // ThreadWaker + park_timeout
let (tx, rx) = mpsc::channel();
tokio::task::spawn_blocking(move || {
    let _permit = permit;                            // held until completion
    let _ = tx.send(block_on(orchestrator(request)));
});
match rx.recv_timeout(deadline_remaining) {
    Ok(Ok(v)) => enforce_response_cap(v),
    Ok(Err(e)) => map_reject(e),
    Err(..) => Err(deadline_envelope),
}
```

## Why This Matters

An unbounded bridge turns a slow peer into a worker-thread hog and eventually
a stall; an event-loop handler that blocks without a bound stalls every other
peer. The bounded model gives deterministic behavior: N concurrent orchestrator
calls max, one shared deadline, oversized work rejected before dispatch,
responses capped before return.

## When to Apply

- Any sync callback surface over an async core (network event loops, plugin
  hooks, FFI callbacks).
- Reviewing specs that claim concurrency for a serialized wire path — align
  the wording with the real model.

## Examples

### Before
`block_in_place` + `Handle::block_on` per invoke — one worker blocked per
invoke, unbounded, no deadline, no caps.

### After
Semaphore(8) → bounded acquire → spawn_blocking → mpsc `recv_timeout` join →
response cap; envelope codes `invoke_busy` / `invoke_deadline_exceeded` /
`payload_too_large` / `response_too_large`; source-asserted no `block_in_place`.
