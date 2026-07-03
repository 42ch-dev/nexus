---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-07-03-v1.86-local-api-trust-hardening"
verdict: "Approve"
generated_at: "2026-07-03"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Runtime Model: minimax-cn-coding-plan/MiniMax-M3
- Review Perspective: Architecture coherence and maintainability risk
- Report Timestamp: 2026-07-03

## Scope
- plan_id: `2026-07-03-v1.86-local-api-trust-hardening`
- Review range / Diff basis: `merge-base(main, iteration/v1.86)..iteration/v1.86` (HEAD `b2cdcfd6`); equivalent to `git diff main...iteration/v1.86`
- Working branch (verified): `iteration/v1.86`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus` (`git rev-parse --show-toplevel`)
- Files reviewed: 15 source files (4 prod src, 1 test src inside crate, 5 integration tests, plus harness plans/specs/status)
- Commit range: `b2cdcfd6..merge-base(main, iteration/v1.86)` (8 commits incl. merge + harness bookkeeping; net 7 V1.86 work commits)
- Tools run: `cargo clippy -p nexus-daemon-runtime -- -D warnings`, `cargo test -p nexus-daemon-runtime --lib --no-fail-fast`, `cargo test -p nexus-daemon-runtime --test works_api error_envelope fl_e_schedule_api memory_pagination_bounded memory_review_fragments_api sort_contract --no-fail-fast`, `cargo +nightly-2026-06-26 fmt -p nexus-daemon-runtime --all --check`, direct `git diff`/`git log` review, `Grep` for `resolve_guarded_path` + `HostToolExecutor::` callers across crates

## Findings

### 🔴 Critical
_None._

### 🟡 Warning
_None._

### 🟢 Suggestion

- **S-001 · CorsLayer + middleware allowlist could silently diverge on invalid `HeaderValue`** — `api/mod.rs:514-524` filters the Origin list via `HeaderValue::from_str(...).ok()` and silently drops invalid origins from the **CorsLayer** allowlist, while `require_allowed_origin` (`auth_middleware.rs:230-253`) iterates the **full** string `Vec` and treats string-equality as authoritative. In practice both default origins and the env-override path (`resolve_allowed_origins`, `auth_middleware.rs:85-96`) produce HeaderValue-valid strings, so the lists stay consistent today — but there is no single-source guarantee. Cohesion risk only; no current bug. → Suggest centralizing both layers behind one helper that materializes `(Vec<HeaderValue>, Vec<String>)` so they cannot drift.

- **S-002 · `validate_file_path` runs once in admission with `must_exist=false` and `execute_read_file` runs the same guard again with `must_exist=true`** — `host_tool_handlers.rs:97-99` + `host_tool_handlers.rs:563-571`. The two checks differ in intent (cheap pre-flight vs actual fs access) and operate on different inputs (path-shape vs canonicalized target), so the redundancy is justified by the spec's defense-in-depth posture (§4.3) — but the cost is `spawn_blocking { canonicalize() }` running twice per request. Worth a brief ADR/note in the handler. No action needed beyond documentation; the design holds.

- **S-003 · Repeated `state.workspace_path() + is_empty()/ok_or` boilerplate across three fs sites** — `host_tool_handlers.rs:84-90`, `host_tool_handlers.rs:555-560`, `host_tool_handlers.rs:616-621`, `host_tool_handlers.rs:806-812`. Four call sites repeat the same workspace_root resolution + empty check pattern. A small helper (e.g. `fn require_workspace_root(state) -> Result<&Path, NexusApiError>`) would collapse this to one place. Minor; keep if you accept the duplication.

- **S-004 · Allowlist port resolution is duplicated between `DaemonApiConfig::from_env` and `DaemonConfig::resolve_transport`** — `auth_middleware.rs:112-115` reads `NEXUS_DAEMON_PORT` with `unwrap_or(DEFAULT_PORT)`, while `boot.rs:75-82` re-reads the same env var with different fallthrough (CLI port wins unless `port == 8420`). The chain `from_env → with_resolved_port(transport.port)` keeps them aligned at runtime, but the precedence logic is fragile if either side changes. If a future contributor edits one branch without the other, the allowlist could disagree with the bound port silently. Recommend extracting a single `resolve_daemon_port(cli_port: u16) -> u16` and using it in both places.

- **S-005 · The component-wise path-guard fix is **`fs/*`-scoped**; `host_tool_handlers.rs:2155-2161` (manuscript body read) still uses string-prefix** — out of scope for V1.86 (the §13 amendment explicitly covers `fs/*` only), but it is the same anti-pattern V1.86 just removed for `fs/*`. Worth a follow-up residual / spec amendment so the W-002 normalization reaches manuscript-family surfaces in a later iteration. Not a regression (was pre-V1.86).

## Source Trace

- **F-001 (T1 — allowlist composition + middleware layering):**
  - Source Type: git-diff + manual-reasoning
  - Source Reference: `crates/nexus-daemon-runtime/src/api/auth_middleware.rs:31-188` (`DaemonApiConfig` shape, `default_allowed_origins`, `resolve_allowed_origins`, `from_env`, `with_resolved_port`); `crates/nexus-daemon-runtime/src/api/mod.rs:514-536` (CORS layer + middlewares + outermost `attach_request_id`); `crates/nexus-daemon-runtime/src/api/auth_middleware.rs:220-256` (`require_allowed_origin` semantics).
  - Confidence: High.

- **F-002 (T2 — workspace-required admission gate):**
  - Source Type: git-diff + linter (clippy clean) + manual-reasoning
  - Source Reference: `crates/nexus-daemon-runtime/src/api/handlers/host_tool_handlers.rs:83-101` (admission deny); `host_tool_handlers.rs:555-560` + `:616-621` (execute_*_file self-defense); new tests `host_tool_executor_tests.rs:fs_write_rejected_without_workspace`, `fs_read_rejected_without_workspace`.
  - Confidence: High.

- **F-003 (T3 — `resolve_guarded_path` delegation + duplicate code removal):**
  - Source Type: git-diff + manual-reasoning
  - Source Reference: `host_tool_handlers.rs:794-826` (`validate_file_path` new body); `host_tool_handlers.rs:768-786` (`resolve_guarded_path_async` wrapper); `path_guard.rs:37-102` (canonical implementation retained).
  - Confidence: High.

- **F-004 (T4 — sibling-prefix + symlink + worker-IPC coverage backfill):**
  - Source Type: git-diff + manual-reasoning
  - Source Reference: `host_tool_executor_tests.rs` adds `fs_read_rejects_sibling_prefix_escape`, `fs_write_rejects_sibling_prefix_escape`, `fs_read_rejects_symlink_escape` (unix), `fs_write_rejects_symlink_parent_escape` (unix), `worker_fs_read_rejects_escape`.
  - Confidence: High.

- **F-005 (T5 — async conversion + `spawn_blocking`):**
  - Source Type: git-diff + linter + manual-reasoning
  - Source Reference: `host_tool_handlers.rs:37` (`admission_pipeline` → `async fn`), `:543-589` (`execute_read_file` becomes async, wrapped `std::fs::read_to_string` in `spawn_blocking`), `:596-656` (`execute_write_file`), `:773-786` (`resolve_guarded_path_async` wrapper); all four callers (`HostToolExecutor::execute`, `HostToolExecutor::dispatch_from_worker`, `HostToolExecutor::dispatch_for_schedule`, `acp::tool_execute`) use `.await`. Registry wrappers updated: `host_tool_handlers.rs:1152-1167` (`registry_read_file`/`registry_write_file` now `Box::pin(execute_*_file(...))` rather than the previous sync-passthrough double-await pattern).
  - Confidence: High.

- **F-006 (T7 — TOCTOU note refresh):**
  - Source Type: git-diff (doc only)
  - Source Reference: `crates/nexus-daemon-runtime/src/api/path_guard.rs:22-30`; `apps/desktop/src-tauri/src/lib.rs` `guard_path` doc updated in lockstep. Spec note: `knowledge/specs/daemon-runtime.md` §13.3.4. No logic change.
  - Confidence: High.

- **F-007 (Error envelope SSOT):**
  - Source Type: manual-reasoning (grep across the four fs/* + admission/validate paths)
  - Source Reference: all new error returns go through `NexusApiError::Forbidden { resource, reason }` or `NexusApiError::Internal { code, message }`; `NexusApiError::BadRequest` from `resolve_guarded_path` is mapped to `Forbidden { resource: "file" }` in `host_tool_handlers.rs:565-571` and `:626-632` (domain reclassification, not ad-hoc). No ad-hoc JSON bodies found. Per `crates/nexus-daemon-runtime/AGENTS.md` "Error envelope SSOT" rule.
  - Confidence: High.

- **F-008 (Reuse vs duplication):**
  - Source Type: manual-reasoning (`grep resolve_guarded_path`)
  - Source Reference: Single SSOT at `path_guard.rs:37-102`; `host_tool_handlers.rs` delegates via `resolve_guarded_path_async`; `outline.rs` and `chapters.rs` already delegate synchronously. No duplicated path logic was introduced. The only new duplication is the workspace_path bootstrapping block — see S-003.
  - Confidence: High.

- **F-009 (Layer ordering + Spec §13.1.4 alignment):**
  - Source Type: manual-reasoning (axum `.layer` semantics — outer-layer applied last)
  - Source Reference: `api/mod.rs:531-540` — request order is `attach_request_id → require_allowed_origin → cors_layer → routes`. So `attach_request_id` runs first (so every error body carries a request_id), `require_allowed_origin` runs second (cross-origin rejected before CORS preflight resolution and before auth), `cors_layer` runs third (headers + OPTIONS short-circuit), handler runs last. Matches spec §13.1.4. The `OPTIONS` short-circuit at `auth_middleware.rs:226-228` is correctly implemented (returns `Ok(next.run(...))` for preflights so `CorsLayer` is authoritative).
  - Confidence: High.

- **F-010 (R# status.json entries):**
  - Source Type: git-diff on `.mstar/status.json`
  - Source Reference: Both `R-V186-REGRESS-M004` (resolves via commit `42335a16`) and `R-V186-REGRESS-W001` (resolves via commit `0eb9aa4f`) correctly registered under `residual_findings["2026-07-03-v1.86-local-api-trust-hardening"]`, each with `severity: "medium"`, `lifecycle: "resolved"`, `resolution.plan_id` + `resolution.commit` + `reason` ("actually moved to spawn_blocking; prior resolution was insufficient" / "backfilled missing privileged-path coverage; prior resolution was insufficient"). Accurately recorded.
  - Confidence: High.

- **F-011 (CI / format gates):**
  - Source Type: linter
  - Source Reference: `cargo clippy -p nexus-daemon-runtime -- -D warnings` → clean. `cargo test -p nexus-daemon-runtime --lib` → 387 passed / 0 failed. Per-file integration tests → green. `cargo +nightly-2026-06-26 fmt --all --check` → clean.
  - Confidence: High.

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 5 |

**Verdict**: **Approve**

## Notes for the integrator

The V1.86 design holds together from an architecture/maintainability perspective:

- The async-conversion ripple is clean and centralized. `admission_pipeline` → `execute_*` → `resolve_guarded_path_async` is the only path that touches the filesystem; all four external entry points (`HostToolExecutor::execute`, `dispatch_from_worker`, `dispatch_for_schedule`, `acp::tool_execute`) propagate `.await` consistently. No caller was left synchronous. No `execute().await.await`. The registry wrappers for `fs/read_text_file`/`fs/write_text_file` were simplified at the same time (from `Box::pin(async move { sync_result })` to `Box::pin(async_fn)`), eliminating the previous sync-passthrough indirection.
- Defense-in-depth is layered correctly: `request_id` → `require_allowed_origin` → `cors_layer` → `routes` (with `require_api_key` on protected subroutes). The `OPTIONS` short-circuit is correctly placed so `CorsLayer` remains authoritative for preflights.
- `resolve_guarded_path` remains the single source of truth for the component-wise W-002 guard. No duplicated path logic was introduced.
- The two fresh residuals are accurately recorded and resolved; their resolution metadata points back to the right commits and gives a reason ("prior resolution was insufficient"), which matches the regression-of-resolution pattern PM committed to in the plan.

The five Suggestions are low-impact refinements rather than blockers. If PM elects to act on any of them, S-001 and S-004 are the highest-leverage (prevent silent allowlist/port drift); S-005 is a forward-looking pointer to the same anti-pattern in manuscript tools that future iterations should address via a §13 amendment or follow-up residual. S-002 is a documentation note rather than a code change.
