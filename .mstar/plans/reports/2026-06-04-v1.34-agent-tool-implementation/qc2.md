---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: "2026-06-04-v1.34-agent-tool-implementation"
verdict: "Request Changes"
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
| 🔴 Critical | 2 |
| 🟡 Warning | 4 |
| 🟢 Suggestion | 5 |

**Verdict**: Request Changes

**Rationale**: Two Critical findings (C-1: contract drift on POLICY_BLOCKED error code for both HTTP and worker paths; C-2: audit logging does not cover every invocation/denial as required by spec and assignment, creating a security observability blind spot for cross-creator, policy-blocked, and permission cases). These are correctness and security risks that must be addressed before merge. The 8 tests are a good start (all pass, cover the listed TV + over-privilege + same-dispatch proof for happy path) and the 5-gate admission + allowlist + scoping + patch restrictions are implemented correctly in the happy/error paths that are exercised. Overlaps with qc3 (audit, atomicity) noted but evaluated independently under security/correctness lens. No changes to business code were made.

**Evidence of alignment checks**:
- `git rev-parse --show-toplevel` and `git branch --show-current` executed at session start and before edits (see Scope).
- `cargo test -p ... --test agent_tool_api` executed (8/8 ok, tail captured).
- Report Scope copies Assignment `plan_id` and `Review range / Diff basis` verbatim.
- Only this report file will be `git add`'ed + committed (no `git add .`, no business edits, no status.json).

## Source Trace (selected)
- Finding C-1: `host_tool_executor.rs:666 (assemble), 318 (worker error_code), errors.rs:176 (BadRequest mapping), spec:333 (Worker...Result), TV-3`
- Finding C-2: `host_tool_executor.rs:282 (only success audit), 150 (only gate1), 310 (dispatch_from_worker), agent_tool_api.rs:128-276 (no audit asserts)`
- W-1 / test gaps: `tests/agent_tool_api.rs:218 (patch reject), 238 (assemble), 173 (cross), 1115 (worker happy only)`
- W-2 / patch: `host_tool_executor.rs:50 (ALLOWED), 475 (REJECTED), 577 (stage_metadata append)`
- Regression fs / baseline: `703 (fs handlers), 783 (validate), T1 commit message`

**Next steps for PM**: Address C-1/C-2 (and W-1) in a targeted fix commit on this branch, then targeted re-review by seats that raised Critical (per mstar-review-qc). Re-run the exact test command + `cargo test -p nexus-daemon-runtime --test agent_tool_api` + clippy after fixes. DF-47 remains CLOSED per T4 (unification complete in the surface provided).
