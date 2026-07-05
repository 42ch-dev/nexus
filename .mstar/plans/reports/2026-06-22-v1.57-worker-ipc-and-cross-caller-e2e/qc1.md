---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-06-22-v1.57-worker-ipc-and-cross-caller-e2e"
verdict: "Approve"
generated_at: "2026-06-22"
---

# QC1 Review — V1.57 P3 Worker IPC & Cross-Caller E2E

## Reviewer Metadata
- **Reviewer**: @qc-specialist (Reviewer #1)
- **Runtime Agent ID**: qc-specialist
- **Review Perspective**: Architecture coherence and maintainability risk
- **Report Timestamp**: 2026-06-22

## Scope
- **plan_id**: `2026-06-22-v1.57-worker-ipc-and-cross-caller-e2e`
- **Review range / Diff basis**: `65bb9450..2a24267a`
- **Working branch (verified)**: `iteration/v1.57` (HEAD at `2a24267a`)
- **Review cwd (verified)**: `/Users/bibi/workspace/organizations/42ch/nexus`
- **Files reviewed**: 6 (717 insertions, 7 deletions)
- **Commit range**: `65bb9450..2a24267a` (2 commits)
  - `23f99e9a feat(v1.57-p3): worker IPC allowlist dynamic derivation + cross-caller E2E`
  - `2a24267a merge(v1.57): P3 — Worker IPC dynamic allowlist + Cross-Caller E2E (54 cases: 18 IDs × 3 caller paths)`
- **Tools run**: `cargo test -p nexus-daemon-runtime --test cross_caller_e2e`, `cargo test -p nexus-daemon-runtime`, `cargo clippy -p nexus-orchestration -p nexus-daemon-runtime -- -D warnings`

## Acceptance Criteria Checklist

| # | Criterion | Status | Evidence |
|---|-----------|--------|----------|
| 1 | Worker allowlist extended from 1 ID to 18 shipped IDs (reconciled from plan's 35) | ✅ **Met** | `admission_pipeline()` in `host_tool_handlers.rs` now uses `host_tool_registry().lookup()`. Registry has 18 `nexus.*` + 2 `fs/*` = 20 entries. Registry invariant test `registry_has_twenty_host_tools` asserts `reg.len() == 20`. |
| 2 | `worker/agent_tool_request` IPC can dispatch any of the 18 IDs | ✅ **Met** | `dispatch_from_worker()` normalizes IPC into `ToolExecuteRequest` → `execute()` → `admission_pipeline()` → `CapabilityRegistry::dispatch()`. Test `test_worker_dispatches_all_registered_nexus_tools()` verifies no `NOT_SUPPORTED` for any of the 18 IDs via worker path. |
| 3 | Hermetic cross-caller E2E test covering 54 invocation cases (18 × 3) | ✅ **Met** | `crates/nexus-daemon-runtime/tests/cross_caller_e2e.rs` — 10 tests total. The primary parametrized test `all_18_ids_admission_equivalent_across_3_paths` iterates all 18 IDs across 3 paths = 54 cases. |
| 4 | E2E test verifies dispatch equivalence: same (ID, input) → same output across all 3 caller paths | ✅ **Met** | `all_18_ids_admission_equivalent_across_3_paths` asserts same error code across HTTP/Worker/Schedule paths. For success cases, `assert_outputs_equivalent` checks non-timestamp fields match. Additional per-tool tests (`whoami_equivalent_all_3_paths`, `work_get_equivalent_all_3_paths`, `daemon_health_equivalent_all_3_paths`, etc.) provide targeted validation. |
| 5 | E2E test verifies admission gate behavior: rejected on one path → rejected on all 3 | ✅ **Met** | `unknown_tool_rejected_on_all_3_paths` and `not_supported_equivalence_all_3_paths` verify `NOT_SUPPORTED` is returned consistently across all 3 paths for unknown IDs including `nexus.publish.chapter` (which is OUT/deferred-to-V2.0+). |
| 6 | Profile-set IDs verified as §3.3 metadata — not action IDs | ✅ **Met** | `test_profile_sets_are_not_action_capabilities` asserts 3 profile IDs (`nexus.profile.minimal`, `nexus.profile.writer`, `nexus.profile.publisher`) are absent from `host_tool_registry()` and absent from `NEXUS_TOOL_IDS`. This is a correct negative assertion. |
| 7 | `orchestration-engine.md` §6.4 updated | ✅ **Met** | Updated to state "18 shipped `nexus.*` IDs" are dispatchable via worker IPC, documents dynamic allowlist via `CapabilityRegistry::lookup()`. |
| 8 | `daemon-runtime.md` host_tool section updated | ✅ **Met** | V1.57 P3 section added: "The admission pipeline's Gate 1 (tool ID allowlist) now uses `CapabilityRegistry::lookup()` as its dynamic SSOT". Documents E2E test location. |
| 9 | `cargo test -p nexus-daemon-runtime --test cross_caller_e2e` passes (all 54 cases) | ✅ **Met** | 10 tests passed, 0 failed (0.99s). Tests include: `all_18_ids_registered_in_capability_registry`, `all_18_ids_admission_equivalent_across_3_paths`, `unknown_tool_rejected_on_all_3_paths`, `not_supported_equivalence_all_3_paths`, `test_profile_sets_are_not_action_capabilities`, and 5 per-tool equivalence tests. |
| 10 | `cargo test -p nexus-daemon-runtime` passes | ✅ **Met** | 269 unit + 151 integration tests = all passed (no failures). |
| 11 | `cargo clippy -p nexus-orchestration -p nexus-daemon-runtime -- -D warnings` passes | ✅ **Met** | Clean clippy run with no warnings. |

**AC met: 11 / 11**

## Findings

### 🟡 Warning 1: `TOOL_ALLOWLIST` constant kept with `#[allow(dead_code)]`

**File**: `crates/nexus-daemon-runtime/src/api/handlers/host_tool_executor.rs` (lines 27–60)

**Issue**: The `TOOL_ALLOWLIST` constant is now dead code in production — Gate 1 of `admission_pipeline()` uses `host_tool_registry().lookup()` instead. The constant is annotated `#[allow(dead_code)]` and retained for test cross-validation and documentation.

**Assessment**: This is a **justified and pragmatic choice**:
- The cross-validation test `tool_allowlist_matches_registry_ids()` enforces that `TOOL_ALLOWLIST` and the registry stay in sync — if either drifts, the test fails.
- It serves as inline documentation of the canonical tool ID set.
- The alternative (complete removal) would lose the documentation value and break the cross-validation test, which is a useful safety net.

**Recommendation**: Accept as-is. The `#[allow(dead_code)]` annotation is correctly scoped and the justification comment is clear. If future maintainers want stricter hygiene, the constant can be moved into the test module entirely.

### 🟡 Warning 2: Test location deviation (nexus-daemon-runtime vs nexus-orchestration)

**Files**: `crates/nexus-daemon-runtime/tests/cross_caller_e2e.rs` (actual) vs plan-specified `crates/nexus-orchestration/tests/cross_caller_e2e.rs`

**Issue**: The cross-caller E2E test was placed in `nexus-daemon-runtime/tests/` instead of `nexus-orchestration/tests/` as the plan stub specified.

**Assessment**: This is an **architecturally sound decision**:
- The 3-caller dispatch normalization lives in `HostToolExecutor` (`nexus-daemon-runtime`), not in the registry (`nexus-orchestration`).
- The test exercises the unification layer: converting `agent_tool_request` (worker) and `ToolExecuteRequest` (HTTP/CLI) into the common admission + registry dispatch pipeline — all `nexus-daemon-runtime` concerns.
- `nexus-orchestration` does NOT need a cross-caller E2E test because it provides the registry SSOT, and the registry's dispatch behavior is tested via crate-internal tests and the `capability_registry.rs` invariant tests already present there (42 test files covering registry, schedules, workers, convergence, etc.).
- During Wave 1 (P0/P1), the god-file split and 3-caller adapter were placed in `nexus-daemon-runtime` by design. Placing the E2E integration test in the same crate avoids circular test dependencies and keeps the test close to the code being tested.

**Recommendation**: No action needed. The plan stub should be updated to reflect the actual test location — this is a documentation reconciliation issue, not a code quality issue.

### 🟢 Suggestion 1: 18 IDs vs 35 IDs — plan reconciliation

**Context**: The plan stub estimated 35 shipped IDs × 3 paths = 105 cases. The actual implementation has 18 `nexus.*` IDs × 3 paths = 54 cases. The discrepancy is documented in the commit message and the test file's preamble.

**Observation**: The registry correctly contains 18 `nexus.*` + 2 `fs/*` = 20 entries, which aligns with the V1.57 P0 roster and the actual shipped catalog. The plan's 35-ID estimate appears to have been based on the full DF-46 catalog count including unshipped, deferred, or scaffold-equivalent IDs. The reconciliation to 18 shipped IDs is correct.

**Recommendation**: The plan stub in `2026-06-22-v1.57-worker-ipc-and-cross-caller-e2e.md` should be updated at next plan maintenance cycle to reflect the reconciled 18 IDs / 54 cases for historical accuracy.

### 🟢 Suggestion 2: Profile-set test only verifies negative (absence from registry)

**File**: `crates/nexus-daemon-runtime/tests/cross_caller_e2e.rs` — `test_profile_sets_are_not_action_capabilities`

**Observation**: The test verifies that `nexus.profile.{minimal,writer,publisher}` are NOT in the action registry (correct behavior), but does not verify the positive aspect of the §3.3 metadata contract — i.e., that these profile-set IDs ARE registered somewhere as `scaffold-equivalent` metadata. The acp §4 roster (P0 deliverable) is expected to handle the positive tagging.

**Recommendation**: Accept for P3 scope. The negative assertion is sufficient for this plan's scope. The positive assertion (that profile-set IDs are correctly tagged in the acp roster) should be verified in P-last or during the spec consolidation pass.

### 🟢 Suggestion 3: `all_18_ids_admission_equivalent_across_3_paths` uses a list constant duplicated from the registry

**File**: `crates/nexus-daemon-runtime/tests/cross_caller_e2e.rs` — `NEXUS_TOOL_IDS` constant (line ~48)

**Observation**: The constant `NEXUS_TOOL_IDS` duplicates the tool IDs that are already available from `host_tool_registry().ids()`. While the companion test `all_18_ids_registered_in_capability_registry` verifies that all entries in `NEXUS_TOOL_IDS` exist in the registry, this is a one-directional check (constant → registry). If a new tool is added to the registry but forgotten in `NEXUS_TOOL_IDS`, the cross-caller E2E test would silently not cover it.

**Recommendation**: Consider deriving `NEXUS_TOOL_IDS` dynamically from `host_tool_registry().ids()` (filtering to `nexus.*` prefix) instead of maintaining a separate constant. This would eliminate the drift risk entirely. However, accept as-is for P3 — the cross-validation test provides adequate safety.

## Detailed Notes

### Architecture: Dynamic derivation of worker allowlist (Gate 1)

The core architectural change is the shift from a static `TOOL_ALLOWLIST` constant to dynamic `CapabilityRegistry::lookup()` in the admission pipeline. This is **correctly implemented**:

- **`host_tool_handlers.rs` line 43–44**: `let reg = host_tool_registry(); if reg.lookup(&req.tool_name).is_none()` — this is the only Gate 1 check. No hardcoded IDs remain in the admission path.
- **All 3 caller paths converge**: `execute()` (CLI/HTTP), `dispatch_from_worker()` (worker IPC), and `dispatch_for_schedule()` (schedule) all call `admission_pipeline()` → `registry_dispatch()`, so the dynamic allowlist benefits all entry points uniformly.
- **Unknown ID handling**: `NOT_SUPPORTED` returned consistently across all 3 paths.

### Test architecture: 54 cases, 10 tests

The cross-caller E2E test suite has excellent coverage:

| Test | Paths exercised | What it verifies |
|------|----------------|------------------|
| `all_18_ids_admission_equivalent_across_3_paths` | All 3 × 18 IDs = 54 | Same error code (or success) across all paths; output equivalence for successes |
| `unknown_tool_rejected_on_all_3_paths` | All 3 × 1 unknown | `NOT_SUPPORTED` consistency |
| `not_supported_equivalence_all_3_paths` | All 3 × 3 unknowns | `NOT_SUPPORTED` for multiple unknown patterns |
| `whoami_equivalent_all_3_paths` | All 3 | Context tool output equivalence |
| `workspace_info_equivalent_all_3_paths` | All 3 | Workspace tool output equivalence |
| `work_get_equivalent_all_3_paths` | All 3 | Seeded-ID tool output equivalence |
| `daemon_health_equivalent_all_3_paths` | All 3 | Observability tool output equivalence |
| `registry_refresh_equivalent_all_3_paths` | All 3 | Registry tool output equivalence |
| `all_18_ids_registered_in_capability_registry` | N/A (unit) | Registry × NEXUS_TOOL_IDS cross-validation |
| `test_profile_sets_are_not_action_capabilities` | N/A (unit) | §3.3 metadata negative assertion |

### Scope creep check: Clean

All 6 files changed are within P3 scope:
1. `host_tool_executor.rs` — TOOL_ALLOWLIST comment update + `#[allow(dead_code)]` annotation (documentation only)
2. `host_tool_handlers.rs` — Gate 1 dynamic derivation (registry SSOT change)
3. `host_tool_executor_tests.rs` — Worker dispatch tests (testing)
4. `cross_caller_e2e.rs` — New E2E test harness (testing)
5. `orchestration-engine.md` — Spec amendment (documentation)
6. `daemon-runtime.md` — Spec amendment (documentation)

No registry consolidation changes, no god-file refactoring, no schema renames — all properly left to P0/P1/P2.

### Spec amendment consistency

- **`orchestration-engine.md` §6.4**: Updated from "20 registered host tools (nexus.* + fs/*)" to explicitly state "18 shipped `nexus.*` IDs" with mention of 20 total (18 + 2 fs/*). The dynamic allowlist mechanism is documented. Correctly references plan ID. ✅
- **`daemon-runtime.md`**: Added §V1.57 P3 section describing the dynamic derivation mechanism, the registry SSOT, and the E2E test location. Consistent with the implementation. ✅

## Verdict

**Verdict: Approve**

- **Critical**: 0
- **Warning**: 2 (both assessed as justified/mitigated — no action blocks)
- **Suggestion**: 3 (improvement opportunities, no merge block)

All 11 acceptance criteria are met. The implementation correctly shifts the worker allowlist from a static constant to dynamic `CapabilityRegistry::lookup()`, the cross-caller E2E test suite covers 54 invocation cases with proper equivalence and admission-gate assertions, and the spec amendments are accurate and consistent. No scope creep detected.

The two Warnings are both mitigated:
1. **`TOOL_ALLOWLIST` with `#[allow(dead_code)]`**: Mitigated by `tool_allowlist_matches_registry_ids` cross-validation test.
2. **Test location deviation**: Architecturally sound — the test belongs with the dispatch normalization layer in `nexus-daemon-runtime`.

## Completion Report v2

**Agent**: qc-specialist (Reviewer #1, architecture/maintainability)
**Task**: QC1 review of P3 — Worker IPC & Cross-Caller E2E
**Status**: Done
**Scope Delivered**: Full QC1 report covering all 11 ACs, 2 Warnings, 3 Suggestions
**Artifacts**: `.mstar/plans/reports/2026-06-22-v1.57-worker-ipc-and-cross-caller-e2e/qc1.md`
**Validation**: All ACs verified via code inspection, test execution, and clippy
**Issues/Risks**: None blocking. Warning 1 (dead_code) and Warning 2 (test location) are both architecturally justified.
**Plan Update**: Recommend updating plan stub to reconcile 18 vs 35 IDs and test file path at next plan maintenance cycle.
**Handoff**: Report committed to integration branch. PM to consolidate with QC2 and QC3 reports.
**Git**: `git add .mstar/plans/reports/2026-06-22-v1.57-worker-ipc-and-cross-caller-e2e/qc1.md && git commit -m "qc(v1.57-p3): qc1 report — Approve (11/11 AC met)"`
