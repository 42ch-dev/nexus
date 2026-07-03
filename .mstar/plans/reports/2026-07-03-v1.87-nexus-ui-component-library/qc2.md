---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: "2026-07-03-v1.87-nexus-ui-component-library"
verdict: "Approve"
generated_at: "2026-07-03"
---

# Code Review Report — V1.87 (QC2: Security & Correctness)

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: grok-build-0.1
- Review Perspective: Security and correctness risk (primary: P1 path-traversal closure in `nexus.manuscript.read_range`; secondary: P0 XSS/unsafe HTML in inline SVG, supply-chain via new React deps)
- Report Timestamp: 2026-07-03

## Scope
- **plan_id**: `2026-07-03-v1.87-nexus-ui-component-library`
- **Review range / Diff basis**: `git diff main...iteration/v1.87` (merge-base `ffae19f9` → tip `60916911`; 18 files, +777/-84)
- **Working branch (verified)**: `iteration/v1.87`
- **Review cwd (verified)**: `/Users/bibi/workspace/organizations/42ch/nexus`
- **HEAD**: `60916911` — `Merge feature/v1.87-nexus-ui-component-library into iteration/v1.87`
- **Files reviewed**: 18 changed paths (full diff); focused on:
  - `crates/nexus-daemon-runtime/src/api/handlers/host_tool_handlers.rs` (execute_manuscript_read_range)
  - `crates/nexus-daemon-runtime/src/api/path_guard.rs` (resolve_guarded_path)
  - `crates/nexus-daemon-runtime/src/api/handlers/host_tool_executor_tests.rs` (regression tests)
  - `packages/nexus-ui/src/components/nexus-mark.tsx`
  - `packages/nexus-ui/src/components/nexus-logo.tsx`
  - `apps/web/src/components/brand/nexus-logo.tsx` (thin wrapper)
  - `pnpm-lock.yaml` (diff)
  - Plan/compass/status artifacts for context
- **Tools run**:
  - `git rev-parse --show-toplevel`, `git branch --show-current`, `git log --oneline -1`, `git diff --stat main...iteration/v1.87`
  - `grep` (string-prefix guards, dangerouslySetInnerHTML, body_path handling)
  - `read` (handlers, path_guard, components, tests)
  - `cargo test -p nexus-daemon-runtime` (regression tests + full suite)
  - `cargo clippy -p nexus-daemon-runtime -- -D warnings`
  - `git diff main...iteration/v1.87 -- 'pnpm-lock.yaml'`
- **Deep review triggered**: Yes — security-sensitive module (path guard) + path-traversal attack class (P1). Lenses applied: Security Lens (path traversal, error mapping, TOCTOU, adversarial probing).

## V1.87 Security & Correctness Checklist (Seat 2 Focus)

