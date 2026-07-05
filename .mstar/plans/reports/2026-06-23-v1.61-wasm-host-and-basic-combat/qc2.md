---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: "2026-06-23-v1.61-wasm-host-and-basic-combat"
verdict: "Approve"
generated_at: "2026-06-23"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: grok-build-0.1 (xai/grok-build-0.1)
- Review Perspective: Security and correctness risk (fuel, memory, wall-time, host whitelist, marshalling, statelessness, untrusted input defense)
- Report Timestamp: 2026-06-23

## Scope
- plan_id: 2026-06-23-v1.61-wasm-host-and-basic-combat
- Review range / Diff basis: d268f8e6..feature/v1.61-wasm-host-and-basic-combat
- Working branch (verified): iteration/v1.61
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 23 (new crate crates/nexus-wasm-host/ + modules/basic-combat/ + plan doc)
- Commit range: cddc1913 (T1–T8), d42d853e (T9), 862bce92 (T10) merged into iteration/v1.61
- Tools run: `git diff d268f8e6..feature/v1.61-wasm-host-and-basic-combat`, `cargo test -p nexus-wasm-host` (17 tests, all PASS), code review of sandbox.rs/host.rs/compute.rs + tests

## Compass Alignment (Q6)
Read from `.mstar/iterations/v1.61-programmable-narrative-progression-delivery-compass-v1.md` §0:
> Q6 | Security model | **Per-invocation sandbox.** Stateless pure function. Fuel + memory + wall-time limits. | Instance-per-call (μs creation). No cross-call state pollution. Reproducible compute.

All claims were verified in source + tests.

## Findings

### 🔴 Critical
(none)

### 🟡 Warning
- **Memory cap is grow-time only.** `StoreLimitsBuilder::memory_size(64 MiB)` + `store.limiter` is wired and catches `grow` traps (mapped to `ComputeError::MemoryCapExceeded`). However, a hostile module can still declare a large initial `memory` size at instantiation time before the limiter fully bounds it. The plan completion report explicitly calls this out as "future hard memory cap that also bounds initial instantiation". For V1 (embedded, trusted modules + fuel+wall-time as primary bounds) this is acceptable defense-in-depth; it becomes material if user-supplied modules are loaded later. Source: `compute.rs:80-82`, `sandbox.rs:24-25`, `error.rs:32-34`, completion report "Risks for Wave 3".

### 🟢 Suggestion
- The WAT probe test (`host_kb_read_end_to_end_via_wasm`) and the rejection test (`non_whitelisted_import_rejected_at_instantiation`) together give strong coverage of the whitelist boundary. Consider adding a second probe that exercises `narrative_query` for symmetry (currently only `kb_read` has an end-to-end WAT probe).
- Wall-time watchdog uses 25 ms cancellable chunks + `AtomicBool` — this is the documented fix from "naive 30 s sleep". The join is best-effort; a long-running module that finishes exactly on the boundary can still race the epoch bump. The current `cancelled` check before `increment_epoch` is correct, but adding a tiny sleep after setting the flag before join (or using a timed join) would make the "prompt cancellation" property even more observable in tests.
- `basic-combat` module itself does not call any host functions (it consumes the inline `key_blocks` snapshot). This is fine for V1, but means the live host-function paths are only exercised by the synthetic probe tests. If future modules will rely on `kb_read`, an integration test that actually calls it from a real module would increase confidence.

## Source Trace
- Finding ID: F-WASM-001 (Warning above)
- Source Type: manual code review + test inspection + compass cross-check
- Source Reference: `git diff d268f8e6..feature/...`, `crates/nexus-wasm-host/src/{sandbox,host,compute,error}.rs`, `tests/sandbox_limits.rs:44-56` (OutOfFuel), `tests/sandbox_limits.rs:62-75` (manifest fuel override), `tests/basic_combat.rs`, `host.rs:319-359` (non-whitelist rejection), `host.rs:248-315` (end-to-end kb_read via WAT), `compute.rs:94-109` (watchdog + 25 ms STEP), `engine.rs:65-70` (consume_fuel + epoch_interruption)
- Confidence: High

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 1 |
| 🟢 Suggestion | 3 |

**Verdict**: Approve

## Detailed Security & Correctness Assessment (per assignment focus)

**Fuel limit enforcement** — Verified.  
`config.consume_fuel(true)` at engine construction; `store.set_fuel(...)` per invocation; `Trap::OutOfFuel` explicitly mapped to `ComputeError::OutOfFuel`. The test `infinite_loop_is_bounded_by_fuel` constructs a real `(loop $forever (br $forever))` WAT module and asserts the exact error variant. Manifest override path also tested. PASS.

**Memory cap (64 MiB)** — Partial but adequate for V1.  
`StoreLimitsBuilder::memory_size(...)` + `store.limiter` is present and traps on grow. The warning above documents the known limitation (initial memory size at instantiation). No test currently attempts to allocate beyond 64 MiB at start; the existing limits + fuel make this low-risk for embedded modules.

**Wall-time limit (30 s)** — Verified, and the noted fix is present.  
`store.epoch_deadline_trap(); store.set_epoch_deadline(1);` + dedicated watchdog thread using 25 ms `STEP` chunks with `AtomicBool` cancellation. On expiry: `engine.increment_epoch()` → `Trap::Interrupt` → `WallTimeExceeded`. The cancel path prevents the thread from bumping epoch after the compute has already finished. Matches the "fix from naive 30 s sleep" described in the assignment.

**Host function whitelist** — Strong.  
`register_host_imports` only links functions present in `manifest.host_functions`. A module importing a non-whitelisted `nexus::*` function fails at `linker.instantiate` (explicit test: `non_whitelisted_import_rejected_at_instantiation`). End-to-end round-trip via a real WAT probe that calls `kb_read` also passes. Only two functions exist in the surface (`kb_read`, `narrative_query`).

**No cross-call state pollution** — Verified.  
Every `compute()` call creates a brand-new `Store` + `InvocationState { ctx: HostContext::from_input(...), limits }`. `HostContext` is built from the exact `key_blocks` + `narrative_state` of that invocation only. `WasmModule` (the compiled artifact) is reused, but no `Instance` or `Store` is cached. Test `compute_is_reproducible_across_invocations` confirms identical output on two fresh calls.

**Memory-buffer marshalling safety** — Clean.  
- `read_bytes` / `write_or_overflow` respect the caller's `out_cap`.
- `write_or_overflow` returns `RET_OVERFLOW (-2)` before any write if the response would exceed cap.
- After `compute` returns, the host checks `written < 0` and `written > out_cap` before reading.
- Sentinels are consistently used: -1 = not-found, -2 = overflow.
- No `unsafe` or raw pointer arithmetic outside wasmtime's safe `Memory::read`/`write` APIs.

**Untrusted module input defense** — Adequate for V1.  
All execution is gated by the three independent limits before user code runs. Host functions only ever see the per-invocation snapshot (no ambient global state). The module cannot influence the host's process beyond its own trap (which is turned into a typed `ComputeError`). Embedded modules are committed binaries; user modules (future) will go through the same path. No privileged operations are exposed.

**Overall for V1** — Defense-in-depth is acceptable. Primary bounds are fuel + wall-time; memory cap provides a useful secondary guard on allocation growth. The one documented gap (initial memory size) is called out in the plan's own "Risks for Wave 3" section and does not block this wave.

All 17 tests (`cargo test -p nexus-wasm-host`) passed cleanly, including the two explicit sandbox enforcement tests and the host-ABI probe/rejection tests.
