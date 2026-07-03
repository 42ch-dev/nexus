---
plan_id: 2026-07-03-v1.88-frontend-slate-clear-and-reliability-cleanup
reviewer: qc2
reviewer_index: 2
branch: iteration/v1.88
scope: "6 residual closures + tracker hygiene (P0 frontend: nexus-logo*, nexus-mark*, tokens.ts, index.ts; P1 reliability: path_guard.rs + host_tool_handlers.rs + outline.rs + chapters.rs + auth_middleware.rs + mod.rs; T6 desktop-build.yml LFS; T7 deferred tracker)"
status: Approve
generated_at: 2026-07-04T02:01:00Z
tools_run:
  - "cargo test -p nexus-daemon-runtime (full suite: 14 + 17 + doc tests passed)"
  - "cargo clippy --all -- -D warnings (clean)"
  - "cargo +nightly-2026-06-26 fmt --all --check (clean)"
  - "pnpm --filter @42ch/nexus-ui run build/typecheck/test (build + typecheck + 7 tests passed)"
  - "git diff + targeted file reads on path_guard, host_tool_handlers, chapters, outline, auth_middleware, mod.rs, frontend components"
---

# Code Review Report — QC2 (security / correctness focus)

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: grok-build-0.1
- Review Perspective: security / correctness (path-guard semantics, gate ordering, error mapping, Arc<DaemonApiConfig> shared-state risk, new panic paths in async wrapper)
- Report Timestamp: 2026-07-04T02:01:00Z

## Scope
- plan_id: 2026-07-03-v1.88-frontend-slate-clear-and-reliability-cleanup
- Review range / Diff basis: main..iteration/v1.88 (per Assignment + plan frontmatter)
- Working branch (verified): iteration/v1.88
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: path_guard.rs, host_tool_handlers.rs (admission + execute_*), chapters.rs, outline.rs, auth_middleware.rs, api/mod.rs, nexus-logo.tsx, nexus-mark.tsx, tokens.ts, index.ts, nexus-logo.test.tsx, desktop-build.yml, deferred-features trackers
- Commit range: 17cfa6ee (integration merge) and prior P0/P1 commits on the branch
- Tools run: see frontmatter

## Findings

### 🔴 Critical
(none)

### 🟡 Warning
(none)

### 🟢 Suggestion
(none — all targeted items verified clean; no residual hygiene items surfaced under the security/correctness lens)

## Detailed Security / Correctness Verification (QC2 lens)

### 1. Path-guard semantics preserved (in-bounds succeed; sibling-escape / out-of-bounds rejected)
- `path_guard.rs`:
  - `resolve_guarded_path_async` is a thin `spawn_blocking` wrapper around the unchanged canonical `resolve_guarded_path`.
  - New unit tests:
    - `resolve_guarded_path_async_accepts_inside_and_rejects_escape`
    - `resolve_guarded_path_async_rejects_prefix_confusion_sibling` (covers the exact sibling-prefix attack case from V1.86 T4)
  - Both read (`must_exist=true`) and write (`must_exist=false`) paths are exercised for escape cases.
- Call sites (host_tool_handlers, outline, chapters) now route through the async wrapper; error mapping at each site is preserved (see §3).
- `chapters.rs:174` sync probe in `to_detail` is **explicitly out of scope** per plan clarify (lightweight DTO editability check, no FS I/O); sync `resolve_guarded_path` remains available for legitimate sync contexts.
- **Verdict**: semantics identical to pre-V1.88; new regression coverage added.

### 2. Removing Gate 3 path check for `fs/*` does not weaken security
- `host_tool_handlers.rs:97` (admission_pipeline):
  ```rust
  // Gate 3 workspace bounds check intentionally skipped for fs/* tools:
  // execute_read_file / execute_write_file call resolve_guarded_path_async
  // before any FS access, making them the single resolution site.
  ```
