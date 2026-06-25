---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: "v1.65"
verdict: "Request Changes"
generated_at: "2026-06-25"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-3
- Runtime Agent ID: qc-specialist-3
- Runtime Model: kimi-for-coding/k2p7
- Review Perspective: Performance and reliability risk
- Report Timestamp: 2026-06-25T17:55:00Z

## Scope
- plan_id: v1.65
- Review range / Diff basis: merge-base 644acbc56856d03e8e3aaf2139f73dccfcf6ed54 ... HEAD 73e3343081ffa415b221252b5432dc1c6e21f07b (= `git diff origin/main...HEAD`; 112 files, +8902/-422)
- Working branch (verified): iteration/v1.65
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 112 (diff reviewed; focused inspection on P0/P1/P2 hot paths)
- Commit range: 644acbc56856d03e8e3aaf2139f73dccfcf6ed54..73e3343081ffa415b221252b5432dc1c6e21f07b
- Tools run:
  - `git rev-parse --show-toplevel`, `git branch --show-current`, `git merge-base origin/main HEAD`
  - `cargo test -p nexus-daemon-runtime --lib`
  - `cargo test -p nexus-acp-host --lib`
  - `cargo test -p nexus-cloud-sync --lib`
  - `cargo test -p nexus42 --lib`
  - `cargo test -p nexus-contracts --test schema_drift_detection`
  - `cargo clippy -p nexus-daemon-runtime -p nexus-local-db -p nexus-acp-host -p nexus-cloud-sync -p nexus42 -p nexus-contracts -- -D warnings`
  - `pnpm --filter nexus-contracts build`
  - `pnpm --filter web typecheck`
  - `pnpm --filter web test` (×2, stable)
  - `pnpm --filter web build`
  - `pnpm --filter web test:coverage`

## Findings

### 🔴 Critical

_None._

### 🟡 Warning

#### W-1 — Chapter list pagination uses OFFSET, not keyset
- **Source**: manual review + `crates/nexus-daemon-runtime/src/api/pagination.rs`, `crates/nexus-local-db/src/work_chapters.rs:284-311`
- **Evidence**: `decode_offset_cursor` / `encode_offset_cursor` encode a SQL `OFFSET` (`v1:<offset>`). `list_chapters_paginated` builds `ORDER BY volume, chapter LIMIT ? OFFSET ?`.
- **Impact**: For Works with many chapters, deep pages degrade because SQLite must scan and discard an ever-growing prefix. The cursor is also not stable under insertions/deletions in earlier pages.
- **Fix**: Replace the offset-backed cursor with a keyset over `(volume, chapter)` — cursor encodes the last seen `(volume, chapter)` tuple and the query adds `WHERE (volume, chapter) > (?, ?)`. Update `pagination.rs` to support a `v2:` keyset token or add a chapter-specific encoder. Verify the existing `idx_work_chapters_next_volume_aware (work_id, status, volume, chapter)` or a new `(work_id, volume, chapter)` index covers the new pattern.
- **Residual severity map**: `high`

#### W-2 — Body GET performs unbounded file read with no size cap or streaming
- **Source**: manual review + `crates/nexus-daemon-runtime/src/api/handlers/chapters.rs:679-731` (`get_chapter_body`), `read_guarded_file:284-312`
- **Evidence**: `tokio::fs::read_to_string(&path).await` loads the entire body file into memory and returns it as a single JSON string. There is no `MAX_BODY_BYTES` guard, range request, or streaming response.
- **Impact**: A large chapter body (hundreds of thousands of words + frontmatter) can OOM the daemon thread and the HTTP JSON serializer. Body files are expected to grow far larger than outlines.
- **Fix**: Add a configurable size cap (e.g., 8–16 MiB) to `read_guarded_file`; return `413 Payload Too Large` or a dedicated `CHAPTER_BODY_TOO_LARGE` error when exceeded. For bodies above the cap, consider a range/streaming endpoint in a future plan; at minimum, document the cap in the contract and UI.
- **Residual severity map**: `high`

