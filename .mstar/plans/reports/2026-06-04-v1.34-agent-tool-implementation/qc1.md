---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-06-04-v1.34-agent-tool-implementation"
verdict: "Approve w/ residuals"
generated_at: "2026-06-05"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Runtime Model: openai/gpt-5.5
- Review Perspective: Architecture coherence and maintainability risk
- Report Timestamp: 2026-06-05

## Scope
- plan_id: 2026-06-04-v1.34-agent-tool-implementation
- Review range / Diff basis: merge-base: origin/main..HEAD on feature/v1.34-agent-tool-implementation; 4 P4 commits: dfe29c0 T1 — agent tool registry + 6 nexus.* handlers + 2 fs baseline; 3575b6b T2 — worker upcall unified to single dispatch table; 8d3fa3c T3 — 8 hermetic agent-tool API tests; bde3b81 T4 — DF-47 CLOSED disposition
- Working branch (verified): feature/v1.34-agent-tool-implementation
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.34-agent-tool-implementation
- Files reviewed: 6 (`host_tool_executor.rs`, `acp.rs`, `works.rs` P4 diff, `agent_tool_api.rs`, plan, spec)
- Commit range: exact P4 implementation diff reviewed as `dfe29c0^..bde3b81`; required branch diff evidence gathered with `git diff --stat $(git merge-base HEAD origin/main)..HEAD`. Local `HEAD` also already contains sibling QC report commits (`qc2.md`, `qc3.md`), so code findings below are anchored to the four assigned P4 commits.
- Tools run: `git rev-parse --show-toplevel`; `git branch --show-current`; `git log --oneline -5`; `git diff --stat $(git merge-base HEAD origin/main)..HEAD`; `git show --stat --oneline --name-only dfe29c0 3575b6b 8d3fa3c bde3b81`; `git diff --stat dfe29c0^..bde3b81`; `grep` for worker/tool dispatch references; `cargo clippy -p nexus-daemon-runtime -- -D warnings 2>&1 | tail -10`

## Findings

### 🔴 Critical

- **C-1 — DF-47 is closed without production worker-upcall wiring.** T2 adds `HostToolExecutor::dispatch_from_worker()` and the unit test `worker_upcall_whoami_same_result_as_http`, but a repository search for `agent_tool_request` / `dispatch_from_worker` finds only this helper and tests. No worker IPC / orchestration entrypoint calls it, so `worker/agent_tool_request` is not actually mapped into the registry as required by spec §7 and §12.1. This is only a partial shared adapter, not end-to-end upcall unification. **Fix:** wire the real worker upcall handler to `HostToolExecutor::dispatch_from_worker()` (or equivalent normalized `ToolExecuteRequest` path), add an integration test at that boundary, and only then keep DF-47 closed. If that wiring is intentionally deferred, reopen/register DF-47.

- **C-2 — Stable tool error codes are declared but not used by HTTP/worker responses.** `ToolErrorCode` / `ToolExecuteError` exist in `host_tool_executor.rs`, but handler failures return generic `NexusApiError` variants. `POLICY_BLOCKED` and `NOT_SUPPORTED` are carried inside `NexusApiError::BadRequest { code, ... }`, while `NexusApiError::error_code()` still returns `BAD_REQUEST`; `dispatch_from_worker()` serializes `e.error_code()`, so worker errors lose the spec §5 / §12.4 code. The same issue affects HTTP error bodies through the generic API error response. Tests currently inspect the internal `BadRequest.code` field and miss the wire contract. **Fix:** centralize tool errors in the registry response path (or make `BadRequest` expose its stable `code` in tool responses), and add HTTP + worker assertions for `POLICY_BLOCKED`, `NOT_SUPPORTED`, `FORBIDDEN`, and `INVALID_INPUT` wire codes.

### 🟡 Warning

- **W-1 — The five-gate admission pipeline does not reliably audit denied outcomes.** `HostToolExecutor::execute()` writes audit rows only on success, and `admission_pipeline()` writes one denial only for unknown tool ids. Active-creator failures, permission denials, workspace/entity denials, invalid inputs, handler failures, and `POLICY_BLOCKED` return before `audit_tool_execution()`. This violates spec §4.3 gate 5 and §12.6 (`allowed/denied`, error code/reason) and weakens maintainability because audit behavior is split between admission and success handling. **Fix:** make `execute()` wrap admission+dispatch with a single outcome/audit finalizer so every return path records one redacted audit row with the stable tool error code.

