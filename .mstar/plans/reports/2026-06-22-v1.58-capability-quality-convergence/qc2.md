---
plan_id: 2026-06-22-v1.58-capability-quality-convergence
reviewer: qc-specialist-2
reviewer_index: 2
focus: security_correctness
review_cwd: /Users/bibi/workspace/organizations/42ch/nexus
working_branch: iteration/v1.58
diff_basis: e6024060..4bfb1399
reviewed_at: 2026-06-22T07:30:27Z
verdict: Approve
---

# QC2 — V1.58 P2 Capability Quality Convergence — Security/Correctness Review

## Summary

P2 is a surgical polish pass addressing residual items from DF-56 (conditional routing) and V1.57-new host-call smoke gaps. Changes are confined to:
- New integration test file exercising real dispatch paths (`conditional_routing_e2e.rs`)
- Tracing instrumentation in `inject_workspace_context` and `inject_registry_refresh_context`
- Spec reconciliation in `.mstar/knowledge/specs/preset-conditional-routing.md`
- Hermetic CLI-side test extraction + documented justification for remaining `#[ignore]` tests in `host_call_smoke.rs`
- Test-contract comment strengthening in `converge_runtime_e2e.rs`
- Minor supporting changes in `tasks/mod.rs` and `host_call.rs`

**No source behavior changes outside tests and tracing.** All new tests pass. Clippy clean on touched crates.

## Findings

### High severity
(none)

### Medium severity
(none)

### Low severity
(none)

## Security/Correctness Properties Verified

### 1. Expression routing integration tests (T3, R-V156P2-M003)
- **File**: `crates/nexus-orchestration/tests/conditional_routing_e2e.rs` (new, 304 LOC)
- **Dispatch path exercised**: `preset::load_preset_from_str(yaml, &caps)` → `loaded.outer_graph.get_task(start_id)` → `task.run(ctx)` → `resolve_expression_target()` → `NextAction::GoTo`
- **No test-only mocks**: Uses real `CapabilityRegistry::with_builtins()`, real loader, real `StateCompositeTask` constructed by the graph builder.
- **Coverage**: 8 tests covering numeric threshold, string equality multi-branch, compound `&&`, registry synthetic fallback, default fall-through.
- **Error paths**: Not explicitly tested in this file (expressions that produce `ExprError` surface as `TaskExecutionFailed` per spec §3.3.2). Pre-existing error-propagation contract in spec remains unchanged.
- **Conclusion**: Real dispatch path; no masking of security-relevant behavior.

### 2. `inject_workspace_context` tracing (T8, R-V156P3-W001)
- **Location**: `crates/nexus-orchestration/src/tasks/mod.rs:1178-1190`
- **Fields emitted** (all `tracing::debug!`):
  - `state_id = %self.id`
  - `source = "hook" | "default"`
  - `conflict_detected = %bool`
  - `changes_applied = %i64`
- **Sensitive data**: None. Default synthetic state (when `self.workspace_state.is_none()`) contains only `session_id: ""`, `conflict_detected: false`, `changes_applied: 0`, `workspace_root: ""`.
- **P0 workspace session data**: The actual sensitive `workspace_state` (if provided via test hook) is **not** logged in full; only the two boolean/count fields are extracted for tracing. The full object is still stored under `__workspace_state` for expression evaluation, but tracing does not serialize or emit it.
- **Conclusion**: No session secret leakage. Tracing is safe.

### 3. `registry.refresh` invocation instrumentation (T9, R-V156P3-W002)
- **Location**: `crates/nexus-orchestration/src/tasks/mod.rs:1104-1141` (inside `inject_registry_refresh_context`)
- **Fields emitted**:
  - `capability = "registry.refresh"`
  - `duration_ms`
  - `status = "ok" | "fallback"`
  - On fallback: `error = %e` (only the error message string)
- **Payloads**: No full registry output, no capability list, no snapshot contents are logged.
- **URL/credential concern**: The only URL-related tracing in the registry surface is pre-existing (P0/P1) in `crates/nexus-orchestration/src/capability/builtins/registry.rs:420`:
  ```rust
  tracing::debug!(cdn_url = %cdn.url, timeout_ms = cdn.timeout_ms, "fetching registry from CDN");
  ```
  This line was **not touched by P2**. P2 only instruments the *invocation wrapper* in the task context injection path. CDN URL sanitization (if required) is a pre-existing concern outside P2 scope.
- **Conclusion**: P2 tracing adds duration + status with no payload leakage. URL tracing is outside the diff.

### 4. Host-call smoke un-ignore (T13, R-V157P1-W001)
- **Files**: `crates/nexus42/tests/host_call_smoke.rs`, `crates/nexus42/src/commands/host_call.rs`
- **Three `#[ignore]` tests retained**:
  - `host_call_smoke_read_tool`
  - `host_call_smoke_write_tool`
  - `host_call_smoke_policy_gated_tool`
