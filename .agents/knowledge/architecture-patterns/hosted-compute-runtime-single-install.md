---
module: nexus-core (hosted execution factory, compute runtime)
date: 2026-09-24
problem_type: architecture_pattern
category: architecture-patterns
severity: medium
plan_id: 2026-09-22-v1.195-p2-compute-service
applies_when:
  - "Installing a WASM/compute runtime into a hosted owner instead of per request"
  - "Warming an embedded module cache or any compile-on-first-use resource at boot"
  - "Deciding what happens when part of a runtime installs and part of it fails"
  - "Placing an engine-global resource (interrupt counter, budget, watchdog) behind a shared serializer"
related_components:
  - nexus-core
  - nexus-wasm-host
  - nexus-core-node
tags:
  - wasm-runtime
  - engine-singleton
  - module-cache-warm
  - all-or-nothing-install
  - engine-epoch-watchdog
  - feature-cohort
status: active
---

# Hosted compute runtime — one engine/cache/serializer per owner, installed only when fully warm

## Context

Compute invocations run through a WASM engine with an embedded module registry, and the hosted owner is the thing that lives long enough to hold them. Three properties of the runtime make per-request construction wrong, and one failure mode makes a partial install worse than no install:

- the engine carries an **engine-global interrupt counter** (the invocation watchdog); per-request counters or a per-request serializer let the shortest budget trap every concurrent invocation;
- compiling a module is expensive enough to be worth doing once (`warm_embedded` over the shipped registry) rather than on the run path;
- the compute edge is a **cohort** decision, not a runtime default: the native/service cohort selects it, while the domain/default and Connect-only cohorts keep the WASM edge off.

## Guidance

### 1. One engine, one cache, one serializer, owned by the owner

Construct the engine, its `ModuleCache` and **one** `Arc<Semaphore>(1)` serializer with the hosted owner and hand the same three handles to every compute request. The single permit is what makes each run's watchdog observe only its own budget: the counter it arms is engine-global, so concurrent invocations sharing the engine would otherwise be trapped at the shortest budget in the set. The serializer is a *correctness* requirement of the engine, not a throughput choice.

### 2. Warm once, before admission

Compile the embedded registry at owner construction (`warm_embedded`), so the run path never compiles per request and first-run latency is not a function of how many modules shipped.

### 3. Install the runtime as ONE bundle, or not at all

`warm_embedded` fails on the **first** module it cannot compile while leaving the successfully compiled ones cached. Installing a partially warmed bundle would therefore let the run path answer `not_found` for a module the registry still **lists** — blaming the caller's module id for an environment fault. So the three dependencies (engine, cache, serializer) are installed together and only after every shipped module warmed. With no bundle installed:

- module **discovery** still answers truthfully (the registry is compiled in, and the registry is not a runtime installation);
- every invocation refuses with the typed missing-runtime error, which is the honest "this owner has no compute runtime" answer.

### 4. Keep the cohort explicit

Select the compute feature in the cohort that must serve it; domain/default and transport-only cohorts keep it off, and the graph-pin probe asserts each cohort instead of assuming one. A build without the feature compiles none of this.

### 5. Do not let readiness imply the runtime

An owner whose WASM engine failed to construct is still a valid owner for the rest of its operations; it reports the missing compute runtime when compute is actually requested. Runtime presence is a capability of the owner, and its absence is a typed refusal — never an empty catalog, never a fake success, never a substituted `not_found`.

## Why This Matters

A per-request engine looks correct until two runs overlap and the shorter watchdog kills the longer one; a per-request cache turns every run into a compile; a partial install produces a **wrong** error class (`not_found` for an id that exists) which sends the caller to fix their module id instead of the environment. All three are invisible in a single-run smoke test and appear only as intermittent traps or as refusals that name the wrong cause.

## When to Apply

- Adding or moving a compute/WASM invocation path into a long-lived owner (service boot, hosted factory, worker pool).
- Any `Option<Runtime>` bundle whose parts are mutually dependent — decide the install condition once, for the bundle.
- Reviewing an "engine-global" resource (interrupt counter, budget, watchdog, epoch) reached from concurrent callers.
- Adding a feature cohort that gates a heavy runtime dependency.

## Examples

### Before — three deps installed independently

```rust
deps.compute_engine = WasmEngine::new().ok().map(Arc::new);
let cache = Arc::new(ModuleCache::new());
let _ = cache.warm_embedded(&engine);          // failure ignored: some modules cached
deps.compute_cache = Some(cache);              // discovery then lists modules that cannot run
deps.compute_serializer = None;                // per-request fallback elsewhere
```

### After — one conditioned bundle

```rust
match WasmEngine::new() {
    Ok(engine) => {
        let engine = Arc::new(engine);
        let cache = Arc::new(ModuleCache::new());
        match cache.warm_embedded(&engine) {
            Ok(_warmed) => {                       // every shipped module compiled
                deps.compute_engine = Some(engine);
                deps.compute_cache = Some(cache);
                deps.compute_serializer = Some(Arc::new(Semaphore::new(1)));
            }
            Err(error) => tracing::error!(%error, "embedded compute modules did not warm; \
                                                  compute runtime stays uninstalled"),
        }
    }
    Err(error) => tracing::error!(%error, "WASM engine unavailable; compute invocations refuse"),
}
```

## Evidence

- The conditioned bundle and its reasoning comments — `crates/nexus-core/src/execution/production.rs` (`start_hosted_execution` compute block).
- Handles carried on the owner and handed to the invocation path — `crates/nexus-core/src/execution/lifecycle.rs` (`RunnerDeps::{compute_engine, compute_cache, compute_serializer}`, `ExecutionHandle::{compute_engine, compute_cache, compute_serializer}`), `crates/nexus-core/src/execution/handle_ops.rs` (`compute_context`).
- Cohort selection — `crates/nexus-core-node/Cargo.toml` (`nexus-core` with `["execution", "provider-host", "compute"]`), asserted per cohort by `tooling/check-graph-pins.sh` (see [graph-pin-honesty-discipline.md](../conventions/graph-pin-honesty-discipline.md)).
- Invocation-path behaviour — `apps/nexus-service/tests/compute-http.test.mjs`: real module schema with zero World effect until accept, a schema-invalid run persisted as an inspectable failed row, ownership/malformed-query refusals, and World-scoped terminal clear that keeps accepted effects.

## Coverage gaps

- The forced embedded-module warm **failure** path was source-reviewed, not fault-injected: no test makes one shipped module uncompilable to observe the uninstalled-bundle refusal end to end.
- The shared serializer's engine-global constraint is documented and carried in code (one permit, one engine per owner), but a dedicated concurrency regression that traps a second invocation under the shortest watchdog budget was not written.