- **W-2 — `nexus.work.patch` accepts arbitrary `stage_metadata` keys instead of the §4.4 allowlist.** Top-level rejected fields are blocked, but any JSON under `stage_metadata` is accepted and appended into `inspiration_log` as text. Spec §4.4 allows only policy-approved metadata keys (`agent_notes`, `research_summary_ref`, `draft_outline_ref`, `review_summary_ref`, `last_agent_tool_request_id`) that do not advance FL-E state. The current shape would accept nested `stage_metadata.current_stage` or unrelated capability-grant data, and it does not persist stage metadata as such. **Fix:** validate `stage_metadata` as an object with the exact approved key allowlist, reject nested state/routing fields, and either persist it in the intended metadata surface or narrow the P4 contract/tests to inspiration-only.

### 🟢 Suggestion

- **S-1 — Split the registry module after the blocking contract fixes.** `host_tool_executor.rs` is now >1,100 lines and mixes DTOs, admission, policy parsing, dispatch, handlers, fs path validation, audit SQL, and tests. A small follow-up split (`registry.rs`, `admission.rs`, `audit.rs`, `handlers/*`) would make the single-dispatch invariant easier to preserve without changing behavior.

- **S-2 — Move active creator/workspace readers out of `works.rs`.** P4 makes `read_active_creator_id()` and `read_active_workspace_slug()` public from the Works handler module. They are workspace identity helpers, not Work-specific API behavior; placing them under a small workspace/identity utility would avoid future handlers depending on `works.rs` for non-Work concerns.

## Source Trace

- Finding ID: C-1
  - Source Type: git-diff + content search + manual reasoning
  - Source Reference: `host_tool_executor.rs:288-328`, `host_tool_executor.rs:1113-1145`, `grep agent_tool_request|dispatch_from_worker` (only helper/test references), spec §7 / §12.1 / §12.7
  - Confidence: High
- Finding ID: C-2
  - Source Type: code review + contract check
  - Source Reference: `host_tool_executor.rs:114-130`, `host_tool_executor.rs:672-676`, `host_tool_executor.rs:317-325`, `api/errors.rs:162-180`, spec §5 / §12.4
  - Confidence: High
- Finding ID: W-1
  - Source Type: code review + spec check
  - Source Reference: `host_tool_executor.rs:144-200`, `host_tool_executor.rs:266-285`, `host_tool_executor.rs:851-896`, spec §4.3 / §12.6
  - Confidence: High
- Finding ID: W-2
  - Source Type: code review + spec check
  - Source Reference: `host_tool_executor.rs:50-64`, `host_tool_executor.rs:575-600`, spec §4.4
  - Confidence: High

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 2 |
| 🟡 Warning | 2 |
| 🟢 Suggestion | 2 |

**Verdict**: Request Changes

## Revalidation

### Scope revalidated

- Targeted fix wave 2 range: `034b996..67acdf4` (2 commits).
- Overall P4 basis retained from assignment: `merge-base: origin/main..HEAD`.
- Verified checkout:
  - `git rev-parse --show-toplevel` → `/Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.34-agent-tool-implementation`
  - `git branch --show-current` → `feature/v1.34-agent-tool-implementation`
- Diff evidence reviewed:
  - `git show --stat --patch 034b996 67acdf4`
  - `grep agent_tool_request|dispatch_from_worker` across `*.rs`
  - Direct reads of `api/errors.rs`, `host_tool_executor.rs`, and `tests/agent_tool_api.rs`

### Fix wave 2 findings disposition

