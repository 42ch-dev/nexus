---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: 2026-06-20-v1.53-capability-registry-unification
working_branch: feature/v1.53-capability-registry-unification
review_cwd: main worktree
review_range: 71dc6b1d..69594902
focus: performance/reliability
date: 2026-06-20
generated_at: "2026-06-20"
verdict: Approve with Notes
---

# QC #3 Review — V1.53 P0 CapabilityRegistry Unification (performance/reliability)

## Summary

This review covers the six commits in `71dc6b1d..69594902` that implement the V1.53 P0 `CapabilityRegistry` unification in `crates/nexus-daemon-runtime`. The change replaces the old `dispatch_tool()` match table with a central `CapabilityRegistry` (id → access → admission → handler → ACP wire → failure mode → test vector) and routes all host-tool entrypoints through it.

Overall, the cutover is clean and complete: Sub-phase 3 removed the old match table, parity/cutover tests are present, and `cargo clippy` / `cargo +nightly fmt --check` pass. From a performance/reliability standpoint, the registry is stateless per call, uses plain function pointers (no trait-object allocations or captured closures), and returns deterministic errors for unknown tool ids. The main concern is that the registry is rebuilt on every dispatch invocation (`HostToolExecutor::execute`, `dispatch_for_schedule`, and therefore `DaemonToolDispatchAdapter`), which introduces per-call heap allocations on what is also the schedule-execution hot path. For V1.53's ≤8 tools and single-daemon model this is acceptable, but it should be baselined and cached rather than left as the long-term shape.

## Verification evidence

```text
# Commit range
$ git log --oneline 71dc6b1d..69594902
69594902 docs(v1.53-p0): fill capability-registry field semantics + plan roadmap
11718930 style: cargo +nightly fmt --all
d94e9674 style(v1.53-p0): fix clippy warnings — doc_markdown, missing_errors_doc, too_many_lines
e8a39db4 refactor(v1.53-p0-sp3): remove old dispatch_tool() match table
85559d0d feat(v1.53-p0-sp2): cutover HostToolExecutor::execute() to CapabilityRegistry
1d8b4452 feat(v1.53-p0-sp1): introduce CapabilityRegistry behind adapter

# Diff summary
$ git diff --stat 71dc6b1d..69594902
 .mstar/knowledge/specs/capability-registry.md      | 159 +++++-
 ...-06-20-v1.53-capability-registry-unification.md |  40 +-
 .../src/api/handlers/host_tool_executor.rs         | 345 ++++++++++---
 .../src/capability_registry.rs                     | 565 +++++++++++++++++++++
 crates/nexus-daemon-runtime/src/lib.rs             |   1 +
 5 files changed, 1031 insertions(+), 79 deletions(-)

# Tests (8 threads)
$ cargo test -p nexus-daemon-runtime --lib capability_registry -- --test-threads=8
running 7 tests ... test result: ok. 7 passed

$ cargo test -p nexus-daemon-runtime --lib host_tool_executor -- --test-threads=8
running 15 tests ... test result: ok. 15 passed

$ cargo test -p nexus-daemon-runtime --test agent_tool_production_wiring -- --test-threads=8
running 6 tests ... test result: ok. 6 passed

$ cargo test -p nexus-daemon-runtime --lib -- --test-threads=8
running 200 tests ... test result: ok. 200 passed

$ cargo clippy -p nexus-daemon-runtime -- -D warnings
    Finished dev profile: no warnings

$ cargo +nightly fmt --all --check
(no output)
```

## Findings

### Blocking / High severity
(none)