#### W-3 — Outline PUT writes file before DB, leaving FS/DB inconsistent on DB failure
- **Source**: manual review + `crates/nexus-daemon-runtime/src/api/handlers/chapters.rs:505-576` (`put_chapter_outline`)
- **Evidence**: Inside the runtime-lock block, `atomic_write_outline` writes + renames the file first, then `work_chapters::update_outline_path` updates the row. If the DB update fails, the handler returns 500 but the file already contains the new content; a subsequent GET follows the old `outline_path` from the DB and returns stale/missing content.
- **Impact**: Data inconsistency between filesystem SSOT and DB metadata under a partial-failure scenario. The runtime lock prevents concurrent writers but does not make the two stores atomic.
- **Fix**: Reorder so the DB `outline_path` (and `updated_at`) is updated first inside a transaction, then the file is written. If the file write fails after the DB commit, the DB points to the intended path and a retry PUT is idempotent. Alternatively, keep the current order but roll back the file rename on DB failure (more complex across async boundaries). Add a regression test that simulates a DB error after a successful file write.
- **Residual severity map**: `high`

#### W-4 — Body context menu leaks `keydown` event listeners
- **Source**: manual review + `apps/web/src/pages/chapter-page.tsx:352-364`
- **Evidence**: The `useEffect` adds a `click` listener (with `{ once: true }`) and an anonymous `keydown` listener, but the cleanup only removes the `click` listener. Each time `menuOpen` flips to `true`, a new anonymous `keydown` listener is added and never removed.
- **Impact**: Accumulating listeners per right-click interaction; in a long editing session this is a slow resource leak and can cause Escape-key handling to fire multiple times.
- **Fix**: Declare the `keydown` handler as a named function (or memoize it) and remove it in the cleanup. Better yet, use a single `useEffect` that depends on `menuOpen` and consistently adds/removes both listeners.
- **Residual severity map**: `medium`

### 🟢 Suggestion

#### S-1 — TipTap `useEditor` receives unstable inline dependencies
- **Source**: manual review + `apps/web/src/pages/chapter-page.tsx:61-69`
- **Evidence**: `extensions: [StarterKit, Markdown]` and `onUpdate: () => { ... }` are created fresh on every render. `useEditor` may treat them as new dependencies and re-initialize the editor, causing unnecessary ProseMirror teardowns and layout thrashing.
- **Fix**: Memoize `extensions` with `useMemo` and `onUpdate` with `useCallback`. Alternatively, pass a stable deps array as the second argument to `useEditor`.
- **Residual severity map**: `low`

#### S-2 — Structure table lacks virtualization for large chapter counts
- **Source**: manual review + `apps/web/src/pages/chapters-page.tsx:148-305`
- **Evidence**: `rows.map(...)` renders every chapter row into the DOM. The page uses cursor pagination (`useChapters`) but still flattens all fetched pages into one table.
- **Impact**: Works with hundreds of chapters will create a large DOM and slow render/scroll. V1.65 is single-Work scope, so this is a future-readiness note rather than a blocker.
- **Fix**: Evaluate `@tanstack/react-virtual` or a windowed table once multi-hundred-chapter Works are in scope; until then, consider capping the page size and encouraging the existing "Load more" pattern.
- **Residual severity map**: `low`

#### S-3 — Runtime lock relies on TTL cleanup if guard is dropped mid-panic
- **Source**: manual review + `crates/nexus-daemon-runtime/src/api/handlers/chapters.rs:23-106`
- **Evidence**: `RuntimeLockGuard::Drop` only logs a warning and does not spawn async release. The handlers do call `lock.release().await` on all normal/Error paths, but a panic inside the locked async block will leave the lock held until TTL expires.
- **Impact**: In the local-first single-writer model this is mitigated by TTL, but a panic during outline save would block orchestration on that Work for the TTL window.
- **Fix**: Consider a closure-based wrapper (e.g., `with_runtime_lock(pool, creator_id, work_id, |guard| async { ... }).await`) that guarantees release via `defer`/finally, or document the TTL dependency explicitly. Current pattern follows the existing `works.rs` guard style, so this is a low-priority architectural follow-up.
- **Residual severity map**: `low`

#### S-4 — Test coverage misses save-error and protected-chapter confirm completion
- **Source**: `pnpm --filter web test:coverage` output + `apps/web/src/pages/chapter-page.test.tsx`, `apps/web/src/pages/chapters-page.test.tsx`
- **Evidence**: Statement coverage is 95.24%, but branch coverage for `chapter-page.tsx` is 60% and for `chapters-page.tsx` is 68.18%. The `handleSave` catch path, reset action, and the `ProtectedEditDialog` confirm flow are not exercised end-to-end.
- **Impact**: Low risk because the happy paths are covered, but error handling and the protection gate could regress silently.
- **Fix**: Add tests for (a) outline save returning 500 → `saved-error` state and toast, (b) clicking Reset reverts editor content, (c) editing a finalized chapter, clicking Confirm Edit, and asserting `confirm_structural_edit=true` is sent.
- **Residual severity map**: `low`

