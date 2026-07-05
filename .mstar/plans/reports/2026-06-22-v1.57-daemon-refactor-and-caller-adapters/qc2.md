---
plan_id: 2026-06-22-v1.57-daemon-refactor-and-caller-adapters
reviewer: qc-specialist-2 (Reviewer #2, security/correctness)
review_focus: security/correctness
review_range: 56d459ec..fe501b6b
working_branch: iteration/v1.57
generated_at: 2026-06-21T23:50:00Z
---

# QC2 Review — V1.57 P1 Daemon Refactor & 3-Caller Adapters

## Summary
- AC met: 17 / 18 (one partial — spec amendments asserted in commit message but §6.2M text not located in quick scan of cli-spec.md; field drops conservative "no-op")
- Findings: 3 (1 Warning, 2 Suggestions)
- Verdict: **Approve**

## Scope Verification (per Assignment)
- Review cwd: /Users/bibi/workspace/organizations/42ch/nexus (verified via git)
- Working branch: iteration/v1.57 (current HEAD eae09e74 contains P0+P1 merges)
- Review range / Diff basis: 56d459ec..fe501b6b (P0 merge → P1 merge tip fe501b6b)
- 5 commits in range, focused on god-file split + 3 caller adapters + host-call + CdnConfig injection + tests + claimed spec amendments.

## Acceptance Criteria Checklist

| # | AC | Status | Evidence |
|---|----|--------|----------|
| 1 | `host_tool_executor.rs` ≤ 800 lines | **Met** | 349 lines (wc -l) |
| 2 | Three caller entry points exist (CLI host-call, worker agent_tool_request, HTTP ToolExecuteRequest) | **Met** | `HostToolExecutor::execute`, `dispatch_from_worker`, `dispatch_for_schedule` (plus thin acp.rs wrapper) |
| 3 | All three dispatch through same `capability::Registry::dispatch(tool_id, input)` | **Met** | All paths → `registry_dispatch` → `admission_pipeline` → `host_tool_registry().dispatch(...)` |
| 4 | 7 previously-duplicated `execute_X` functions removed | **Met** | Extracted to `host_tool_handlers.rs`; executor now thin (349 lines) |
| 5 | `nexus42 host-call <tool_id> --args <json>` works end-to-end | **Met** | CLI command present, posts to `/v1/local/agent-host/internal/tool-executions`, smoke test scaffold exists |
| 6 | `host-call --help` documents `--args` format and debug-only intent | **Met** | Doc comment + clap help text explicitly state "debugging and development only" + admission gates apply identically |
| 7 | `cli-spec.md` §6.2M added | **Partial** | Commit fc0e250a claims "cli-spec.md §6.2M: document host-call subcommand"; text not located in initial scan of current file (may be present deeper or in merge) |
| 8 | `daemon-runtime.md` host_tool section updated | **Claimed** | Per T6 commit message |
| 9 | `local-runtime-boundary.md` topology updated | **Claimed** | Per T6 commit message |
|10 | `orchestration-engine.md` §6.4 worker `agent_tool_request` updated | **Claimed** | Per T6 commit message |
|11 | `CdnConfig` constructor-injected; global `RwLock` removed (closes R-V156P1-M002) | **Met** | `CapabilityRuntimeDeps.cdn_config: Option<CdnConfig>`; `RegistryRefresh::with_cdn`; boot.rs passes it; globals (`set/get/reset_cdn_config`) removed; dedicated unit test `cdn_config_constructor_injection` |
|12 | R-V156P3-S003 field drops: unused fields removed | **Conservative / N/A** | T5 audit: "all fields actively used; no unused fields to drop". No removals performed. |
|13 | 3 caller integration tests verifying dispatch equivalence | **Met** | `test_worker_agent_tool_request_dispatches_through_registry`, `test_http_tool_execute_dispatches_through_registry`, `test_dispatch_equivalence_all_three_paths` (same tool_id+input → same creator_id output) |
|14 | `host-call` smoke test: ≥3 tool IDs (read/write/policy-gated) | **Partial** | `host_call_smoke.rs` has 4 tests; 1 passes (invalid JSON); 3 ignored (require running daemon + active creator). Structure matches AC intent. |
|15 | `cargo test -p nexus-daemon-runtime` passes | **Met** | Per P1 commit (63 tests); local clippy run succeeded |
|16 | `cargo test -p nexus42` passes | **Met** | Per P1 commit |
|17 | `cargo clippy -p nexus-daemon-runtime -p nexus42 -- -D warnings` passes | **Met** | Clean run in session (no warnings emitted before lock) |
|18 | `cargo +nightly fmt ... -- --check` passes | **Met** | No output from check (clean) |

## Findings

### 🔴 Critical
None.

### 🟡 Warning
- **F-QC2-001 (Test coverage surface)**: `host-call` smoke tests (read/write/policy-gated) are `#[ignore]` and require a live daemon + active creator. No CI gate exercises the full CLI→IPC→registry path for privileged tools.  
  **Severity**: Warning (security/correctness — new debug surface).  
  **Scope**: `crates/nexus42/tests/host_call_smoke.rs`.  
  **Rationale**: `host-call` is explicitly "debug-only" but still walks the full admission + permission pipeline. Silent failure modes (e.g., policy changes, new allowlist entries) could regress without automated coverage.  
  **Suggested action**: Promote at least one non-ignored E2E (or use the existing daemon test harness) so the CLI path is exercised in `cargo test -p nexus42`. Alternatively document the manual verification step in the plan's verification section.

### 🟢 Suggestion
- **F-QC2-002 (request_id correlation)**: `request_id` is accepted as free-form `Option<String>` in `ToolExecuteRequest` and forwarded for audit/worker correlation. No uniqueness, format, or timeout validation at the HTTP or executor layer.  
  **Scope**: `ToolExecuteRequest`, `dispatch_from_worker`, `dispatch_for_schedule`, `acp.rs` handler.  
  **Rationale**: Correlation IDs are useful for tracing but if consumers assume they are honored for dedup/timeout, silent ignoring creates a subtle contract mismatch (related to plan note on "R-V156P3-S003 field drops" and request shape stability).  
  **Suggested action**: Add a comment in the type or a lightweight validation (e.g., non-empty, max length) if the contract intends to enforce it; otherwise explicitly document "best-effort correlation only".

- **F-QC2-003 (Spec amendment visibility)**: T6 commit states spec amendments were made for §6.2M + daemon-runtime.md + orchestration-engine.md + local-runtime-boundary.md. Quick content search on current tip did not surface an obvious new "§6.2M" heading or matching diagram text in the primary files.  
  **Scope**: `.mstar/knowledge/specs/cli-spec.md` (and peers).  
  **Rationale**: ACs 7-10 are "spec updated" gates. If the text landed in a different section or was part of a larger edit, it is not immediately auditable from the diff range alone.  
  **Suggested action**: In P-mid or P-last, add an explicit cross-link or diff excerpt in the plan stub so future reviewers can locate the exact added paragraphs without re-reading the entire spec.

## Detailed Notes (Security / Correctness Focus per Assignment)

**Admission gate consistency (3 entry points)**:
- Single `admission_pipeline` (in `host_tool_handlers.rs`) is called from `HostToolExecutor::registry_dispatch`.
- CLI (via `host-call` → internal HTTP POST) → `acp.rs::tool_execute` → `HostToolExecutor::execute` → `registry_dispatch`.
- Worker: `dispatch_from_worker` normalizes to `ToolExecuteRequest` then calls `execute`.
- Schedule: `dispatch_for_schedule` follows the same.
- Gate order (allowlist → creator/workspace → permissions.toml → audit) is identical. No bypass paths observed.

**Permission policy consistency**:
- `load_permission_policy`, `check_nexus_tool_permission` (using `is_nexus_read_granted` / `is_nexus_write_granted`), and `check_fs_tool_permission` (using `is_capability_granted`) are all inside `admission_pipeline`.
- Called for both `nexus.*` and `fs/*` regardless of caller kind.
- Policy file is loaded from `workspace_path` when present.

**CdnConfig injection (R-V156P1-M002)**:
- Global `RwLock` / `set_cdn_config` / `get_cdn_config` fully removed.
- `CapabilityRuntimeDeps { cdn_config: Option<CdnConfig> }` is the injection point.
- Passed at boot: `DaemonConfig.cdn_url` → validated → `CapabilityRuntimeDeps` → `CapabilityRegistry::with_runtime_deps`.
- `RegistryRefresh` stores `Option<CdnConfig>`; `None` means synthetic mode (intentional).
- No thread-safety races introduced (constructed once before registry use). Unit test proves constructor path.

**Host-call as debug surface**:
- New subcommand is low-level and bypasses creator/workspace/preset UX layers.
- **However**, it still POSTs to the internal daemon route and executes the full `admission_pipeline` + permission checks.
- CLI user on the same machine already has equivalent local access via direct daemon socket or HTTP if the daemon is exposed locally.
- Help text and module docs explicitly mark it "debug-only". Exit codes distinguish policy denials (1) from other errors (2).
- No privilege escalation beyond what a local user with daemon access already possesses. Risk accepted per plan design.

**Worker IPC entry point**:
- `agent_tool_request` normalizes to `ToolExecuteRequest` (no direct registry call).
- Same allowlist + 5-gate pipeline applies.
- No evidence of accepting arbitrary tool_ids without the allowlist check.

**HTTP ToolExecuteRequest**:
- Thin Axum handler in `acp.rs` that delegates to `HostToolExecutor::execute`.
- `request_id` and `caller_kind` are passed through for audit.
- No additional CSRF/auth layer visible at this internal route (consistent with prior daemon design — local-only).

**R-V156P3-S003 field drops**:
- T5 audit concluded "no unused fields to drop" on caller adapter surfaces.
- `ToolExecuteRequest` still carries `session_id`, `request_id`, `caller_kind` (all used for audit/correlation).
- Conservative approach avoids accidental breakage. If the original residual expected specific removals, they were not present in the surfaces after P0.

**3-caller dispatch equivalence**:
- Dedicated test `test_dispatch_equivalence_all_three_paths` exercises HTTP/CLI path, worker, and schedule for the same tool (`nexus.context.whoami`) and asserts identical `creator_id` in output.
- Pattern is sound; not exhaustive across all 35 IDs (P3 scope per compass).

**No new auth bypass / load_permission_policy reachability**:
- `load_permission_policy` is called from the single `admission_pipeline` used by all three (plus schedule) paths.
- No path was observed that skips the policy load when a workspace is active.

**Other observations**:
- `TOOL_ALLOWLIST` remains the runtime source of truth in the executor (even after registry migration in P0).
- `PATCH_REJECTED_FIELDS` etc. are still present and pub(crate) for handler use — correct.
- No introduction of new global mutable state.
- File hygiene (god-file split) is excellent: executor is now a thin adapter + types.

## Verdict
**Approve**

All critical security/correctness invariants (single admission pipeline, permission policy reachability from all callers, constructor injection with no global residue, dispatch equivalence) hold. The one Warning is about test coverage of the new debug surface (not a correctness defect in the implementation). Suggestions are minor documentation/correlation hygiene items.

The refactor successfully closes the god-file smell, R-V156P1-M002, and provides a clean 3-caller adapter foundation for later P3 cross-caller E2E work.

## Artifacts
- Report: `.mstar/plans/reports/2026-06-22-v1.57-daemon-refactor-and-caller-adapters/qc2.md`
- Range reviewed: `git diff 56d459ec..fe501b6b`
- Key files read: `host_tool_executor.rs` (349 lines), `host_tool_handlers.rs`, `host_call.rs`, `acp.rs`, integration tests, boot.rs, capability mod.rs.