### Medium severity
- R-V153P0QC3-001: Per-dispatch registry allocation on the schedule hot path
  - Severity: medium
  - Scope: `crates/nexus-daemon-runtime/src/api/handlers/host_tool_executor.rs:379`, `crates/nexus-daemon-runtime/src/capability_registry.rs:256`
  - Decision: accept (defer to post-P0 optimization, track as residual)
  - Evidence:
    ```rust
    // host_tool_executor.rs:379
    let reg = crate::capability_registry::host_tool_registry();
    let dispatch_result = reg.dispatch(req, state, &creator_id).await;
    ```
    ```rust
    // capability_registry.rs:256-258
    pub fn host_tool_registry() -> CapabilityRegistry {
        use crate::api::handlers::host_tool_executor as hte;
        let mut reg = CapabilityRegistry::new();
    ```
  - Note: Every call to `registry_dispatch()` constructs a fresh `CapabilityRegistry`, allocates a new `HashMap`, and inserts 8 rows each containing a newly allocated `Vec<AdmissionGate>`. For HTTP/agent calls this is negligible, but `DaemonToolDispatchAdapter::dispatch_tool` → `dispatch_for_schedule` → `execute` → `registry_dispatch` means schedule-fired tool calls (potentially 60+/min across many Works) pay this cost every time. The old match table had zero allocation. V1.53's scope and scale make this tolerable, but PM should ensure the deferred optimization (R-V153P0-001) lands with a benchmark and a cached `once_cell::sync::Lazy<CapabilityRegistry>` or `std::sync::LazyLock` singleton.

### Low severity
- R-V153P0QC3-002: Missing dispatch-latency benchmark for the match→HashMap cutover
  - Severity: low
  - Scope: `crates/nexus-daemon-runtime/` (no `benches/` directory)
  - Decision: accept (add in optimization follow-up)
  - Evidence: `find crates/nexus-daemon-runtime -name '*bench*'` returned no files.
  - Note: The plan documents the old path as "compiler-optimized O(1)" and the new path as "amortized O(1)" but there is no Criterion or custom benchmark measuring p50/p99 dispatch latency. A benchmark should be added before the caching optimization so the improvement is quantified.

- R-V153P0QC3-003: Admission vectors could be static slices instead of `Vec`
  - Severity: low
  - Scope: `crates/nexus-daemon-runtime/src/capability_registry.rs:141`, `:256-449`
  - Decision: accept
  - Evidence: `pub admission: Vec<AdmissionGate>,` and each `reg.register(CapabilityRow { admission: vec![...] })`.
  - Note: Since the gate lists are immutable and known at compile time, using `&'static [AdmissionGate]` would remove the per-row heap allocation in `host_tool_registry()` even before a global cache is introduced. This is a low-risk future cleanup.

### Nit / observation
- The verification command block in the assignment references `crates/nexus-daemon-runtime/src/host_tool_executor.rs`, but the actual file is `crates/nexus-daemon-runtime/src/api/handlers/host_tool_executor.rs`. This is harmless but should be corrected in the assignment template for future reviews.
- Concurrency safety confirmed: `CapabilityRegistry` contains only an immutable `HashMap<&'static str, CapabilityRow>` after construction; `dispatch` takes `&self`. There are no `Mutex`, `RwLock`, `RefCell`, or `Lazy` in `capability_registry.rs`. `HashMap::new()` is thread-safe by construction because each call produces an independent value.
- No resource leaks: handlers are `for<'a> fn(...)` function pointers; the registry does not hold trait objects, closures, or `Box`ed state. Rows hold `&'static str` only.
- No panics on missing keys: `CapabilityRegistry::dispatch` returns `NexusApiError::BadRequest { code: "NOT_SUPPORTED" }` for unregistered tools, consistent with the old allowlist rejection.
- No infinite loops in `admission_pipeline`: it is a straight-line sequence of five gates with no recursion or iteration over unbounded inputs.
- Daemon startup cost is unchanged: `crates/nexus-daemon-runtime/src/boot.rs:153` builds the orchestration-level `CapabilityRegistry::with_runtime_deps` once at boot; it does **not** call the local `host_tool_registry()`. The per-call cost is therefore paid on first tool dispatch, not startup.
- Test reliability: all 200 unit tests and the 6 `agent_tool_production_wiring` integration tests passed under `--test-threads=8` on the first run. The tests create isolated `create_test_workspace()` roots and unique temp files, so `#[serial]` is unnecessary.

## Verdict

**Approve with Notes**

The P0 registry unification is functionally correct, cut over cleanly, and passes the full lint/test matrix. The only performance/reliability concern is the per-dispatch allocation overhead introduced by rebuilding `CapabilityRegistry` on every call, which matters because `DaemonToolDispatchAdapter`/`dispatch_for_schedule` place that path on the schedule-execution hot path. This is acceptable for V1.53's small registry and single-daemon scope, but it must be tracked and resolved by the deferred optimization (caching + benchmark). No blocking or high-severity issues were found.