## Source Trace

| Finding ID | Source Type | Source Reference | Confidence |
|------------|-------------|------------------|------------|
| W-1 | manual-reasoning | `crates/nexus-daemon-runtime/src/api/pagination.rs`, `crates/nexus-local-db/src/work_chapters.rs:301` | High |
| W-2 | manual-reasoning | `crates/nexus-daemon-runtime/src/api/handlers/chapters.rs:302-311`, `:713-719` | High |
| W-3 | manual-reasoning | `crates/nexus-daemon-runtime/src/api/handlers/chapters.rs:545-565` | High |
| W-4 | manual-reasoning | `apps/web/src/pages/chapter-page.tsx:352-364` | High |
| S-1 | manual-reasoning | `apps/web/src/pages/chapter-page.tsx:61-69` | Medium |
| S-2 | manual-reasoning | `apps/web/src/pages/chapters-page.tsx:148-305` | Medium |
| S-3 | manual-reasoning | `crates/nexus-daemon-runtime/src/api/handlers/chapters.rs:96-106` | Medium |
| S-4 | static-analysis | `pnpm --filter web test:coverage` report | High |

## Verification Summary

| Check | Result | Notes |
|-------|--------|-------|
| `cargo test -p nexus-daemon-runtime --lib` | ✅ pass | 305 tests, ~22s |
| `cargo test -p nexus-acp-host --lib` | ✅ pass | 157 tests |
| `cargo test -p nexus-cloud-sync --lib` | ✅ pass | 89 tests |
| `cargo test -p nexus42 --lib` | ✅ pass | 762 tests |
| `cargo test -p nexus-contracts --test schema_drift_detection` | ✅ pass | 4 tests |
| `cargo clippy -p nexus-daemon-runtime -p nexus-local-db -p nexus-acp-host -p nexus-cloud-sync -p nexus42 -p nexus-contracts -- -D warnings` | ✅ pass | |
| `pnpm --filter nexus-contracts build` | ✅ pass | 3.53 kB mjs / 80.53 kB dts |
| `pnpm --filter web typecheck` | ✅ pass | |
| `pnpm --filter web test` (run 1) | ✅ pass | 80 tests, 2.48s |
| `pnpm --filter web test` (run 2) | ✅ pass | 80 tests, 2.13s — stable |
| `pnpm --filter web build` | ✅ pass | `dist/assets/index-DIGnklAS.js` 942.19 kB (300.87 kB gzip); no V1.64 baseline in this checkout |
| `pnpm --filter web test:coverage` | ✅ pass | Statements 95.24%, Branches 78.78%, Functions 72.5%, Lines 95.24% |

## Dep-bump reliability notes (P-sec)

- **vitest 2 → 3 / vite 5 → 6**: Web test suite passes twice with stable timings; build warning about 942 kB chunk is expected for a TipTap + react-markdown SPA. No runner regression observed.
- **wiremock 0.5 → 0.6**: All three consuming crates (`nexus-acp-host`, `nexus-cloud-sync`, `nexus42`) pass their lib tests. No mock-stability regression observed.
- **msw 2.7.0**: `server.resetHandlers()` between tests prevents cross-test leakage; 80 web tests pass deterministically across two runs.

## Soft-concurrency / file-I/O assessment

- **Outline PUT atomicity**: `atomic_write_outline` uses temp-file + `sync_all` + `rename`, so readers see only the old or new file, never a partial write. ✅
- **Orchestration body write**: Existing `host_tool_handlers.rs` manuscript tools use the same temp-write-rename pattern. ✅
- **FS/DB consistency gap**: See W-3 — the ordering between file rename and DB update is not crash-safe.
- **Runtime lock release**: All normal and error exit paths in `put_chapter_outline` and `patch_chapter` call `lock.release().await` before propagating errors. Panic paths rely on TTL; see S-3.

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 4 |
| 🟢 Suggestion | 4 |

**Verdict**: Request Changes

Rationale: No Critical findings, but W-1 (OFFSET pagination), W-2 (unbounded body read), and W-3 (FS/DB inconsistency on outline save) are substantive performance/reliability issues on the P0 hot path. W-4 is a real frontend resource leak. These must be fixed or explicitly risk-accepted before approval per the performance/reliability focus.
