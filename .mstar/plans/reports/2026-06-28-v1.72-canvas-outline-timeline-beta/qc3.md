---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: 2026-06-28-v1.72-canvas-outline-timeline-beta
verdict: Request Changes
generated_at: 2026-06-28
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-3
- Runtime Agent ID: qc-specialist-3
- Runtime Model: volcengine-plan/ark-code-latest
- Review Perspective: Performance and reliability risk
- Report Timestamp: 2026-06-28T17:30:00+08:00

## Scope
- plan_id: 2026-06-28-v1.72-canvas-outline-timeline-beta
- Review range / Diff basis: `git diff 92a1c07f..HEAD -- schemas/local-api/canvas/outline/ packages/nexus-contracts/src/generated/local-api/canvas/outline/ packages/nexus-contracts/package.json crates/nexus-contracts/src/generated/local_api/canvas/ crates/nexus-contracts/src/generated/local_api/canvas/mod.rs crates/nexus-contracts/src/generated/local_api/mod.rs crates/nexus-contracts/src/generated/mod.rs crates/nexus-contracts/tests/schema_drift_detection.rs crates/nexus-daemon-runtime/src/api/errors.rs crates/nexus-daemon-runtime/src/api/handlers/mod.rs crates/nexus-daemon-runtime/src/api/handlers/outline.rs crates/nexus-daemon-runtime/src/api/mod.rs crates/nexus-daemon-runtime/tests/outline_api.rs apps/web/src/components/canvas/conflict-modal-base.tsx apps/web/src/components/canvas/conflict-modal.tsx apps/web/src/components/canvas/outline-canvas.tsx apps/web/src/components/canvas/outline-conflict-modal.tsx apps/web/src/components/canvas/outline-conflict-modal.test.tsx apps/web/src/lib/canvas/use-outline-data.ts apps/web/src/lib/nexus/browser-client.ts apps/web/src/lib/nexus/query-keys.ts apps/web/src/lib/nexus/types.ts apps/web/src/pages/outline-page.tsx apps/web/src/App.tsx apps/web/DESIGN.md apps/web/DESIGN.dark.md`
- Working branch (verified): iteration/v1.72
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: assigned diff paths plus supporting query/Tauri/client/lock files
- Commit range: 92a1c07f..HEAD
- Tools run: `git diff 92a1c07f..HEAD -- <assigned paths>`; `cargo clippy --all -- -D warnings` (PASS); `cargo test --all` (PASS); `cargo +nightly-2026-06-26 fmt --all --check` (PASS); `pnpm --filter web typecheck` (PASS); `pnpm --filter web build` (PASS with Vite chunk-size warning); `pnpm --filter web test` (PASS, 19 files / 153 tests)

## Findings

### 🔴 Critical

#### F-001: Canvas patches can overwrite a concurrently edited outline markdown body with a stale pre-lock copy

- **Trigger condition**: Each write handler reads `(initial_frontmatter, body)` before acquiring `RuntimeLockGuard`, then re-reads only the frontmatter under the lock (`read_outline_file(...).await?.0`) and later calls `atomic_write_outline(..., &body)` using the pre-lock body. If the work-level `Outlines/outline.md` body changes between the first read and the locked write without changing `outline_revision` (manual local edit, editor-owned body update, or any daemon path that treats the body as editor-owned), the canvas writes the old body back while committing its new frontmatter revision.
- **Performance/reliability impact**: This is a data-loss race in the exact boundary compass §6.4 calls out: “the outline markdown file body remains V1.65 editor-owned and must never be overwritten by the canvas.” The response may correctly return the new revision, but body content can silently roll back to the stale snapshot captured before the lock.
- **Evidence**: `crates/nexus-daemon-runtime/src/api/handlers/outline.rs:360-376`, `465-488`, and `551-565` read `body` before acquiring the lock, then use only `.0` from the under-lock read. Writes at `400`, `511`, and `588` pass the stale `body` to `atomic_write_outline`.
- **Fix**: Re-read both frontmatter and body under the lock and write the under-lock body after the revision check, e.g. `let (mut frontmatter, body) = read_outline_file(...).await?;`. Keep the early pre-lock read only as a fast conflict shortcut if desired, but never use its body for persistence. Add a regression/failure-injection test that simulates body text changing between the early read and committed patch and asserts the body is preserved.

### 🟡 Warning

