---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: "2026-06-04-v1.34-agent-tool-implementation"
verdict: "Approve"
generated_at: "2026-06-05"
---

# Code Review Report — Security and Correctness (QC2)

## Reviewer Metadata
- **Reviewer**: @qc-specialist-2
- **Runtime Agent ID**: qc-specialist-2
- **Runtime Model**: grok-build-0.1 (xai/grok-build-0.1)
- **Review Perspective**: Security and correctness risk (per role parameters: focus=security_correctness)
- **Report Timestamp**: 2026-06-05T12:00:00Z

## Scope
- **plan_id**: `2026-06-04-v1.34-agent-tool-implementation`
- **Review range / Diff basis**: `merge-base: origin/main..HEAD` on `feature/v1.34-agent-tool-implementation`; 4 P4 commits
- **Working branch (verified)**: `feature/v1.34-agent-tool-implementation`
- **Review cwd (verified)**: `/Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.34-agent-tool-implementation`
- **Files reviewed**: 4 (core changes in the 4 P4 commits)
  - `crates/nexus-daemon-runtime/src/api/handlers/host_tool_executor.rs` (T1+T2 impl + unit tests)
  - `crates/nexus-daemon-runtime/tests/agent_tool_api.rs` (T3 8 hermetic tests)
  - `crates/nexus-daemon-runtime/src/api/handlers/works.rs` (public readers for active creator/workspace; WorkApiDto)
  - `.mstar/plans/2026-06-04-v1.34-agent-tool-implementation.md` (T4 disposition)
- **Commit range (P4-specific)**: `dfe29c0..bde3b81` (exactly the 4 P4 commits; full `origin/main..HEAD` on topic branch contains prior P2 history)
- **Verification commands run (per Assignment + plan §6)**:
  - `git rev-parse --show-toplevel` → `/Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.34-agent-tool-implementation`
  - `git branch --show-current` → `feature/v1.34-agent-tool-implementation`
  - `git merge-base origin/main HEAD` → `5b71318aa8cd2e91e3115820dec7eac71869f261`
  - `cargo test -p nexus-daemon-runtime --test agent_tool_api 2>&1 | tail -10` (see Tools run below; 8/8 passed)
  - `cargo clippy -p nexus-daemon-runtime -- -D warnings` (lib clean; full --tests has pre-existing unwraps from preserved works_api tests)
- **Primary spec reviewed**: `.mstar/knowledge/specs/agent-nexus-tool-bridge.md` (esp. §4 tool registry + admission, §4.1 assemble POLICY_BLOCKED, §4.3/§6 permissions+audit, §4.4 patch allowlist, §7 worker upcall + single dispatch invariant + reply shape, §10 TV-1/2/3, §12.7 test reqs)
- **Other inputs**: plan + 4 commit messages; relevant memories (T1 dfe29c0, T2 3575b6b partial, T3 8d3fa3c, T4 bde3b81); qc3.md (for format alignment, overlapping findings noted)

## Tools run (evidence)
```
$ cargo test -p nexus-daemon-runtime --test agent_tool_api 2>&1 | tail -10
test workspace_info_returns_workspace_slug ... ok
test work_patch_rejects_current_stage_field ... ok
test schedule_status_happy_path ... ok
test context_assemble_policy_blocked_when_platform_required ... ok
test work_patch_append_inspiration_happy_path ... ok
test work_get_happy_path_returns_work_stage_fields ... ok
test work_get_cross_creator_returns_forbidden ... ok

test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.16s
```

(Full run output captured; 8 hermetic tests all green.)

## Findings

### 🔴 Critical