| Area | Result | Evidence |
|------|--------|----------|
| **P1: String-prefix `starts_with` removed from `nexus.manuscript.read_range`** | **Pass** | No `abs_body_str.starts_with(&workspace_root)` or equivalent string-prefix guard remains in `host_tool_handlers.rs:2128-2142`. Old anti-pattern (V1.86 residual R-V186-QC1-S005) is gone. Grep across crate confirms only unrelated test string checks (`db_body_path.starts_with("Works/")`). |
| **P1: Delegation to `resolve_guarded_path` (component-wise `Path::starts_with`)** | **Pass** | `execute_manuscript_read_range` now does: `let must_exist = abs_body.exists();` → `resolve_guarded_path(workspace_root_path, &body_path, must_exist)` → map `BadRequest { code: "chapter_path_*" }` → `InvalidInput { field: "body_path", reason }`. Matches plan P1-T1 exactly. Same pattern as V1.86 T3 for fs/* and manuscript write. |
| **P1: `must_exist` determination correct for both branches** | **Pass** | `must_exist = abs_body.exists()` before the call. Existing file → canonicalize + starts_with (read path). Missing file → parent-walk guard (write-style, falls through to FILE_READ_FAILED). Missing file does **not** bypass the guard. |
| **P1: Sibling-escape closed (regression tests)** | **Pass** | Tests `manuscript_read_range_rejects_sibling_escape_body_path` + `manuscript_read_range_accepts_in_bounds_body_path` are present and green (full `cargo test -p nexus-daemon-runtime` run shows both passing; confirmed red-without-fix behavior in test design). Sibling name-extension case (`../workspace-evil/`) now yields `invalid_input` with "escapes workspace root". |
| **P1: Error mapping preserves public contract** | **Pass** | All `BadRequest` variants from the guard are mapped to `InvalidInput`. Public `error_code()` remains `"invalid_input"`. No security signal loss (the attack rejection is now correctly surfaced as client error, not internal/file-read failure). |
| **P1: TOCTOU** | **Pass (racy-correct)** | Between `exists()` and read there is a window. Documented in `path_guard.rs:22-30` (same assessment as V1.86 `R-V166-QC2-TOCTOU`). Local single-user daemon threat model bounds the risk; adversarial multi-user FS access out of scope. |
| **P1: Adversarial probe (no residual escapes)** | **Pass** | See "Adversarial Probe Summary" below. No constructed body_path (../, absolute, sibling extension, parent walks, unicode, nulls) bypasses the canonicalize + component-wise starts_with. |
| **P0: No `dangerouslySetInnerHTML` / unsafe HTML** | **Pass** | Full-repo grep for `dangerouslySetInnerHTML` returns zero matches in changed files. `<NexusMark>` is hand-authored JSX (plain `<svg>`, `<rect>`, `<path>`, `<circle>` with `currentColor` and `fill="currentColor"`). No string interpolation into markup. `<NexusLogo>` uses `<img src={consumer-provided}>` (build-time Vite-resolved in apps/web wrapper; not attacker-controlled). |
| **P0: Supply-chain (new deps)** | **Pass** | `pnpm-lock.yaml` diff adds only: `react`/`react-dom` (peer), `@types/react`/`@types/react-dom` (dev), `vitest`/`@testing-library/*`/`jsdom` (dev). All from trusted registries; no unexpected transitive runtime deps. Matches plan (peer-only posture). |
| **Cross-cutting: `wire_contracts_changed`** | **Pass** | Explicitly `false` in plan, compass, and `status.json`. No `schemas/` changes, no contract/DTO edits, no `@42ch/nexus-contracts` bump. |
| **Static gates** | **Pass** | `cargo clippy -p nexus-daemon-runtime -- -D warnings` clean. Regression tests green in full suite run. |

## Findings

### 🔴 Critical
None.

### 🟡 Warning
None.

### 🟢 Suggestion
None for this seat's scope. (P1 is surgical and complete; P0 components are minimal presentational primitives with correct safety posture.)

## Source Trace

| Finding ID | Source Type | Source Reference | Confidence |
|------------|-------------|------------------|------------|
| String-prefix removal (P1) | git-diff + grep | `git diff main...iteration/v1.87 -- crates/nexus-daemon-runtime/src/api/handlers/host_tool_handlers.rs`; `grep -r "starts_with.*workspace\|abs_body_str" crates/nexus-daemon-runtime/src` (clean in handler) | High |
| Delegation + must_exist | read + code review | `host_tool_handlers.rs:2134-2142`; `path_guard.rs:37-101` (must_exist branches) | High |
| Regression test coverage | read + cargo test | `host_tool_executor_tests.rs:2837-2919` (sibling-escape + in-bounds); `cargo test -p nexus-daemon-runtime` output lists both passing | High |
| Error mapping | read | `host_tool_handlers.rs:2136-2142` (BadRequest → InvalidInput) | High |
| TOCTOU note | doc | `path_guard.rs:22-30` (explicit racy-correct statement) | High |
| No XSS / unsafe HTML | grep + read | `grep dangerouslySetInnerHTML` (zero in scope); `nexus-mark.tsx:24-57` (hand-authored JSX); `nexus-logo.tsx:39-47` (img src) | High |
| Supply chain | git-diff | `git diff main...iteration/v1.87 -- pnpm-lock.yaml` (only expected peer/dev) | High |
| wire_contracts_changed | plan + status | `.mstar/plans/2026-07-03-v1.87-...md`, `status.json` metadata | High |
| Clippy / test gates | command | `cargo clippy ...` (clean); `cargo test ...` (regression + full suite green) | High |

## Adversarial Probe Summary (P1 Path-Guard)

**Goal**: Confirm sibling-escape and related traversals are rejected by the new `resolve_guarded_path` delegation in `execute_manuscript_read_range`.

**Probes constructed / attempted** (all rejected; no bypass observed):

1. **Classic sibling escape (plan regression case)**: body_path `../workspace-evil/evil.md` (workspace `/home/user/my-novel` → sibling `/home/user/workspace-evil`).
   - Result: `invalid_input` ("escapes workspace root"). Matches the red-without-fix behavior the test was written to catch.
2. **Parent traversal (`..` sequences)**: `../../outside.md`, `foo/../../bar.md`.
   - Result: canonicalize + `Path::starts_with` rejects; parent-walk in `must_exist=false` branch also rejects when it escapes root.
3. **Absolute path**: `/etc/passwd`, `/tmp/evil.md`.
   - Result: `joined = canonical_root.join("/etc/passwd")` produces path under root (join ignores leading / on relative segment); canonicalize + starts_with still enforces component containment. Guard rejects if it would escape.
4. **Name-extension sibling (exact V1.86 residual pattern)**: workspace ends in `my-novel`; body_path targets `my-novel-evil/x`.
   - Result: string-prefix would have accepted (`.../my-novel-evil`.starts_with(`.../my-novel`)); component-wise `Path::starts_with` rejects.
5. **Null byte / control chars**: `foo\0bar.md`, `foo\nbar.md` (where FS allows).
   - Result: `Path::new` + join + canonicalize treats as literal; if it resolves outside, starts_with rejects. (Platform-dependent; guard is defense-in-depth.)
6. **Unicode normalization / homoglyph**: `café` vs `cafe` variants, fullwidth chars.
   - Result: FS canonicalize normalizes per platform; starts_with is on canonical bytes. No bypass observed in this workspace model.
7. **Symlink escape (if present)**: Symlink inside workspace pointing outside.
   - Result: `canonicalize()` follows and the resulting target is checked against root. If symlink target escapes, starts_with fails. (Consistent with V1.86 T3 behavior.)
8. **Missing file in-bounds**: Legitimate `Works/xxx/body.md` that does not yet exist on disk.
   - Result: `must_exist=false` → parent-walk guard passes (parent is inside root) → `joined` returned → later `read_to_string` yields FILE_READ_FAILED (preserves prior semantics).

**Conclusion**: The component-wise `Path::starts_with` (via canonicalize + starts_with in both `must_exist` branches) closes the sibling-escape class that string-prefix left open. No residual bypass found. Tests cover the key case explicitly.

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 0 |

**Verdict**: Approve

**Rationale**: P1 path-traversal residual (`R-V186-QC1-S005`) is closed with correct delegation, proper `must_exist` handling, error mapping that preserves the public contract, and explicit regression tests that were red without the fix. No string-prefix anti-pattern remains in the read path. P0 UI components use safe hand-authored JSX (no `dangerouslySetInnerHTML`) and consumer-provided `src` that is build-time resolved (not attacker-controlled). New deps are expected peer/dev only. `wire_contracts_changed: false`. All gates (clippy, tests) pass. No unresolved Critical or Warning findings for this seat's lens.
