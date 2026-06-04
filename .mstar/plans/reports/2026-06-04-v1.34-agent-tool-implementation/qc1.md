---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-06-04-v1.34-agent-tool-implementation"
verdict: "Request Changes"
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