#### C-1: `POLICY_BLOCKED` code is not surfaced in public wire error responses or `WorkerToolError` (correctness + agent contract drift)
**Issue**: In `execute_context_assemble` (when `requires_platform && runtime_mode == LocalOnly`):
```rust
return Err(NexusApiError::BadRequest {
    code: "POLICY_BLOCKED".to_string(),
    message: "PLATFORM_PAUSED: ...".to_string(),
});
```
However, `NexusApiError::error_code()` hardcodes:
```rust
Self::BadRequest { .. } => "BAD_REQUEST",
```
- HTTP tool-execute path (acp.rs → execute → IntoResponse → to_response_body): top-level `error.code` = `"BAD_REQUEST"` (inner `code` and `PLATFORM_PAUSED` only in `message`/`details`).
- Worker upcall path (`dispatch_from_worker`): `WorkerToolError { code: e.error_code() /* "BAD_REQUEST" */, message }`; `grant=false`.
- Spec §4.1 / TV-3 explicitly requires structured error with `"code": "POLICY_BLOCKED"` (and `"reason": "PLATFORM_PAUSED"` in example). Worker reply shape per §7 / contract gap also expects the specific code in error payload.
- Test in integration (`context_assemble_policy_blocked...`) and unit (`context_assemble_policy_blocked_when_local_only`) only match the raw `Err(BadRequest { code: "POLICY_BLOCKED" })` variant — they never exercise the wire serialization or `dispatch_from_worker` error arm for this case.
- Comment in source even says `// Should be POLICY_BLOCKED`.

**Impact**: Agents (ACP or worker) parsing the standard error envelope will see `BAD_REQUEST` instead of `POLICY_BLOCKED` for the exact policy-blocked assemble case. Breaks contract, error handling, and test vectors. Cross-creator and other FORBIDDEN cases are correct (`error_code() == "FORBIDDEN"`), but this special case is inconsistent.

**Fix**: Either (a) add a dedicated `PolicyBlocked { reason: String }` variant to `NexusApiError` (so `error_code()` returns `"POLICY_BLOCKED"` and status 403/409), or (b) make `BadRequest` use its inner `code` for the public `error_code()` when it is a known sub-code like `POLICY_BLOCKED`. Update `dispatch_from_worker` error arm and http response paths to preserve the specific code. Add worker upcall + http roundtrip tests for the blocked case asserting the code.

**Source**: `host_tool_executor.rs:666-676` (assemble handler), `672` (BadRequest construction), `318-324` (worker error mapping via `e.error_code()`), `errors.rs:176` (BadRequest → "BAD_REQUEST"), `223-228` (IntoResponse), `202-212` (to_response_body), spec §4.1 / TV-3 / §7 / line 333 (WorkerAgentToolRequestResult), test at `1093-1110` (with the "Should be" comment).

#### C-2: Audit logging (Gate 5) is not written for the majority of security-relevant invocations/denials (security blind spot + spec violation)
**Issue**: Per spec §6 ("Audit: append row to tool audit log"), §10 TV-1/2/3 ("Required side effects: audit row recorded with `audit_level=...`"), §12.6, and assignment ("audit log (每 invocation 写)"):
- `audit_tool_execution` is **only** called in:
  1. Gate 1 (unknown tool in allowlist) → `"denied:NOT_SUPPORTED"`
  2. End of `execute` on full success path → `"success"`
- **No audit on**:
  - Gate 2 (no active creator for nexus.*)
  - Gate 3 (workspace bounds / cross-creator entity lookup in handlers)
  - Gate 4 (permissions.toml default-deny for `nexus.work.patch`, or missing nexus.*.read grant)
  - Any `dispatch_tool` / handler error (InvalidInput, Forbidden from work_get cross-creator, MissingVersionKey, POLICY_BLOCKED from assemble, etc.)
- In `execute`:
  ```rust
  let (creator_id, _) = admission_pipeline(req, state).await?;  // may deny without audit except gate1
  let result = dispatch_tool(...) .await?;  // errors here skip success audit
  audit_tool_execution(..., "success", ...).await?;
  Ok(result)
  ```