- **C-1 — Still open / Request Changes.** `034b996` documents `dispatch_from_worker()` as the adapter and explicitly says the worker-side IPC caller in `nexus-orchestration` is deferred. `67acdf4` adds worker-path tests for the adapter (`worker_upcall_surfaces_policy_blocked_error_code`, `worker_upcall_whoami_equivalent_to_http`, `worker_upcall_schedule_status_equivalent_to_http`), but repository search still finds `dispatch_from_worker` only in `host_tool_executor.rs` and tests; there is no production worker/orchestration call site for `worker/agent_tool_request`. This does not resolve the original architecture finding that DF-47 was closed without production upcall wiring. Acceptable resolution remains either (a) wire the production caller, or (b) reopen/register the deferred worker IPC integration instead of treating DF-47 as closed end-to-end.
- **C-2 — Resolved.** `NexusApiError::error_code()` now surfaces canonical tool bridge codes for `BadRequest` (`POLICY_BLOCKED`, `NOT_SUPPORTED`, `INVALID_INPUT`) and preserves `FORBIDDEN` via the existing `Forbidden` variant. `status_code()` maps `BadRequest { code: "POLICY_BLOCKED" }` to 403. Worker error serialization uses `e.error_code()`, and tests cover `POLICY_BLOCKED`, `NOT_SUPPORTED`, `FORBIDDEN`, and `INVALID_INPUT` paths.
- **W-1 — Resolved.** Audit logging is centralized in `HostToolExecutor::execute()`: admission denials, dispatch/handler failures, and success paths each call `audit_tool_execution()` with stable error codes where applicable. Fix wave 2 tests cover success, unknown-tool denial, cross-creator denial, policy-blocked denial, and invalid-input denial.
- **W-2 — Resolved for the P4 contract.** `stage_metadata` is now required to be an object; nested stage-control keys such as `current_stage` are rejected; unknown metadata keys are rejected; and only the spec §4.4 allowlist (`agent_notes`, `research_summary_ref`, `draft_outline_ref`, `review_summary_ref`, `last_agent_tool_request_id`) is accepted. The value is still recorded as an `inspiration_log` metadata entry, but the P4 implementation comment explicitly narrows this minimal persistence surface and the unsafe state-transition surface is blocked.
- **S-1 — Still suggested / non-blocking.** `host_tool_executor.rs` remains a large mixed-responsibility module. The fix wave correctly avoided an unrelated split; keep this as follow-up maintainability debt after blocking contract scope is closed.
- **S-2 — Still suggested / non-blocking.** Active creator/workspace readers still live under `works.rs`. This remains a small architecture cleanup suggestion and is not required to resolve fix wave 2.

### Verification evidence

- `git log --oneline -10` showed fix commits at HEAD: `67acdf4 test(daemon): R-FL-E-P4-02 expand hermetic tests 8→26 covering all QC findings` and `034b996 fix(daemon): R-FL-E-P4-01 surface stable error codes + audit all paths + stage_metadata allowlist`.
- `cargo test -p nexus-daemon-runtime --test agent_tool_api 2>&1 | tail -10`:

```text
test audit_log_written_on_cross_creator_denial ... ok
test worker_upcall_surfaces_not_supported_error_code ... ok
test worker_upcall_surfaces_policy_blocked_error_code ... ok
test workspace_info_returns_workspace_slug ... ok
test worker_upcall_whoami_equivalent_to_http ... ok
test work_get_cross_creator_returns_forbidden ... ok
test worker_upcall_surfaces_forbidden_error_code ... ok

test result: ok. 26 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.59s
```

- `cargo clippy -p nexus-daemon-runtime -- -D warnings 2>&1 | tail -10`:

```text
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.23s
```

**Revalidation Verdict**: Request Changes — C-2, W-1, and W-2 are resolved, S-1/S-2 remain non-blocking suggestions, but original C-1 remains unresolved because fix wave 2 still lacks a production worker/orchestration caller for the worker upcall path or an explicit reopened/deferred DF-47 residual.

## Revalidation 2

### Scope revalidated

- Targeted fix wave 3 commit: `e604a4f` (`R-FL-E-P4-05`), doc-only.
- Overall P4 basis retained from assignment: `merge-base: origin/main..HEAD`.
- Verified checkout:
  - `git rev-parse --show-toplevel` → `/Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.34-agent-tool-implementation`
  - `git branch --show-current` → `feature/v1.34-agent-tool-implementation`
- Diff evidence reviewed:
  - `git show --stat --name-status e604a4f`
  - `git show --format=fuller --no-ext-diff e604a4f`

### Fix wave 3 disposition

