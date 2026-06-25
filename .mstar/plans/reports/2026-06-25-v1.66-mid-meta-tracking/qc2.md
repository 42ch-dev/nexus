---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: "2026-06-25-v1.66-mid-meta-tracking"
verdict: "Approve"
generated_at: "2026-06-25"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Review Perspective: Security and correctness risk (reviewer 2 of 3)

## Scope
- plan_id: 2026-06-25-v1.66-mid-meta-tracking (P-mid umbrella — multi-plan single tri-review per compass §3)
- Feature / scope label: V1.66 iteration integration — P0 desktop shell core + P-sec hygiene (3 residuals) + P1 sidecar lifecycle + macOS CI
- Review range / Diff basis: `merge-base: 6e1f18e0 (origin/main) + tip: c8d22976 (iteration/v1.66 HEAD)` — equivalent to `git diff 6e1f18e0...c8d22976` (118 files, +10020/-572)
- Working branch (verified): iteration/v1.66
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus (repo root)
- Commit range: 6e1f18e0..c8d22976
- Tools run: git diff/show, cargo test (chapters_api 12 pass, pagination_info_parity 3 pass), source inspection of lib.rs/sidecar.rs/capabilities/path_guard.rs/host_tool_handlers.rs/tauri-client.ts

## Findings

### 🔴 Critical
None. Core security invariants (path guard authoritative, sidecar ownership tracking, fail-closed on unknown root) implemented; new tests exercise rejection paths.

### 🟡 Warning (both accepted as documented §5 #8 trade-offs / test-scope limitations)
- **Broad Tauri opener capability grants (`**`)**: `capabilities/main.json` grants `opener:allow-open-path`/`reveal-item-in-dir` with `path: "**"`. Per §5 #8 LOCKED, this is defense-in-depth only — Rust `guard_path` (canonicalize root + candidate + `starts_with`) is AUTHORITATIVE and runs before any opener call. No bypass found; unit tests cover traversal/escape/unknown-root. Remaining theoretical risk: TOCTOU/symlink-race if guard has future edge-case regression. Accepted documented trade-off.
- **Chapter integration tests handler-direct only**: `chapters_api.rs` exercises W-002 guard logic (`resolve_guarded_path` + `CHAPTER_PATH_FORBIDDEN`) via direct handler calls, not full HTTP routing (axum-test hyphenated-UUID limitation). Guard is shared; an E2E malicious `body_path` over the desktop transport is not asserted this wave. Existing daemon chapter PUT handler applies the guard before FS ops. Acceptable for hygiene residual; noted for security completeness.

### 🟢 Suggestion
- Sidecar ownership & restart policy: owned-sidecar externally killed → bounded restart (5 attempts) may surprise users who intentionally terminated. Consider distinct "externally terminated while owned" state. (No correctness bug — "don't kill unrelated" contract upheld.)
- TOCTOU between guard and opener/write remains theoretical (local user-controlled workspace threat model); worth a one-line comment in both guards.
- RuntimeLockGuard dedup is pure hygiene, no lock-state/ordering regression; fsync additions are a durability win.
- PaginationInfo dedup parity test is strong (asserts exact serialized shape + round-trip); byte-stable; 3 tests pass.
- TauriClient transport parity clean (thin wrapper reusing BrowserClient HTTP; port resolution identical to sidecar --port injection).
- Path guard unit tests comprehensive (relative/absolute-in/traversal/fail-closed-unknown-root/nonexistent).

## Source Trace
- Path guard + authoritative design: capabilities/main.json, lib.rs:141-156, guard_path:109-128 — High
- Chapter escape test + handler guard (R-V165-QC1-W2): cargo test 12/12 pass; chapters_api.rs + host_tool_handlers.rs W-002 additions — High
- Sidecar ownership stop-only-if-owned: sidecar.rs owned flag, stop(), spawn_monitor() — High
- fsync + write-path guard parity: host_tool_handlers.rs diff — High
- PaginationInfo parity: cargo test 3/3 pass; new test file — High

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 2 (accepted §5 #8 trade-offs) |
| 🟢 Suggestion | 6 |

**Verdict**: **Approve** — the two Warnings are documented design trade-offs / test-scope limitations with no demonstrated bypass or incorrect ownership behavior. Path guard, sidecar ownership model, write-path parity, fsync, and contract tests are correct and contained. No changes required before merge.