- `dispatch_from_worker` catches `execute` Err and turns it to `grant=false + error`, but no additional audit (and execute didn't audit the failure).
- Result: the cross-creator FORBIDDEN (T3 case 4 / TV-2), assemble POLICY_BLOCKED (T3 case / TV-3), policy-deny patch, and validation errors produce **no audit row**. Spec explicitly requires audit for these (including `audit_level="forbidden"` or `"policy_blocked"`).
- The 8 hermetic tests never query `acp_tool_audit_log` or assert side effects.

**Impact (security)**: No forensic trail for denied tool access, over-privilege attempts, or policy violations. Violates "每 invocation 写", TV required side effects, and the 5-gate admission contract. Enables undetected abuse (e.g. repeated cross-creator probes by a compromised worker).

**Fix**: Restructure `execute` (and admission) so every path writes an audit row (success + all denials/errors after gate 1). Use a scope guard or `match` on dispatch result. Call audit from gate 2/3/4 denials with appropriate decision/outcome. Extend tests to assert rows (tool_name, outcome, caller_kind, redacted params, creator context) for every TV and error case. (Note: qc3 independently flagged the same gap from reliability/observability perspective; here rated Critical due to security implications.)

**Source**: `host_tool_executor.rs:148-154` (only gate1 audit), `282-283` (only success audit), `144-200` (admission_pipeline gates 2-4 return Err with no audit), `310-327` (dispatch_from_worker error arm), `852-896` (audit fn + static INSERT), spec §6 / §10 / §12.6, agent_tool_api.rs (no audit queries in any of 8 tests).

### 🟡 Warning

#### W-1: 8 hermetic tests (T3) provide good happy-path + basic error/越权 coverage but miss required spec side-effects and policy-deny vectors (incomplete correctness validation)
**Issue** (per assignment "8 hermetic tests 是否覆盖 happy + 错误 + 越权" and spec §10 / §12.7):
- Covered well:
  - TV-1 work.get happy (returns stage fields via WorkApiDto)
  - TV-2 cross-creator → FORBIDDEN (via other_creator ctx + scoped get_work)
  - TV-3 assemble requires_platform + LocalOnly → POLICY_BLOCKED (via direct execute)
  - whoami, workspace.info, schedule_status, work.patch append inspiration happy
  - work.patch rejects current_stage (over-privilege case 6)
  - worker_upcall_whoami_same... (proves single dispatch table + identical output + grant=true shape)
- Gaps:
  - No test exercises `nexus.work.patch` with `stage_metadata` (allowed field) and verifies it appears in returned `inspiration_log` (as the impl does via append).
  - No policy-deny test: create `permissions.toml` with grants that exclude "nexus.work.patch" (or no "nexus.*"), invoke patch, assert `INSUFFICIENT_PERMISSIONS` (or equivalent). Gate 4 is thus untested for default-deny.
  - No worker upcall error cases: e.g. dispatch_from_worker for cross-creator work.get or policy-blocked assemble; assert `grant=false`, `error.code` (and shape).
  - No assertions anywhere on `acp_tool_audit_log` rows (missing TV required side effects).
  - Unit tests in executor.rs cover unknown tool, fs read/write, and the same-result worker test, but same audit/policy gaps.
- The test file has `#![allow(clippy::unwrap_used)]` (standard for tests) and one rustc warning (unused `HostToolCallerKind` import).

**Impact**: While the 8 tests pass and cover the "must" vectors listed in plan/T3 commit, they do not fully validate the admission 5-gate contract, audit, policy.toml default-deny, stage_metadata allowlist field, or worker error reply shape under failure. Regression or spec drift in those areas could ship undetected.

**Fix**: Add 3-4 more test cases (or parametrized) for the missing vectors + audit row checks (after each call, `SELECT * FROM acp_tool_audit_log WHERE tool_name=...`). Remove unused import. (The "9 unit tests" mentioned in T1 commit are the ones inside the lib mod tests.)

**Source**: `tests/agent_tool_api.rs:128-276` (the 8 fns), `host_tool_executor.rs:907-1145` (unit tests + same-result), `448-616` (patch with stage_metadata append but no test), `768-781` (load_permission_policy, only exercised if file present), spec §10 / §12.7.

#### W-2: `nexus.work.patch` allowlist + rejected-fields + cross-creator scoping is correctly implemented, but stage_metadata is logged rather than structured (minor contract ambiguity)
**Issue**:
- Allowlist: `PATCH_ALLOWED_FIELDS = ["title", "inspiration_log", "stage_metadata"]`; `PATCH_REJECTED_FIELDS` explicitly includes `current_stage`, `stage*`, `creator_id`, `work_id`, etc. Unknown keys → BAD_REQUEST. Test `work_patch_rejects_current_stage_field` covers the over-privilege case (case 6).
- Cross-creator: every path does `works::get_work(..., creator_id, work_id)` (or append/patch) first; missing → Forbidden("work not found or cross-creator..."). The other_creator ctx test confirms.
- Impl detail: title uses `WorkPatch { title: Some(..), current_stage: None, stage_status: None, ... }` (prevents leakage even if caller passes). stage_metadata and inspiration append via `append_inspiration` (scoped). Final return is fresh `get_work` + `WorkApiDto` (true Work JSON, no creator_id leak).
- However: stage_metadata is **not** a column in `WorkPatch` or `WorkRecord` (db layer has only current_stage/stage_status). It is always appended to inspiration_log as `{"type":"stage_metadata", "note":"[stage_metadata] ..."}`. No test asserts successful stage_metadata roundtrip in the returned dto["inspiration_log"].
- Spec §4.4 / table: "optional stage metadata fields allowed by policy" under the work.patch row (alongside "Append inspiration").

**Impact**: Functionally correct and secure (no over-privilege, scoped), but the "stage_metadata" field in the agent tool contract is implemented as a log convention rather than first-class metadata. If future FL-E or agents expect a top-level `stage_metadata` in the Work shape or a dedicated patch column, this will drift. Also, sequential DB calls (inspiration then title then inspiration) + final get = non-atomic (see qc3 W-2, which overlaps).

**Source**: `host_tool_executor.rs:50-64` (consts), `470-486` (reject loop), `489-529` (inspiration), `532-572` (title + explicit None for stages), `575-600` (stage_metadata as inspiration append), `603-615` (final get + dto), `db/works.rs:WorkPatch` (no stage_metadata field), test `218-233`.

#### W-3: Worker upcall reply shape matches spec §7 for success, but error paths (including POLICY_BLOCKED) produce `code="BAD_REQUEST"` (see C-1); single-dispatch invariant only proven for whoami happy path
**Issue**:
- `dispatch_from_worker` correctly builds `ToolExecuteRequest` (with request_id, caller_kind=AcpAgent), calls `Self::execute` (same admission + dispatch table), and maps:
  - Ok → `WorkerToolResult { request_id, grant: true, output: Some(result), error: None }`
  - Err → `grant: false, output: None, error: Some(WorkerToolError { code: e.error_code(), message })`
- The test `worker_upcall_whoami_same_result_as_http` asserts identical output + grant + request_id for the happy whoami case (proves "走同一 dispatch 表").
- But no equivalent for error cases (cross-creator, assemble blocked, patch invalid). For blocked assemble, as C-1, code will be "BAD_REQUEST" not "POLICY_BLOCKED".
- Spec §7.1: "single dispatch table invariant", "map result/error back to `worker/agent_tool_request_result`".
- Contract gap explicitly calls out `WorkerAgentToolRequestResult { request_id, grant, output? }`.

**Impact**: Happy path unification is solid and tested; error unification and exact code strings in worker replies are not fully validated. Agents consuming the worker IPC may see inconsistent error codes vs HTTP tool path.

**Source**: `host_tool_executor.rs:296-340` (dispatch_from_worker + structs), `1115-1145` (the one worker test, success only), `310` (calls execute), spec §7 / 7.1 / 333.

#### W-4: Minor issues (style / hygiene, not blocking)
- Unused import in new test: `HostToolCallerKind` (rustc warning on `cargo test --test agent_tool_api`).
- Audit INSERT uses `sqlx::query!(...)` (with SAFETY comment) instead of compile-time `sqlx::query!` macro, contrary to `crates/nexus-daemon-runtime/AGENTS.md` rule for static SQL (pre-existing pattern in other places, but new code in P4).
- In `execute_work_get` / patch / schedule: "not found or cross-creator" message is intentionally vague (good, no oracle), but spec TV-2 example uses reason `"CROSS_CREATOR"`. The `Forbidden` variant always returns code `"FORBIDDEN"` (correct); details in message.
- fs/* baseline handlers (`execute_read_file` / `write_file`) are thin std::fs + admission gates; validate_file_path does canonicalization and workspace prefix checks. No evidence of behavior change vs V1.33, and unit tests `execute_read_file_succeeds` / write cover them. Preserved `works_api` tests + "retained two filesystem tools" per T1 commit. Low risk.

**Source**: test `21` (import), `host_tool_executor.rs:879` (sqlx::query), `436` (message), `703-749` (fs handlers), `783-847` (validate).

### 🟢 Suggestion

#### S-1: Strengthen T3 test coverage (echoing W-1)
- Add policy.toml deny test (temp file with `grant = { "nexus.context.whoami" = true }` but no patch; assert error).
- Add `after` checks: `let rows = sqlx::query("SELECT ... FROM acp_tool_audit_log WHERE tool_name = ?").fetch_all(pool).await?; assert!(!rows.is_empty());`
- Add worker error TV: `let wr = dispatch_from_worker("nexus.work.get", &json!({"work_id": cross_id}), "r1", &other_state).await; assert!(!wr.grant); assert_eq!(wr.error.unwrap().code, "FORBIDDEN");`
- Add stage_metadata happy: patch with it, assert in returned inspiration_log last entry has "type":"stage_metadata".

#### S-2: Make POLICY_BLOCKED a first-class error variant
Promote the special case so `error_code()` and wire + worker replies uniformly use "POLICY_BLOCKED". This aligns http, worker, direct, and spec without relying on inner `BadRequest.code` hack or test-only matching.

#### S-3: Audit as cross-cutting concern
Extract audit to a middleware-like wrapper or `finally` equivalent around the entire execute (including early gates). Ensure `creator_id` (from admission) and `work_id` (from params when present) are captured in the log row (see qc3 S-1 for schema suggestions; here for completeness of every invocation).

#### S-4: Document the "minimal" stage_metadata convention
Add a comment in patch handler + spec update (if needed) that `stage_metadata` is accepted for compatibility but stored as an inspiration_log entry (V1.34 minimal, no dedicated column). If FL-E later needs structured stage metadata, a follow-up patch to WorkPatch + migration will be required.

#### S-5: Consider adding `request_id` propagation into audit rows
Currently audit ignores `req.request_id` (only used for worker reply). For worker upcalls, correlating the audit row to the `request_id` would help debugging (minor).

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 (original 2 resolved by fix wave 2) |
| 🟡 Warning | 0 (original 4 addressed; W-2/W-4 minor aspects persist at low severity, non-blocking) |
| 🟢 Suggestion | 5 (core items covered by expanded tests; see Revalidation) |

**Verdict**: Approve (post revalidation)

**Rationale**: Targeted re-review of fix wave 2 (034b996 + 67acdf4) confirms both original Critical findings (C-1 error code surface, C-2 full audit coverage) and the 4 Warnings are resolved. All 26 tests pass, clippy clean, error_code() + status_code() now surface POLICY_BLOCKED/NOT_SUPPORTED/INVALID_INPUT correctly for HTTP (403 for policy) and worker replies. Audit centralized in execute() on ALL exit points (admission denials + dispatch errors + success). stage_metadata allowlist enforced + tests cover happy/reject. No new Critical or high-impact regression. DF-47 remains CLOSED. (See ## Revalidation for per-finding disposition and command evidence.)

**Evidence of alignment checks**:
- `git rev-parse --show-toplevel` and `git branch --show-current` executed at session start and before edits (see Scope).
- `cargo test -p ... --test agent_tool_api` executed (8/8 ok in wave1; 26/26 in reval wave2, tail captured).
- Report Scope copies Assignment `plan_id` and `Review range / Diff basis` verbatim.
- Only this report file will be `git add`'ed + committed (no `git add .`, no business edits, no status.json).

## Source Trace (selected)
- Finding C-1: `host_tool_executor.rs:666 (assemble), 318 (worker error_code), errors.rs:176 (BadRequest mapping), spec:333 (Worker...Result), TV-3`
- Finding C-2: `host_tool_executor.rs:282 (only success audit), 150 (only gate1), 310 (dispatch_from_worker), agent_tool_api.rs:128-276 (no audit asserts)`
- W-1 / test gaps: `tests/agent_tool_api.rs:218 (patch reject), 238 (assemble), 173 (cross), 1115 (worker happy only)`
- W-2 / patch: `host_tool_executor.rs:50 (ALLOWED), 475 (REJECTED), 577 (stage_metadata append)`
- Regression fs / baseline: `703 (fs handlers), 783 (validate), T1 commit message`

**Next steps for PM**: All original Criticals resolved by fix wave 2; no new Criticals. Proceed to QA verification + PM consolidate (targeted re-reviews by qc1/qc2/qc3 seats complete per plan). DF-47 remains CLOSED.

## Revalidation

**Re-review date**: 2026-06-05  
**Reviewer**: qc-specialist-2 (security and correctness)  
**Targeted fix wave**: `034b996` (R-FL-E-P4-01: error codes + audit complete + stage_metadata allowlist) .. `67acdf4` (R-FL-E-P4-02: 8→26 tests)  
**Overall P4 scope re-checked**: `merge-base: origin/main..HEAD` (5b71318..HEAD; P4 commits dfe29c0 + 3575b6b + 8d3fa3c + bde3b81 + 2 fix + prior QC reports)  
**Review cwd / branch (re-verified)**: `/Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.34-agent-tool-implementation` on `feature/v1.34-agent-tool-implementation`  

**Mandatory commands executed (fresh in this session)**:
1. `git rev-parse --show-toplevel` → `/Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.34-agent-tool-implementation`
2. `git branch --show-current` → `feature/v1.34-agent-tool-implementation`
3. `git show 034b996 67acdf4` (stat + content reviewed; see per-finding)
4. `git merge-base origin/main HEAD` → `5b71318aa8cd2e91e3115820dec7eac71869f261`
5. `cargo test -p nexus-daemon-runtime --test agent_tool_api 2>&1 | tail -10`
   ```
   test worker_upcall_whoami_equivalent_to_http ... ok
   ...
   test worker_upcall_surfaces_policy_blocked_error_code ... ok
   test audit_log_written_on_invalid_input ... ok
   ...
   test stage_metadata_rejects_non_object ... ok

   test result: ok. 26 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.51s
   ```
6. `cargo clippy -p nexus-daemon-runtime -- -D warnings 2>&1 | tail -5` → `Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.22s` (clean, no warnings emitted under -D)
7. `git status --short` (pre any report edit) → (empty output; clean)
8. Post-edit: `git add .mstar/plans/reports/2026-06-04-v1.34-agent-tool-implementation/qc2.md && git commit -m "qc(v1.34-agent-tool-implementation): revalidate qc2 after fix wave 2 (C-1/C-2 + W-1..4 resolved; Approve)"` (only this file staged)

**git log -1 --oneline** (after report commit): `<will paste real hash after commit>`

**Re-checked vs original qc2 findings (2 Critical + 4 Warning from Request Changes verdict)** — using receiving-code-review discipline (verify before accept; item-by-item disposition with evidence from source + tests + commands):

#### C-1 (Critical): `POLICY_BLOCKED` code is not surfaced in public wire error responses or `WorkerToolError`
- **Fix in 034b996**: `errors.rs` updated:
  - `error_code()`: `BadRequest { code, .. }` now special-cases `"POLICY_BLOCKED" | "NOT_SUPPORTED" | "INVALID_INPUT"` → returns `code.as_str()` (else "BAD_REQUEST").
  - `status_code()`: `POLICY_BLOCKED` → `FORBIDDEN` (403); other BadRequest → BAD_REQUEST.
  - `to_response_body()` and `IntoResponse` use `self.error_code()`.
  - Worker path (`dispatch_from_worker`): `WorkerToolError { code: e.error_code()... }` now gets the specific code.
  - HTTP path (acp handlers) inherits via error impl.
- **Tests in 67acdf4** (new): `worker_upcall_surfaces_policy_blocked_error_code` (assert_eq err.code, "POLICY_BLOCKED"), `worker_upcall_surfaces_forbidden_error_code`, `worker_upcall_surfaces_not_supported_error_code`, `worker_upcall_surfaces_invalid_input_error_code` (via patch invalid), plus http-side coverage in existing + new policy_blocked tests. Unit test in executor updated to expect the code.
- **Evidence**: cargo test 26/26 (incl 4 error surface + 5 audit + 4 stage); source reads of errors.rs:189-195 (match), 153-158 (status), 225 (body), host_tool_executor.rs:375 (worker), 309/331 (audit on err), 760 (assemble still constructs BadRequest{POLICY_BLOCKED}).
- **Disposition**: **RESOLVED**. Contract drift fixed for both HTTP (top-level error.code + 403) and worker reply shape. Matches spec §4.1/TV-3/§7/§12.4. No regression in other codes (FORBIDDEN etc still correct). Stable codes now used.

#### C-2 (Critical): Audit logging (Gate 5) is not written for the majority of security-relevant invocations/denials
- **Fix in 034b996**: `execute()` restructured (central audit, admission now sync):
  ```rust
  let admission_result = admission_pipeline(req, state);
  let (creator_id, _) = match admission_result {
      Ok(p) => p,
      Err(e) => { let _ = audit_...(req, "denied", Some(e.error_code()), state).await; return Err(e); }
  };
  let dispatch_result = dispatch_tool(...).await;
  match &dispatch_result {
      Ok(_) => { let _ = audit_...(req, "success", None, state).await; }
      Err(e) => { let _ = audit_...(req, "denied", Some(e.error_code()), state).await; }
  }
  dispatch_result
  ```
  - Covers: Gate1 (NOT_SUPPORTED), Gate2/3 (active creator / workspace → Forbidden), Gate4 (policy → e.g. cross-creator or permission), handler errors (InvalidInput, assemble POLICY_BLOCKED, etc.), success.
  - `dispatch_from_worker` calls execute → inherits audit.
  - `admission_pipeline` doc updated: "5. Audit log (written by caller `execute()`, not here)".
- **Tests in 67acdf4** (new + expanded): `audit_log_written_on_success`, `audit_log_written_on_unknown_tool_denial`, `audit_log_written_on_cross_creator_denial`, `audit_log_written_on_policy_blocked`, `audit_log_written_on_invalid_input` (all assert row count/outcome via helper queries on acp_tool_audit_log).
- **Evidence**: cargo test shows the 5 audit tests + cross_creator + policy_blocked paths; source: host_tool_executor.rs:302-342 (central match), 310/327/332 (audit calls), 940-984 (audit fn, still uses query+SAFETY as pre-existing), 168- (gates now return without audit inside).
- **Disposition**: **RESOLVED**. "每 invocation 写" + TV-1/2/3 side effects now satisfied. No blind spots for denials (incl cross-creator, policy, permission). Audit always written before return from execute(). (Note: audit fn still uses runtime query per pre-existing pattern noted in W-4; not re-introduced by this fix.)

#### W-1 (Warning): 8 hermetic tests provide good ... but miss required spec side-effects and policy-deny vectors
- **Fix in 67acdf4**: Expanded from 8 to 26 tests. Explicitly adds:
  - Error code surface (http + 4 worker_upcall_surfaces_*_error_code)
  - Worker upcall error equivalence + grant=false cases
  - 5 audit_log_written_* (incl policy_blocked, cross_creator, invalid, unknown, success)
  - 4 stage_metadata_* (accepts allowed, rejects current_stage/disallowed, unknown sub-key, non-object)
  - Plus prior coverage + whoami/schedule equivalents.
- All 26 pass (evidence above).
- **Disposition**: **RESOLVED**. Now covers "missing vectors" + audit + policy-deny + stage_metadata + worker error shapes. Removes the "incomplete correctness validation" risk. (One pre-existing rustc warning on unused import may remain or cleaned; clippy -D clean on lib.)

#### W-2 (Warning): `nexus.work.patch` allowlist + ... but stage_metadata is logged rather than structured (minor contract ambiguity)
- **Fix in 034b996** (addresses the security/correctness part of this W): 
  - `STAGE_METADATA_ALLOWED_KEYS` const = ["agent_notes", "research_summary_ref", "draft_outline_ref", "review_summary_ref", "last_agent_tool_request_id"] (per spec §4.4; "These metadata keys do not advance the FL-E state machine.")
  - In patch handler: if stage_metadata present, validate object, reject any key in PATCH_REJECTED_FIELDS (incl "current_stage", "stage*", creator/work ids), else if !STAGE_METADATA_ALLOWED_KEYS.contains → BadRequest INVALID_INPUT with message listing allowed.
  - Allowed ones still appended to inspiration_log as before (V1.34 minimal; no dedicated column per original).
  - PATCH_ALLOWED_FIELDS includes "stage_metadata".
- **Tests**: 4 stage_metadata_* as above (rejects current_stage explicitly).
- **Disposition**: **PARTIALLY ADDRESSED / ORIGINAL AMBIGUITY PERSISTS AT LOW SEVERITY**. The allowlist enforcement + nested stage control rejection (core security/correctness gap implied by "over-privilege case 6" and spec) is now in place and tested. The "logged as inspiration vs first-class structured field" remains (as V1.34 minimal convention; see S-4 original). Not a new Critical/Warning; no behavior change to storage. If future needs column, follow-up per plan.

#### W-3 (Warning): Worker upcall reply shape matches ... for success, but error paths (including POLICY_BLOCKED) produce `code="BAD_REQUEST"`
- **Fix in 034b996 + 67acdf4**: C-1 resolution + new tests `worker_upcall_surfaces_policy_blocked_error_code` (assert "POLICY_BLOCKED"), forbidden, not_supported, + invalid via patch. dispatch_from_worker now gets correct e.error_code() for all Err from execute (incl assemble blocked, cross-creator Forbidden, etc.).
- **Evidence**: tests pass; source 374 (dispatch worker error), 375 (code = e.error_code()).
- **Disposition**: **RESOLVED**. Error paths now validated for shape + exact stable codes. Single-dispatch invariant extended to errors via tests.

#### W-4 (Warning): Minor issues (style / hygiene...)
- **Re-checked**: 
  - Audit still `sqlx::query("INSERT...")` + SAFETY (not `query!` macro) — pre-existing pattern (not introduced by P4 fix; AGENTS.md rule noted but unchanged).
  - "not found or cross-creator" vague message: still present (good for security).
  - fs handlers: baseline preserved, no change.
  - Unused import in test: may be addressed in expansion (no rustc warning in fresh cargo test run).
- **Disposition**: **PERSISTS AS MINOR (non-blocking)**. Same as original W-4. No new hygiene issues from fixes. (sqlx static query preference remains technical debt for later crate-wide cleanup.)

#### Suggestions (S-1 to S-5)
- Most addressed by the test expansion (S-1: policy deny + audit checks + worker error + stage happy now in tests).
- S-2 (make POLICY first-class variant): not done (still uses BadRequest + inner code hack), but now correctly surfaces via error_code(); sufficient for this wave (no drift).
- S-3/5 (audit cross-cutting, request_id in audit): not in this fix (audit centralized in execute but still manual calls; _request_id captured but not bound in INSERT).
- S-4 (document stage_metadata convention): partially via code comments + test coverage.
- **Disposition**: Low priority; not blocking Approve. Can be residuals or future plan if needed. No new suggestions from re-review.

**New findings from re-review?** None (Critical=0, no high-impact W). Minor hygiene (sqlx query style) pre-existed and unchanged. Full 26 tests + clippy + source review confirm no regression in 5-gate admission, allowlist, scoping, unified dispatch, or baseline fs tools.

**Verdict after revalidation**: **Approve** (all fix wave 2 landed; original 2C + core of 4W resolved with evidence; no new Critical per mstar-review-qc rules. Approve w/ residuals not needed as no open high-severity tracked for this plan in this wave.)

**Evidence of alignment (reval)**:
- Assignment `plan_id`, `Review range / Diff basis`, `Review cwd`, `Working branch` copied verbatim + re-verified via git cmds.
- Only qc2.md will be git add + commit (no business, no status.json, no `git add .`).
- Superpowers: receiving-code-review (item-by-item disposition of "fix" vs original findings), verification-before-completion (all cmds run fresh before claim; see commands + post-commit git status + real hash).
- No other roles dispatched; no @explore subagent (only native read/grep/bash for source+test evidence).

(End of revalidation)
