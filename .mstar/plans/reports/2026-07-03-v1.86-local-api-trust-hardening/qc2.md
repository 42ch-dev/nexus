---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: "2026-07-03-v1.86-local-api-trust-hardening"
verdict: "Approve"
generated_at: "2026-07-03"
---

# Code Review Report — V1.86 Local API Trust-Boundary Hardening (qc2)

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: xai/grok-build-0.1
- Review Perspective: SECURITY and CORRECTNESS — lead reviewer for this security iteration (Deep Review)
- Report Timestamp: 2026-07-03

## Scope
- plan_id: `2026-07-03-v1.86-local-api-trust-hardening`
- Review range / Diff basis: `merge-base(main, iteration/v1.86)..iteration/v1.86` (HEAD `b2cdcfd6`); equivalent to `git diff main...iteration/v1.86`
- Working branch (verified): `iteration/v1.86`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus`
- Files reviewed: 20 (diff --stat); primary security surface: `api/mod.rs`, `api/auth_middleware.rs`, `api/handlers/host_tool_handlers.rs`, `api/path_guard.rs`, `api/handlers/host_tool_executor.rs`, `boot.rs`, `host_tool_executor_tests.rs`
- Commit range: `main...b2cdcfd6`
- Tools run: `git rev-parse`, `git diff`, `cargo test -p nexus-daemon-runtime`, manual code inspection + call-graph grep, spec cross-check

**Deep review: triggered (sensitive module: auth/CORS/path-traversal/fs R/W; multi-module; security-critical).**  
**Lenses applied: Security Lens, Auth Lens.**

## Attack Path Verification (Adversarial Pass)

### Attack Path 1 — CORS + keyless-localhost remote reach (T1)
**Status: CLOSED**

**Evidence inspected:**
- `crates/nexus-daemon-runtime/src/api/mod.rs:514-536`: `CorsLayer` now uses `AllowOrigin::list(allowed_origins)` derived from `auth_config.allowed_origins` (same Vec as middleware).
- `api/auth_middleware.rs:220-256` (`require_allowed_origin`):
  - OPTIONS preflight: unconditionally passes to CorsLayer (line 226-228) — correct per §13.1.3.
  - No-Origin: permitted (line 230-253 only acts when header present).
  - Exact string match: `config.allowed_origins.iter().any(|allowed| allowed == origin_str)` (case-sensitive).
- Allowlist construction (`DaemonApiConfig`):
  - Computed own origin from `NEXUS_DAEMON_PORT` or 8420.
  - Hardcoded: `tauri://localhost`, `http://tauri.localhost`, `http://localhost:5173`.
  - Env override via `NEXUS_DAEMON_ALLOWED_ORIGINS` with `HeaderValue::from_str` validation.
- `with_resolved_port` + boot wiring ensures port is consistent.
- Middleware is applied **before** `require_api_key` (defense-in-depth).

**Bypass attempts checked (per assignment):**
- (a) `null` Origin: if sent as literal string "null", exact match fails → 403. No special case needed; browsers sending "null" for file:/data: contexts are correctly rejected.
- (b) Spoof absence of Origin: non-browser clients (CLI, curl, workers) legitimately omit Origin and are allowed (spec §13.1.2). Browsers always send Origin on cross-origin; same-origin navigation omits it (intended).
- (c) Spoofing / malformed Origin:
  - `http://127.0.0.1:8420.evil.com` → no exact match → rejected.
  - `http://127.0.0.1:8420@evil.com` → no exact match → rejected.
  - `http://evil.com` → rejected.
- (d) OPTIONS state-changing hole: no — preflight is handled by CorsLayer; middleware only skips double-reject. No mutation on OPTIONS.
- (e) CorsLayer vs middleware allowlist drift: **identical source** (`auth_config.allowed_origins`). Both paths in `mod.rs` and test `build_router` derive from the same field.
- (f) Threat model coherence (§13.1.4): Origin gate is **independent of and before** auth. Keyless-localhost remains default (non-goal to deprecate). Cross-site Origin is rejected before keyless loopback check. Non-browser clients pass Origin gate then auth as before. Model is coherent for single-user local daemon.

