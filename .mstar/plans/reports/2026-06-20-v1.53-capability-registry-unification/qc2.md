---
plan_id: 2026-06-20-v1.53-capability-registry-unification
working_branch: feature/v1.53-capability-registry-unification
review_cwd: main worktree
review_range: 71dc6b1d..69594902
reviewer_index: 2
focus: security/correctness
date: 2026-06-20
verdict: Approve with Notes
---

# QC #2 Review — V1.53 P0 CapabilityRegistry Unification (security/correctness)

## Summary

Reviewed the 6 commits in `71dc6b1d..69594902` implementing the adapter-first `CapabilityRegistry` SSOT refactor. The work introduces a 7-field registry row (id/access/admission/handler/ACP wire/failure mode/test vector), migrates all 8 V1.34 host tools (6 `nexus.*` + 2 `fs/*`), and executes a clean three-sub-phase cutover with explicit triggers.

**Security/correctness assessment**: The refactor preserves the single dispatch invariant and the 5-gate `admission_pipeline()`. No bypass paths were found. `execute()` → `registry_dispatch()` → `admission_pipeline()` → `CapabilityRegistry::dispatch()` is the only code path after Sub-phase 3. `DaemonToolDispatchAdapter` (DF-47) correctly delegates through `dispatch_for_schedule()` → `execute()`, which is by design and not a bypass. All tests pass (34 in crate + 6 in agent_tool_production_wiring).

**Key strengths**: Explicit cutover triggers recorded in plan; old `dispatch_tool()` match table fully removed; failure codes (`NOT_SUPPORTED`, `POLICY_BLOCKED`, `FORBIDDEN`) consistent across paths; `AdmissionGate` enum is declarative (actual enforcement stays in `admission_pipeline()`).

**Primary concern**: Parity test coverage is narrow (only `whoami` and `workspace.info` have dedicated parity tests; 3 parity + 2 cutover + 1 unknown-tool test total). The plan claims 8 existing tools; only 2 received parity verification. Cross-validation test (`registry_cross_validates_prefix`) only checks the `nexus.` / `fs/` prefix convention — it does not enforce the catalog↔registry id bijection asserted in `capability-registry.md` §2.1.

## Verification evidence

```bash
# Branch and range
git checkout feature/v1.53-capability-registry-unification
git log --oneline 71dc6b1d..69594902
# 69594902 docs(v1.53-p0): fill capability-registry field semantics + plan roadmap
# 11718930 style: cargo +nightly fmt --all
# d94e9674 style(v1.53-p0): fix clippy warnings
# e8a39db4 refactor(v1.53-p0-sp3): remove old dispatch_tool() match table
# 85559d0d feat(v1.53-p0-sp2): cutover HostToolExecutor::execute() to CapabilityRegistry
# 1d8b4452 feat(v1.53-p0-sp1): introduce CapabilityRegistry behind adapter

git diff --stat 71dc6b1d..69594902
# 5 files changed, 1031 insertions(+), 79 deletions(-)

# Auth/authz preservation
grep -n 'admission_pipeline\|AdmissionGate' crates/nexus-daemon-runtime/src/api/handlers/host_tool_executor.rs
# 15, 47, 167 (definition), 367 (call inside registry_dispatch)

grep -n 'pub fn invoke\|fn invoke(' crates/nexus-daemon-runtime/src/capability_registry.rs
# (no matches — registry has no public invoke; dispatch() is the entry)

# Bypass risk check
grep -rn 'HandlerBinding::invoke\|registry_dispatch\|admission_pipeline' crates/ --include="*.rs" | head -20
# All references route through registry_dispatch → admission_pipeline or are tests.
# No direct HandlerBinding or trait-object bypass outside the registry path.

# DaemonToolDispatchAdapter (R-V153P0-002)
grep -rn 'DaemonToolDispatchAdapter\|dispatch_for_schedule' crates/ --include="*.rs"
# boot.rs:160 wires adapter; host_tool_executor.rs:402 (dispatch_for_schedule calls execute)
# agent_tool_production_wiring.rs:254, 470 — correct delegation, not bypass.

# Test execution
cargo test -p nexus-daemon-runtime --test agent_tool_production_wiring
# 6 tests passed (includes schedule vs execute equivalence)

cargo test -p nexus-daemon-runtime
# 34 tests passed + 1 doc test; 0 failures
```

## Findings

### Blocking / High severity
(none)

