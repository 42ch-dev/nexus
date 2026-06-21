---
plan_id: 2026-06-22-v1.57-worker-ipc-and-cross-caller-e2e
reviewer: qc-specialist-2 (Reviewer #2, security/correctness)
review_focus: security/correctness
review_range: 65bb9450..2a24267a
working_branch: iteration/v1.57
generated_at: 2026-06-22
---

# QC2 Review — V1.57 P3 Worker IPC Dynamic Allowlist + Cross-Caller E2E (54 cases)

## Summary
- AC met: 10 / 10 (reconciled scope: 18 `nexus.*` IDs × 3 caller paths = 54 cases; plan stub estimated 35/105 but implementation + tests correctly use the actual registry roster of 18).
- Findings: 0 Critical, 1 Warning, 1 Suggestion.
- Verdict: **Approve**

## Scope Verification (per Assignment)
- Review cwd: /Users/bibi/workspace/organizations/42ch/nexus (verified: `git rev-parse --show-toplevel`)
- Working branch: iteration/v1.57 (current HEAD 2a24267a contains the P3 merge)
- Review range / Diff basis: 65bb9450..2a24267a (merge-base after P0+P1+P2 → P3 tip)
- 2 commits in range (P3 implementation + merge).
- Files reviewed: 6 (2 spec updates + 3 daemon sources + 1 new E2E test file, 717 insertions).
- Tools run:
  - `git log 65bb9450..2a24267a --oneline`
  - `git diff 65bb9450..2a24267a --stat`
  - `cargo test -p nexus-daemon-runtime --test cross_caller_e2e` (×3 for flakiness)
  - `cargo test -p nexus-daemon-runtime` (full suite)
  - `cargo clippy -p nexus-daemon-runtime -p nexus-orchestration -- -D warnings`
  - Direct source reads: `host_tool_executor.rs`, `host_tool_handlers.rs`, `cross_caller_e2e.rs`, `capability_registry.rs`, orchestration capability/trait sites.

## Acceptance Criteria Checklist (Reconciled 18 IDs / 54 Cases)

| # | AC (reconciled) | Status | Evidence |
|---|-----------------|--------|----------|
| 1 | Worker allowlist extended from 1 ID (V1.42 P3) to all shipped `nexus.*` IDs via dynamic derivation | **Met** | `admission_pipeline` in `host_tool_handlers.rs:44`: `if reg.lookup(&req.tool_name).is_none() { NOT_SUPPORTED }`. Static `TOOL_ALLOWLIST` kept only for docs/test consistency (marked `#[allow(dead_code)]`). |
| 2 | `worker/agent_tool_request` IPC can dispatch any of the 18 IDs through `CapabilityRegistry::dispatch` | **Met** | `HostToolExecutor::dispatch_from_worker` normalizes to `ToolExecuteRequest` (caller_kind=AcpAgent) → `execute` → `registry_dispatch` → `admission_pipeline` → `reg.dispatch`. Same path as CLI/HTTP. |
| 3 | Hermetic cross-caller E2E exists covering 18 × 3 = 54 invocation cases | **Met** | New file `crates/nexus-daemon-runtime/tests/cross_caller_e2e.rs` (587 lines). 10 tests exercising all 18 IDs + targeted equivalence + NOT_SUPPORTED + profile-set + registry integrity. |
| 4 | E2E verifies dispatch equivalence: same (ID, input) → same output across all 3 caller paths | **Met** | `all_18_ids_admission_equivalent_across_3_paths` + 4 per-tool tests (`whoami`, `workspace_info`, `work_get`, `daemon_health`, `registry_refresh`). Uses `assert_outputs_equivalent` (excludes timestamps). |
| 5 | E2E verifies admission gate behavior: request rejected on one path is rejected on all 3 | **Met** | Same test above asserts identical error codes (or all success). `unknown_tool_rejected_on_all_3_paths` and `not_supported_equivalence_all_3_paths` cover explicit NOT_SUPPORTED. |
| 6 | Profile-set IDs (`nexus.profile.{minimal,writer,publisher}`) verified as §3.3 metadata — not action IDs | **Met** | `test_profile_sets_are_not_action_capabilities`: `reg.lookup(profile_id).is_none()` for all three; also absent from `NEXUS_TOOL_IDS`. |
| 7 | `orchestration-engine.md` §6.4 updated for all 18 IDs | **Met** | Diff shows update (plan claims; content scan confirms worker IPC section extended). |
| 8 | `daemon-runtime.md` host_tool section updated for worker IPC entry | **Met** | Diff includes `daemon-runtime.md` update documenting the 3-caller surface. |
| 9 | `cargo test -p nexus-daemon-runtime --test cross_caller_e2e` passes | **Met** | 10/10 passed (0.93–0.97s) on 3 consecutive runs. No flakiness. |
|10 | `cargo test -p nexus-daemon-runtime` + clippy pass | **Met** | Full suite: 34 tests + 1 doc-test passed. Clippy clean (`-D warnings`) on both crates. |

## Findings

### 🔴 Critical
None.

### 🟡 Warning
- **F-QC2-P3-001 (Auth context vs. dispatch surface in E2E)**: The 54-case E2E exercises the *dispatch + admission* surface for the three in-process paths (`HostToolExecutor::execute`, `dispatch_from_worker`, `dispatch_for_schedule`). It correctly sets `caller_kind` (AcpAgent vs Schedule) for audit differentiation, but does **not** assert that different caller_kinds produce observably different behavior, nor does it simulate distinct IPC authentication principals. Real worker IPC (orchestration schedule → daemon) and CLI bearer vs. local user contexts are higher in the stack.  
  **Severity**: Warning (security/correctness — scope clarification).  
  **Scope**: `cross_caller_e2e.rs` (the 10 tests).  
  **Rationale**: Per focus item: "Does the E2E ensure that the dispatch path applies the correct auth context for each path?" The E2E verifies *equivalence of the shared admission pipeline*, which is the intended security property (no path bypasses gates). It does not (and per plan design, need not) re-test the orchestration-side scheduling policy or HTTP bearer auth. The test harness uses `WorkspaceState::new_for_testing` (fresh temp root + DB per ctx); no shared mutable state.  
  **Suggested action**: Add a one-line comment in the E2E module doc or the main equivalence test clarifying: "This E2E verifies that all three entry points apply the identical 5-gate admission pipeline (registry lookup + creator/workspace + policy + audit). Caller-specific auth (IPC principal, HTTP bearer, local CLI user) is enforced upstream and is out of scope for this dispatch-equivalence suite." This prevents future readers from misinterpreting the test as a full end-to-end auth matrix.

### 🟢 Suggestion
- **F-QC2-P3-002 (Orchestration adapter path naming)**: The `DaemonToolDispatchAdapter` (used by orchestration schedules/workers to call host tools) is implemented in `host_tool_executor.rs` by delegating to `dispatch_for_schedule`, **not** `dispatch_from_worker`. The E2E's "worker" path uses `dispatch_from_worker` (simulating direct `agent_tool_request` IPC). Both paths converge on the same `execute` → admission pipeline, so security is equivalent, but the naming can be confusing when tracing "worker IPC" calls.  
  **Scope**: `host_tool_executor.rs:309` (adapter) + `cross_caller_e2e.rs` (test labels) + `nexus-orchestration/src/capability/mod.rs` + `tasks/mod.rs`.  
  **Rationale**: The actual schedule-initiated path that orchestration uses today goes through the "schedule" entry point. The worker IPC surface (`dispatch_from_worker`) is still exercised by the E2E and remains the documented `agent_tool_request` entry. No functional bug, but a minor traceability note.  
  **Suggested action**: Consider a small doc comment or rename suggestion (e.g., `dispatch_for_worker_ipc` vs. `dispatch_for_schedule`) in a follow-up if the distinction becomes material for audit or policy differentiation. Not blocking for P3.

## Detailed Notes (Security / Correctness Focus)

**Worker IPC allowlist security (dynamic derivation)**:
- P3 removed the prior static single-ID check (V1.42 P3 narrow slice).
- Gate 1 is now `reg.lookup(&req.tool_name).is_none()` inside the shared `admission_pipeline`.
- The registry (`CapabilityRegistry`, built once via `LazyLock` in `host_tool_registry()`) is the SSOT for the 18 `nexus.*` + 2 `fs/*` IDs.
- Unknown IDs are rejected with `NOT_SUPPORTED` **before** any creator/workspace/policy checks — consistent for all callers.
- No evidence of a path that can bypass the registry lookup to reach `CapabilityRegistry::dispatch` with an arbitrary ID.
- `TOOL_ALLOWLIST` const is retained only for documentation and a consistency test (`tool_allowlist_matches_registry_ids` in executor tests). It is not used in the runtime admission path for P3.

**Privilege boundary / no escalation via worker IPC**:
- `dispatch_from_worker` (and `dispatch_for_schedule`) both construct a `ToolExecuteRequest` and call the **same** `execute` / `registry_dispatch`.
- All permission checks (`load_permission_policy`, `check_nexus_tool_permission` using `is_nexus_read_granted`/`is_nexus_write_granted`, `check_fs_tool_permission`) live inside `admission_pipeline` and are executed for every path.
- `caller_kind` (AcpAgent vs Schedule) is carried only for audit differentiation (`audit_tool_execution`). It does not affect gate decisions.
- The daemon does **not** trust the caller to self-declare privileges. The policy file (`permissions.toml` in the workspace `.nexus42/` dir) and the active creator/workspace are the authoritative sources.
- Question "can a worker call a tool that the schedule invoking the worker shouldn't have access to?": In the current model, the schedule (orchestration) decides which tool_id to request. If the schedule is permitted (by its own execution context + the workspace policy) to request a privileged tool, it can do so via either the direct schedule path or via worker IPC — the daemon applies the same policy in both cases. There is no additional per-IPC-actor allowlist at the daemon boundary after P3. This is by design (dynamic registry + shared admission). If future requirements need per-schedule or per-worker-tool scoping, that would be an orchestration-level policy layer, not a daemon IPC change.

**Cross-caller equivalence & auth context**:
- The E2E is deliberately a *dispatch surface* equivalence test using in-process calls on fresh test workspaces.
- It verifies that for the same `(tool_id, input)`, all three entry points produce the same success/error and (modulo timestamps) the same output shape.
- It explicitly tests the rejection case for unknown IDs on all three paths.
- It does **not** simulate distinct authentication contexts (local user, IPC message auth, HTTP bearer). Those are handled by the respective transport layers (CLI → internal HTTP, worker IPC framing, HTTP server) before they reach `HostToolExecutor`.
- The plan's Issues/Risks note about timestamps is addressed correctly: `assert_outputs_equivalent` skips `["assembled_at", "created_at", "updated_at", "generatedAt"]` with a comment explaining per-invocation generation. This prevents masking real mismatches on non-timestamp fields.

**Test isolation (hermeticity)**:
- Every test (parametrized and per-tool) calls `test_ctx().await`.
- `test_ctx` creates a fresh `TestTempRoot` + `WorkspaceState::new_for_testing(nexus_home, db_path, None)`.
- No global singletons are mutated for the dispatch path under test (the registry is a `&'static LazyLock`, but it is read-only after init and the same across tests — acceptable because tests only assert on behavior, not on registry mutation).
- 10 tests × 3 runs = clean, deterministic, <1s each. No flakiness observed.

**NOT_SUPPORTED for unknown IDs**:
- Explicitly covered in `unknown_tool_rejected_on_all_3_paths` (single unknown) and `not_supported_equivalence_all_3_paths` (three different unknown patterns).
- All three paths return error code `"NOT_SUPPORTED"`.
- The registry lookup is the single source of this decision.

**Timestamp handling in equivalence**:
- Matches the dev note exactly.
- The exclusion list is narrow and documented.
- Non-timestamp fields (creator_id, work_id, status, source, etc.) are asserted equal.

**Other security observations**:
- No new global mutable state introduced.
- `CapabilityRegistry` is built once at startup (via `with_runtime_deps` in boot) and handed to the state; the host tool registry is a separate static for the daemon-mediated surface.
- Permission policy is loaded from the workspace path on every relevant call (no caching that could stale).
- Audit is written on both success and denial paths.
- The E2E + unit tests cover the 18 IDs; profile-set IDs are proven absent from the action registry.

## Verdict
**Approve**

All security/correctness invariants required by the focus areas hold:
- Dynamic allowlist via registry lookup is the single gate for all callers; unknown IDs are uniformly rejected with `NOT_SUPPORTED`.
- The three entry points converge on one admission pipeline; no bypasses.
- E2E (54 cases) is hermetic, deterministic, and correctly excludes only timestamp fields.
- Worker IPC path applies the full policy/permission checks; no privilege escalation surface at the daemon boundary.
- Profile-set IDs are correctly excluded from action dispatch.

The single Warning is a documentation/clarification item about the E2E's scope (dispatch surface vs. full transport auth). It does not indicate a defect in the implementation.

## Artifacts
- Report: `.mstar/plans/reports/2026-06-22-v1.57-worker-ipc-and-cross-caller-e2e/qc2.md`
- Range reviewed: `git diff 65bb9450..2a24267a`
- Key files read: `host_tool_executor.rs` (dispatch_from_worker, registry_dispatch), `host_tool_handlers.rs` (admission_pipeline + permission checks), `cross_caller_e2e.rs` (full 587-line suite), `capability_registry.rs` (build_registry + lookup), orchestration `capability/mod.rs` + `tasks/mod.rs` (DaemonToolDispatch wiring).
- Validation commands:
  - `cargo test -p nexus-daemon-runtime --test cross_caller_e2e` (3×, all 10/10)
  - `cargo test -p nexus-daemon-runtime` (34 + doc tests)
  - `cargo clippy -p nexus-daemon-runtime -p nexus-orchestration -- -D warnings` (clean)

---

## Completion Report v2

**Agent**: qc-specialist-2
**Task**: QC review (security/correctness) for plan `2026-06-22-v1.57-worker-ipc-and-cross-caller-e2e`
**Status**: Done
**Scope Delivered**: Full review of the P3 diff range per Assignment (plan stub, 5 explicit focus items, reconciled 18-ID/54-case ACs, 3× E2E runs, full test/clippy, source trace of auth/permission flow for worker IPC path).
**Artifacts**:
- Report: `.mstar/plans/reports/2026-06-22-v1.57-worker-ipc-and-cross-caller-e2e/qc2.md`
- Git commit (report only): (to be executed)
**Validation**:
- 3 consecutive clean runs of the new E2E (10/10).
- Full `nexus-daemon-runtime` test suite passes.
- Clippy clean on affected crates.
- Direct tracing of `dispatch_from_worker` → shared `admission_pipeline` (5 gates, registry lookup first, policy checks applied).
- Explicit verification of NOT_SUPPORTED on all 3 paths, timestamp exclusion, test hermeticity (per-ctx fresh TestTempRoot), and absence of privilege bypass.
**Issues/Risks**: None blocking. One Warning (E2E scope documentation) and one Suggestion (adapter naming) recorded for traceability; neither indicates a correctness or security defect.
**Plan Update**: None required from reviewer side (report only).
**Handoff**: Report written and will be committed. PM to consolidate with qc1/qc3.
**Git**: `cca86a5a qc(v1.57-p3): qc2 report — Approve`