- **Doc-only scope confirmed.** `e604a4f` modifies exactly three harness documentation files: `.mstar/plans/2026-06-04-v1.34-agent-tool-implementation.md`, `.mstar/knowledge/specs/agent-nexus-tool-bridge.md`, and `.mstar/knowledge/deferred-features-cross-version-tracker.md`. No business implementation or tests changed.
- **C-001 / original C-1 — Resolved by explicit deferral.** The original blocking condition was not that production worker IPC had to be wired inside P4 unconditionally; it was that P4 could not mark DF-47 closed while production caller wiring remained absent. `e604a4f` corrects the disposition:
  - Plan markdown changes `## DF-47 Disposition: **CLOSED**` to `## DF-47 Disposition: **OPEN** (partial unification)` and states production caller wiring belongs to P5 or a future V1.34+ plan.
  - Spec §7.3 now records **Status: OPEN** for the P4 outcome, explains that `HostToolExecutor::execute` / `dispatch_from_worker` shipped only the unified dispatch adapter, and states that no orchestration-side call site invokes it at runtime.
  - Deferred tracker DF-47 row is retained and updated to `P4 shipped adapter; **production caller wiring OPEN** (deferred to P5/future). Remove when wired end-to-end.`
- **Residual state accepted.** DF-47 remains an open deferred feature rather than a falsely closed P4 deliverable. That satisfies the accepted resolution path from the first revalidation: reopen/register the deferred worker IPC integration if production caller wiring is intentionally deferred.
- **No new Critical findings.** The fix is documentation-only and aligns the plan/spec/tracker with the actual implementation boundary; remaining production caller wiring is a tracked residual, not an unresolved blocker for this P4 review.

### Verification evidence

- `git log --oneline -10` showed fix wave 3 at HEAD:

```text
e604a4f fix(daemon): R-FL-E-P4-05 reopen DF-47 — registry dispatch unified, production caller wiring deferred to V1.34+/P5
b30d7fc qc(v1.34-agent-tool-implementation): qc3 revalidation — Approve w/ residuals (fix wave 2: R-FL-E-P4-01/02)
6ab39fb qc(v1.34-agent-tool-implementation): revalidate qc2 after fix wave 2 (C-1/C-2 + W-1..4 resolved; Approve) [hash in text]
91fbdd8 qc(v1.34-agent-tool-implementation): revalidate qc2 after fix wave 2 (C-1/C-2 + W-1..4 resolved; Approve) [final hash]
18c6bd8 qc(v1.34-agent-tool-implementation): revalidate qc2 after fix wave 2 (C-1/C-2 + W-1..4 resolved; Approve) [final hash fill]
5bb60fc qc(v1.34-agent-tool-implementation): revalidate qc2 after fix wave 2 (C-1/C-2 + W-1..4 resolved; Approve) [hash fill]
9a633fc qc(v1.34-agent-tool-implementation): revalidate qc2 after fix wave 2 (C-1/C-2 + W-1..4 resolved; Approve)
a2ec68a qc(v1.34-agent-tool-implementation): qc1 revalidate fix wave 2
67acdf4 test(daemon): R-FL-E-P4-02 expand hermetic tests 8→26 covering all QC findings
034b996 fix(daemon): R-FL-E-P4-01 surface stable error codes + audit all paths + stage_metadata allowlist
```

- `git show e604a4f` confirmed doc-only changes:

```text
M	.mstar/knowledge/deferred-features-cross-version-tracker.md
M	.mstar/knowledge/specs/agent-nexus-tool-bridge.md
M	.mstar/plans/2026-06-04-v1.34-agent-tool-implementation.md
```

- `cargo test -p nexus-daemon-runtime --test agent_tool_api 2>&1 | tail -10`:

```text
test worker_upcall_surfaces_not_supported_error_code ... ok
test work_patch_rejects_current_stage_field ... ok
test worker_upcall_schedule_status_equivalent_to_http ... ok
test worker_upcall_whoami_equivalent_to_http ... ok
test workspace_info_returns_workspace_slug ... ok
test work_get_cross_creator_returns_forbidden ... ok
test worker_upcall_surfaces_forbidden_error_code ... ok

test result: ok. 26 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.47s
```

- `cargo clippy -p nexus-daemon-runtime -- -D warnings 2>&1 | tail -10`:

```text
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.23s
```

**Revalidation 2 Verdict**: Approve w/ residuals — C-001 is fully resolved by reopening DF-47 and documenting the partial P4 outcome in the plan, spec §7.3, and deferred tracker. Production caller wiring remains intentionally deferred and tracked as an open DF-47 residual; no unresolved Critical findings remain for P4.