### Medium severity
- **R-V153P0QC2-001**: Narrow parity test coverage — only 2 of 8 tools have explicit parity tests
  - Severity: medium
  - Scope: `crates/nexus-daemon-runtime/src/api/handlers/host_tool_executor.rs:1497-1616`
  - Decision: accept (P1 will expand coverage)
  - Evidence:
    ```rust
    // Only these two parity tests:
    async fn registry_parity_whoami() { ... HostToolExecutor::execute vs registry_dispatch }
    async fn registry_parity_workspace_info() { ... }
    async fn registry_parity_unknown_tool() { ... }  // plus 2 cutover tests
    ```
  - Note: Plan §7 states 8 existing `nexus.*` + `fs/*` tools. Parity only proven for `whoami`/`workspace.info`. `work.get`, `work.patch`, `orchestration.schedule_status`, `context.assemble`, and both `fs/*` tools have no parity assertions. Acceptable for P0 refactor; P1 should add parity or at least regression tests for the 5 DF-46 read-heavy rows.

- **R-V153P0QC2-002**: Cross-validation test only checks prefix convention, not catalog↔registry id bijection
  - Severity: medium
  - Scope: `crates/nexus-daemon-runtime/src/capability_registry.rs:514-523`
  - Decision: accept with note (spec claim vs test reality)
  - Evidence:
    ```rust
    #[test]
    fn registry_cross_validates_prefix() {
        for id in reg.ids() {
            assert!(id.starts_with("nexus.") || id.starts_with("fs/"), ...);
        }
    }
    ```
  - Note: `capability-registry.md` §2.1 states: "Every registry `id` must have a corresponding row in `acp-capability-set.md`... Tests enforce this invariant." The only cross-validation test is prefix-only + field-completeness (`#[test] fn registry_validates_all_fields_populated`). No test compares `host_tool_registry().ids()` against the logical catalog in `acp-capability-set.md`. This is a correctness gap against the spec even if not a runtime bypass.

### Low severity
- **R-V153P0QC2-003**: No test for concurrent dispatch or re-entrancy
  - Severity: low
  - Scope: test modules in `host_tool_executor.rs` and `capability_registry.rs`
  - Decision: accept (not required for P0 correctness)
  - Evidence: 15 test functions total; all are single-threaded hermetic unit tests. No `tokio::spawn` / concurrent dispatch test. Assignment explicitly asked "test for concurrent dispatch?" — none exists.
  - Note: Registry dispatch is `&self` and handlers are `fn(...) -> Pin<Box<dyn Future>>`. Concurrent safety is assumed via existing workspace/DB isolation, not proven by new tests. Low risk for V1.53 P0.

- **R-V153P0QC2-004**: `dispatch_for_schedule` is a thin wrapper; no separate admission test for Schedule caller kind
  - Severity: low
  - Scope: `host_tool_executor.rs:402-417`
  - Decision: accept
  - Evidence:
    ```rust
    pub async fn dispatch_for_schedule(...) {
        let req = ToolExecuteRequest { ..., caller_kind: Some(HostToolCallerKind::Schedule) };
        Self::execute(&req, state).await  // goes through registry_dispatch + admission
    }
    ```
  - Note: This is correct by design (per plan R-V153P0-002). However, no unit test asserts that `caller_kind=Schedule` produces identical admission behavior to `AcpAgent` for a `nexus.*` tool. The E2E in `agent_tool_production_wiring.rs` covers one tool; broader coverage is deferred.

### Nit / observation
- **R-V153P0QC2-N01**: Registry `AdmissionGate` enum is purely declarative (records intent); enforcement lives in `admission_pipeline()`. This is correct and safe for V1.53. Future plans that want to drive admission from the registry row will need to wire the enum into the gate logic — document the invariant that "declarative ≠ enforced" until that happens.
- **R-V153P0QC2-N02**: 8 `pub(crate)` registry wrapper functions (`registry_context_whoami`, etc.) exist solely to adapt existing handlers to `RegistryHandlerFn`. They are not public and do not create bypass surface.
- **R-V153P0QC2-N03**: No new `pub` exports, no `unsafe`, no new env-var reads, no new arbitrary file-path access introduced. Refactor surface is clean.

## Verdict

**Approve with Notes**

The security and correctness invariants are preserved. The adapter-first migration completed all three sub-phases without leaving dual dispatch paths. `admission_pipeline()` remains the single gate before any handler, and `DaemonToolDispatchAdapter` correctly routes through the unified `execute()` path. The primary medium findings (narrow parity coverage and incomplete cross-validation against the catalog) are acceptable for a P0 refactor whose explicit scope was "introduce registry SSOT and remove old table." P1 (DF-46 read slice) is the natural place to expand parity and catalog-registry validation. No blocking security or correctness defects were identified.