- Gate 4 (permissions.toml via `check_fs_tool_permission`) remains unconditionally for fs/* tools (lines 92–95).
- `execute_read_file` (line 564) and `execute_write_file` (line 625) still call `resolve_guarded_path_async(..., must_exist)` **before** any `std::fs::read_to_string` / `std::fs::write` / `create_dir_all`.
- The only callers of `execute_read_file` / `execute_write_file` are the registry dispatch path (`host_tool_executor`); they are not public entry points that bypass admission.
- **Verdict**: relocation of the check, not removal. Single canonicalize path per tool invocation is now enforced at the handler (previously duplicated). No bypass introduced.

### 3. Error code mappings for path-guard rejections unchanged
- All migrated sites preserve the exact pre-migration mapping:
  - `host_tool_handlers.rs:566` (read) and `627` (write):
    ```rust
    .map_err(|e| match e {
        NexusApiError::BadRequest { .. } => NexusApiError::Forbidden { resource: "file", ... },
        other => other,
    })
    ```
  - `outline.rs`: maps `chapter_path_forbidden` → `outline_path_forbidden` (or `?` propagation).
  - `chapters.rs`: maps `chapter_path_forbidden` → caller-provided `forbidden_code` (or `?`).
- The only new error from the async wrapper is `Internal { code: "PATH_GUARD_PANIC" }` on `spawn_blocking` join failure. Every mapping site already has an `other => other` arm, so this surfaces identically to any other non-BadRequest error.
- Existing host_tool_executor_tests (V1.86 T3/T4) plus new T3 regression tests in `host_tool_handlers.rs` (execute_*_accepts_in_bounds / _rejects_escape) continue to assert the 403 Forbidden surface for escape paths.
- **Verdict**: wire-visible error codes for path-guard rejections are identical.

### 4. `Arc<DaemonApiConfig>` introduces no shared-mutable-state risk
- `api/mod.rs`: `create_router` wraps once (`Arc::new(auth_config)`) before the two `route_layer` calls. Callers still pass `DaemonApiConfig` by value; the `Arc` is internal.
- `auth_middleware.rs`:
  - `require_api_key` and `require_allowed_origin` now take `State<Arc<DaemonApiConfig>>`.
  - Access is read-only (`config.auth_mode`, `config.allowed_origins`, `config.as_ref()` for keyed auth). No mutation.
  - `AuthMode::KeyedAll` path passes `config.as_ref()` (shared borrow) — no clone of the full config.
- Test helper (`build_router`) mirrors the same internal wrapping; no production signature change.
- `DaemonApiConfig` is constructed once at startup and never mutated afterward (standard axum State pattern).
- **Verdict**: cheap ref-count clones only; zero risk of shared mutable state or TOCTOU on config.

### 5. No new panic paths or dangerous unwraps in the async wrapper or critical FS paths
- `path_guard.rs:29` — the only `?` after `spawn_blocking` turns a panic into a controlled `Internal { "PATH_GUARD_PANIC" }`.
- `execute_read_file` / `execute_write_file`:
  - Path resolution errors are mapped (no unwrap).
  - `spawn_blocking` join errors → `Internal { "FILE_READ_PANIC" / "FILE_WRITE_PANIC" }`.
  - Inner `std::fs` errors → `Internal { "FILE_READ_FAILED" / "FILE_WRITE_FAILED" }` (no unwrap on the result).
- All other `.unwrap()` / `.expect()` in the crate remain in test code or non-security paths (pre-existing).
- **Verdict**: the security-critical async path (path guard + FS I/O) now has explicit, mapped panic handling instead of implicit propagation.

### 6. Frontend changes (T1/T2) — correctness / no behavior change
- `nexus-logo.tsx`: `Variant` is now `export type Variant = LogoVariantName;` (re-export alias); `VARIANT_FILENAMES` is a re-export of `logoVariants`. Single source of truth in `tokens.ts`.
- `index.ts` exports the alias for backward compat.
- `nexus-logo.test.tsx`: compile-time type-identity guard (`AssertVariantAlias`) + runtime filename map test.
- `nexus-mark.tsx`: wrapped in `React.memo(NexusMarkImpl)` with a one-line rationale comment. Static SVG → no observable change.
- All UI tests (7) + typecheck + build pass; no visual or API breakage.

### 7. Tracker hygiene (T7) and T6 LFS comment
- Active tracker (`deferred-features-cross-version-tracker.md`) grep for the 8 IDs returns zero matches (verified).
- Same 8 rows (BL-10, BL-12, PF-ESSAY, PF-GAME-BIBLE, PF-SCRIPT, FEAT-WORLD-KB-RELATIONSHIPS, REL-01, DF-49) are present in `archived/shipped-features-tracker.md` with correct ship/cancel notes.
- `.github/workflows/desktop-build.yml` LFS comments (lines 49 and 96) already cover both `packages/nexus-ui/assets/logos/*.png` and `apps/desktop/src-tauri/icons/source/*.png`; no code change required (inspection only).

### 8. CI / gate verification
- `cargo test -p nexus-daemon-runtime`: full suite green (including new async path-guard tests + existing fs/* escape tests).
- `cargo clippy --all -- -D warnings`: clean.
- `cargo +nightly-2026-06-26 fmt --all --check`: clean.
- UI: build + typecheck + tests green.
- `wire_contracts_changed: false` — no schemas/, contracts, or DTO changes.

## Source Trace
- Finding ID: QC2-2026-07-03-001 (path-guard + gate ordering)
- Source Type: manual code review + test execution + git diff
- Source Reference: path_guard.rs (async wrapper + sibling test), host_tool_handlers.rs:97 (Gate 3 comment), execute_read/write_file (resolution + mapping), auth_middleware.rs + mod.rs (Arc), chapters.rs:174 (scope note)
- Confidence: High

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 0 |

**Verdict**: Approve

## Verdict Reasoning (QC2)
All security and correctness invariants required by the plan and QC2 role are preserved or improved:

- Path-guard behavior (in-bounds success + sibling-escape rejection) is identical and now has dedicated async regression tests.
- Removing the duplicate Gate 3 path check for `fs/*` is a relocation, not a weakening: Gate 4 remains, and `execute_*` still perform the single authoritative `resolve_guarded_path_async` before any filesystem syscall.
- Error codes returned to callers for path-guard rejections are unchanged.
- `Arc<DaemonApiConfig>` is a read-only, post-construction config; no shared mutable state or additional attack surface.
- The async wrapper introduces exactly one new error (`PATH_GUARD_PANIC`) which is mapped through the existing `other => other` arms — no silent panics or new unwraps on the hot path.
- Frontend refactors are pure alias + defensive memoization with compile-time guards and no behavior change.
- Tracker and LFS hygiene items are verified by inspection + grep.

All verification commands (Rust test/clippy/fmt + UI build/typecheck/test) are green. No Critical or Warning findings under the security/correctness lens.

**Recommend**: Approve. No blockers for merge to main after QA and the remaining QC votes.