- **Justification (module-level doc comment)**:
  - `DaemonClient` is a concrete struct (not behind a trait).
  - `run()` constructs it internally via `DaemonClient::from_config(config)`.
  - Hermetic un-ignore would require either (a) trait extraction + refactor of V1.57 P1 QC-accepted code, or (b) wiremock server.
  - Both exceed P2's "surgical polish" boundary.
  - Daemon-side dispatch is already covered by `nexus-daemon-runtime` integration tests.
  - CLI-side contribution (request envelope construction, arg parsing, error formatting) is now covered by **new hermetic tests** added in the same change:
    - `build_tool_request_read_tool`
    - `build_tool_request_preserves_nested_params`
    - `build_tool_request_handles_bool_params`
    - `host_call_rejects_invalid_json`
- **Mock safety**: The new `build_tool_request` helper only constructs a JSON object. It never talks to a daemon, never bypasses admission gates, never reaches the real `DaemonClient`. It is pure data transformation.
- **Conclusion**: Remaining `#[ignore]` tests have sound, documented justification. Hermetic extraction does not weaken security surface.

### 5. Engine test fidelity (T15, R-V156P2-CACHE-01)
- **File**: `crates/nexus-orchestration/tests/converge_runtime_e2e.rs`
- **Contract comment** (lines 7-25) is explicit and strong:
  > All converge arrivals go through `StateCompositeTask::record_converge_arrival` (the real runtime path, NOT a test-local helper).
  >
  > This is a hard contract for the convergence surface because:
  > 1. `record_converge_arrival` is the only function that writes the `_converge_arrivals_{target}` context key with the correct `HashSet<String>` shape expected by the gate check in `run()`.
  > 2. Bypassing it would test a serialization round-trip, not the actual arrival-recording semantics.
  > 3. The converge gate reads the key back via `context.get::<HashSet<String>>()`.
  >
  > **Any new converge test in this file MUST use the `converge_arrive` helper (which delegates to `record_converge_arrival`). Do not add tests that manually write the converge-arrivals context key.**
- All 11 tests (including the new `converge_no_predecessors_skips_gate`) use `converge_arrive`.
- No bypass paths were introduced or retained.
- **Conclusion**: Real runtime path is enforced by contract + helper discipline.

### 6. Spec overlay (T1/T2/T6)
- **File**: `.mstar/knowledge/specs/preset-conditional-routing.md`
- **T1 (R-V156P2-M001)**: Added "Absent vs null vs empty string" clarification.
  - Absent → `null`; explicit `null` → `null`; `""` → distinct.
  - Recommends `== null` / `!= null` for presence tests.
  - **Tightens** semantics for preset authors; no weakening of security model.
- **T2 (R-V156P2-M002)**: Reconciled 0-predecessor converge.
  - Old spec: "validation error (orphan)".
  - Actual impl + new spec: "gate is skipped, state advances immediately (no validation error)".
  - Rationale: `converge_predecessors.is_empty()` short-circuits the gate check.
  - Still protected by DAG enforcement (no cycles). Orphan converge becomes a pass-through — no privilege escalation or bypass introduced.
- **T6 (R-V156P2-L003)**: Added "Converge timeout" section.
  - Documents current indefinite wait for `wait_for_all`.
  - Explicitly states: "A configurable `wait_for_all_timeout_seconds` ... is planned but deferred — adding it requires schema changes to `ConvergeConfig` (out of scope for P2)".
  - Honest limitation disclosure; does not claim timeout exists.
- **Conclusion**: Spec edits reconcile drift or document limitations. No security guarantees weakened.

## Additional Checks

- **Test execution**:
  - `cargo test -p nexus-orchestration --test conditional_routing_e2e` → 8 passed
  - `cargo test -p nexus-orchestration --test converge_runtime_e2e` → 11 passed
- **Static analysis**: `cargo clippy -p nexus-orchestration -p nexus42 -- -D warnings` → clean (no output = success)
- **No other files touched** in the diff that affect security surface (no auth, no crypto, no filesystem paths, no network client construction changes outside the extracted pure helper).

## Verdict Reasoning

All six scoped items pass review with no Critical or Warning findings. Changes are minimal, well-instrumented, and respect the "surgical polish" intent of P2:
- Real paths are exercised (T3, T15)
- Tracing is safe (T8, T9)
- Mocks do not bypass gates (T13)
- Spec edits are clarifications or honest limitation records (T1/T2/T6)

**Verdict: Approve**

## Cross-Plan Concerns

None identified in this scope. The pre-existing CDN URL tracing in `registry.rs` (P0) is noted but outside P2 diff; any future sanitization requirement would be a separate residual.