**Tests (assert security property, not just status):**
- `cross_origin_request_is_rejected_with_403` (evil.com → 403, message contains origin).
- `same_origin_request_is_allowed`, Tauri variants, Vite dev, OPTIONS preflight all pass.
- Error body: `code: "forbidden"`, message includes rejected origin + env hint.

**Error leakage:** Origin value echoed in 403 — acceptable (per assignment and spec observability §13.1.5).

**Spec alignment:** §13.1.1–13.1.5 fully matched.

### Attack Path 2 — fs/* bypass without workspace (T2)
**Status: CLOSED**

**Evidence inspected:**
- `host_tool_handlers.rs:82-90` (current `admission_pipeline`, now async):
  ```rust
  let workspace_path_str = workspace_path.unwrap_or_default();
  if workspace_path_str.is_empty() {
      return Err(NexusApiError::Forbidden { ... "fs/* tools require an active workspace with defined bounds" });
  }
  ```
  This is **unconditional** and **first** fs/* gate (after allowlist + creator for nexus.* tools). No path operation or permission check precedes it.
- `execute_read_file` (543) and `execute_write_file` (596) now take `&state` and re-assert `workspace_path()` before calling `resolve_guarded_path_async`. Defense-in-depth.
- All three caller surfaces route through `HostToolExecutor`:
  - HTTP: `acp.rs:27` → `HostToolExecutor::execute`
  - Worker: `dispatch_from_worker` → `execute`
  - Schedule: `dispatch_for_schedule` → `execute`
- `HostToolExecutor::registry_dispatch:232` calls `admission_pipeline(req, state).await` before any registry dispatch or execute_*.
- Grep of `execute_read_file` / `execute_write_file` / `HostToolExecutor::execute` confirms no direct bypass paths in the diff or current tree.

**Tests:**
- `fs_write_rejected_without_workspace`: no workspace → 403 "forbidden", message contains "active workspace".
- `fs_read_rejected_without_workspace`: same.
- Both tests use `create_test_workspace()` (no workspace_path) and assert the exact security property.

**Spec alignment:** §13.2 exact match (unconditional deny in admission, before execute, clear error contract).

### Attack Path 3 — String-prefix path traversal (T3)
**Status: CLOSED**

**Evidence inspected:**
- Old vulnerable code (`abs_requested_str.starts_with(&workspace_path_str)`) removed.
- `validate_file_path` (now async, ~800) delegates to `resolve_guarded_path_async` → `resolve_guarded_path`.
- `execute_read_file`/`execute_write_file` also call `resolve_guarded_path_async(..., must_exist=true/false)`.
- `path_guard.rs:37-101` (`resolve_guarded_path`):
  - Read branch (`must_exist=true`): `canonicalize(joined).starts_with(canonical_root)` (component-wise `Path`).
  - Write branch (`must_exist=false`): walk to nearest existing parent, same `starts_with`.
  - Explicit comment (62-64) calls out the string-prefix anti-pattern.
- T5: `spawn_blocking` wrapper (`resolve_guarded_path_async`) + fs I/O in read/write handlers.

**Bypass attempts verified:**
- Sibling prefix: workspace `.../workspace`, target `../workspace-evil/secret.md` → rejected on both read and write (tests + guard).
- `../` escape: rejected.
- Absolute outside: rejected.
- Symlink escape (unix test): symlink inside pointing outside → rejected on canonicalize + starts_with.
- TOCTOU: window exists (canonicalize root → canonicalize target, plus T5's split resolve vs read). Documented in `path_guard.rs:22-30` and spec §13.3.4 as **pre-existing accepted risk** for single-user local model (`R-V166-QC2-TOCTOU`). Not a new regression introduced by T3/T5. Adversarial multi-user FS access out of scope.

**Tests (T4 coverage):**
- `fs_read_rejects_sibling_prefix_escape`, `fs_write_rejects_sibling_prefix_escape`: assert 403 + file not created.
- `fs_read_rejects_symlink_escape` (unix).
- `fs_read_rejects_relative_escape`, absolute outside, relative inside success.
- Chapter path guard tests (in chapters.rs) also cover the same helper for sibling prefix (shared implementation).

**Spec alignment:** §13.3.1–13.3.5 exact (component-wise requirement, delegation to `resolve_guarded_path`, coverage of read/write + sibling/parent/symlink).

### Additional Correctness / Side-Channel Checks
- **Error info leakage:** fs handlers now map guard `BadRequest` → `Forbidden` with generic "path outside the workspace root" (does not echo full attacker-controlled path). Origin error intentionally echoes (spec + assignment).
- **Async split (T5):** `spawn_blocking` correctly isolates all `canonicalize` + `std::fs` from the async runtime. No new TOCTOU class introduced.
- **Coverage adequacy:** regression tests reproduce the attack (would have passed pre-fix sibling string-prefix and no-workspace paths). Tests assert security semantics, not just HTTP codes.
- **Call-site completeness:** all fs/* execution (HTTP, worker, schedule) goes through the single admission gate. No orphaned direct calls in the reviewed diff or current tree.
- **Spec ↔ impl:** `daemon-runtime.md` §13 is a faithful and accurate reflection of the implemented contract.

## Findings

### 🔴 Critical
- None.

### 🟡 Warning
- None (all three attack paths closed with matching regression tests and spec alignment).

### 🟢 Suggestion
- (Minor) Consider adding an explicit test for literal `Origin: null` header (file:/data: context) to make the "null" case auditable, even though current behavior (exact match → 403) is correct.
- (Observability) The startup log of `allowed_origin_sources` (computed/hardcoded/env) is valuable; consider emitting it at INFO on first protected request when keyless mode is active (already required by spec §13.1.5; implementation does log on config load).

## Source Trace
- Finding ID: QC2-2026-07-03-T1/T2/T3
- Source Type: manual adversarial code review + test reproduction + spec cross-check
- Source Reference: `git diff main...iteration/v1.86`, `cargo test -p nexus-daemon-runtime`, direct reads of `auth_middleware.rs:220`, `host_tool_handlers.rs:82-100 + 543-670`, `path_guard.rs:37-101`, `host_tool_executor.rs:232`, tests at lines 99-135 + 276-344, `daemon-runtime.md` §13
- Confidence: High

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 2 (non-blocking) |

**Attack path 1 (CORS + keyless remote reach): CLOSED** — no bypass found.  
**Attack path 2 (fs/* without workspace): CLOSED** — unconditional deny before any path op; all callers gated.  
**Attack path 3 (string-prefix traversal): CLOSED** — component-wise guard + sibling/escape/symlink tests; TOCTOU is pre-existing documented risk.

**Verdict**: Approve

## Verification Performed
- Cwd/branch: `git rev-parse --show-toplevel` + `git branch --show-current` → repo root + `iteration/v1.86`.
- Full test suite: `cargo test -p nexus-daemon-runtime` → green (all relevant fs/*, auth, and path-guard tests pass).
- No modifications to implementation or test files (leaf reviewer).
- No subagent / Task dispatch performed (per role contract).

## Reproducibility Notes
- To re-run the adversarial T2/T3 tests in isolation: `cargo test -p nexus-daemon-runtime --test host_tool_executor_tests fs_read_rejected_without_workspace fs_write_rejected_without_workspace fs_read_rejects_sibling_prefix_escape fs_write_rejects_sibling_prefix_escape -- --nocapture`.
- Origin tests live in `auth_middleware.rs` test module.
- The three regression tests were confirmed to assert the security property (forbidden + message + side-effect absence) rather than merely a status code.