#### F-002: Patch routes are not atomic across DB side effects and outline-frontmatter persistence

- **Trigger condition**: `apply_structure_patch` updates `work_chapters.volume` before `atomic_write_outline`; `apply_chapter_patch` updates slug/planned word count/volume/status before `atomic_write_outline`. If the temp write, rename, target fsync, or parent-dir fsync fails after the DB mutation, the handler returns an error but leaves the DB mutation committed while `outline_revision` remains unchanged on disk.
- **Impact**: The write path is `validate -> DB mutate -> file write`, not one atomic commit. A disk-full/permission/rename failure can leave split-brain state: chapters list reflects the new status/volume/slug, while the outline frontmatter/revision does not. The next canvas read can then render mixed DB/file state and the same `base_revision` may still look current.
- **Evidence**: Structure patch DB update at `outline.rs:632-637`, chapter DB update at `803-815`, and file commit at `400`/`511`. The new integration tests cover success and stale conflicts but do not inject `atomic_write_outline` failure after DB mutation.
- **Fix**: Make the persistence boundary explicit and transactional. Options: (1) prevalidate DB changes, write the durable outline image from the latest locked body, then commit DB changes with a compensating rollback/error strategy; (2) store the revision and all patch-owned metadata in one durable owner; or (3) introduce a small transaction/outbox pattern so recovery can reconcile incomplete commits. At minimum, add an injected write-failure test that proves no DB-visible mutation survives a failed file commit.

#### F-003: Successful outline chapter/structure mutations invalidate only the outline query, leaving chapter data stale

- **Trigger condition**: `usePatchOutlineStructure`, `usePatchOutlineChapter`, and `usePatchTimelineEvent` invalidate only `queryKeys.outline.detail(workId)`. But structure/chapter patches also mutate `work_chapters` fields displayed by `useChapters` (`volume`, `status`, `slug`, `planned_word_count`). The canvas continues to derive rows and inspector values from the stale `chaptersQuery` cache.
- **Impact**: After a successful patch, the revision badge may advance while the chapter list/inspector still show old chapter metadata until a focus/refetch event. Under contention, this can create a stale-then-conflict loop: users submit from a fresh outline revision but stale chapter values, especially around volume/status moves.
- **Evidence**: `apps/web/src/lib/canvas/use-outline-data.ts:54-56`, `69-71`, `84-86` invalidate only outline detail. `apps/web/src/components/canvas/outline-canvas.tsx:85-103` reads chapters via `useChapters` and uses those cached rows for rendering and save diffs.
- **Fix**: Invalidate `queryKeys.chapters.lists()` (and relevant `queryKeys.chapters.detail(workId, chapter)` for chapter patches) on successful structure/chapter patches. Alternatively, have the patch response carry the updated chapter summary.

#### F-004: Outline page is not route-split, so the 825-line canvas UI enters the Control Room bootstrap chunk

- **Trigger condition**: `App.tsx` imports `OutlinePage` statically, and `OutlinePage` imports `OutlineCanvas` statically. V1.70/V1.72 canvas policy says canvas routes should be route-split and React Flow must stay out of the Control Room bootstrap. The build output has no outline chunk; only `index-*.js` (983.07 kB, gzip 310.65 kB) and `strategy-page-*.js` are emitted.
- **Impact**: The current outline surface does not import React Flow, so this is not a React Flow bootstrap violation today. It is still a bundle regression: all outline UI, conflict modal wrappers, TanStack outline hooks, and Lucide icons load for every Control Room route. If/when the β surface adopts React Flow as scoped, the existing static route will pull it into the bootstrap unless fixed first.
- **Evidence**: `apps/web/src/App.tsx:10` static import and route at `44`; `apps/web/src/pages/outline-page.tsx:9` static import of `OutlineCanvas`; `pnpm --filter web build` output: `index-Bd1RQP8s.js 983.07 kB`, `strategy-page-DtwB_V_A.js 320.19 kB`, no outline-specific chunk, Vite chunk-size warning.
- **Fix**: Lazy-load `OutlinePage` behind `/works/:workId/outline` using the same `lazy` + `Suspense` pattern as `StrategyPage`. Keep any future React Flow import below that boundary.

#### F-005: Tauri/adapter parity guard still counts 24 methods and does not exercise the four new outline methods

- **Trigger condition**: `NexusClient` was extended with `getWorkOutline`, `patchOutlineStructure`, `patchOutlineChapter`, and `patchTimelineEvent`, but `adapter-contract.test.ts` still says “all 24 NexusClient methods” and asserts `seen.size` is 24. It does not call any outline method on `TauriClient` or `BrowserClient` in that parity loop.
- **Impact**: Today TauriClient inherits BrowserClient, so runtime parity is likely OK. The reliability gap is test drift: a future desktop-specific override or path regression for the outline methods would not be caught by the adapter parity guard that explicitly exists to pin browser/Tauri method-by-method transport equivalence.
- **Evidence**: `apps/web/src/lib/nexus/adapter-contract.test.ts:126-160` exercises 24 methods and stops at chapter body; new methods are in `apps/web/src/lib/nexus/types.ts:209-227` and `browser-client.ts:298-329`.
- **Fix**: Update adapter-contract tests to exercise all NexusClient methods, including the four outline/timeline routes, and update the expected method count/path assertions.

### 🟢 Suggestion

#### F-006: Outline graph projection has no large-outline smoke/performance test

- The canvas currently renders simple nested lists, not React Flow nodes, so there is no node/edge array rebuild hot path yet. However, `VolumeSection` does `chapters.find(...)` per volume chapter id, and inspector save paths repeatedly search `outline.volumes`. This is bounded at β scale but becomes O(V*C) / O(C*V) for very large works. Add a large-outline smoke test or lightweight benchmark (e.g. 1000 chapters across volumes) before the React Flow projection lands, and consider reusing the existing `chapterById` map inside `VolumeSection`.

## Source Trace

| Finding ID | Source Type | Source Reference | Confidence |
|-----------|-------------|------------------|------------|
| F-001 | manual-reasoning | `outline.rs:360-376`, `465-488`, `551-565`, `400`, `511`, `588`; compass §6.4 body ownership rule | High |
| F-002 | manual-reasoning | `outline.rs:632-637`, `803-815`, `400`, `511`; no write-failure test in `tests/outline_api.rs` | High |
| F-003 | git-diff | `use-outline-data.ts:54-56`, `69-71`, `84-86`; `outline-canvas.tsx:85-103` | High |
| F-004 | build-output + git-diff | `App.tsx:10,44`; `outline-page.tsx:9`; `pnpm --filter web build` chunk output | High |
| F-005 | git-diff | `adapter-contract.test.ts:126-160`; new interface methods in `types.ts:209-227` | High |
| F-006 | manual-reasoning | `outline-canvas.tsx:328-332`, `404-407`, `484-502`, `528`, `553-557`; no large-outline tests found | Medium |

### Non-findings (verified OK)

1. **No N+1 chapter reads**: The UI calls `useChapters(workId)` once and `useWorkOutline(workId)` once; it does not fetch one chapter per rendered row. The daemon similarly uses one `list_chapters` pre-load per route plus one extra list only when a chapter volume changes.
2. **No manual live-overlay subscription leak in this scope**: The outline surface adds no `setInterval`, WebSocket/EventSource, ResizeObserver, or custom subscription. TanStack Query owns observer cleanup. The shared conflict modal correctly removes its `keydown` listeners on unmount (`conflict-modal-base.tsx:112-118`).
3. **Fsync path**: `atomic_write_outline` writes and syncs the temp file, renames to target, syncs the final target file, and syncs the parent directory (`outline.rs:239-249`). This satisfies the “same final path + dir fsync” durability check.
4. **Concurrent stale revision check**: All three write routes re-read under `RuntimeLockGuard` and reject stale `base_revision` before applying patch logic. This covers “first accepted N+1, second 409” for revision-changing metadata writes.
5. **Out-of-bounds status strings**: The generated Rust type carries `status: Option<String>`, but schema/codegen plus handler transition validation reject arbitrary lifecycle strings before DB write (`validate_status_transition`).
6. **Tauri runtime mechanics**: TauriClient is thin-over-BrowserClient, so the new methods are inherited and use loopback `fetch`; no WKWebView-specific polling/subscription cliff was introduced.

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 1 |
| 🟡 Warning | 4 |
| 🟢 Suggestion | 1 |

**Verdict**: Request Changes

Critical F-001 must be fixed before approval. Warnings F-002 through F-005 are reliability/performance gate issues; if PM accepts any as β trade-offs, they should be explicitly tracked as residuals with tests or scope notes.
