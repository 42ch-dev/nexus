---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-06-28-v1.72-canvas-outline-timeline-beta"
verdict: "Request Changes"
generated_at: "2026-06-28"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Runtime Model: deepseek/deepseek-v4-pro
- Review Perspective: Architecture coherence and maintainability risk
- Report Timestamp: 2026-06-28T17:15:00+08:00

## Scope
- plan_id: 2026-06-28-v1.72-canvas-outline-timeline-beta
- Review range / Diff basis: `git diff 92a1c07f..HEAD -- schemas/local-api/canvas/outline/ packages/nexus-contracts/src/generated/local-api/canvas/outline/ packages/nexus-contracts/package.json crates/nexus-contracts/src/generated/local_api/canvas/ crates/nexus-contracts/src/generated/local_api/canvas/mod.rs crates/nexus-contracts/src/generated/local_api/mod.rs crates/nexus-contracts/src/generated/mod.rs crates/nexus-contracts/tests/schema_drift_detection.rs crates/nexus-daemon-runtime/src/api/errors.rs crates/nexus-daemon-runtime/src/api/handlers/mod.rs crates/nexus-daemon-runtime/src/api/handlers/outline.rs crates/nexus-daemon-runtime/src/api/mod.rs crates/nexus-daemon-runtime/tests/outline_api.rs apps/web/src/components/canvas/conflict-modal-base.tsx apps/web/src/components/canvas/conflict-modal.tsx apps/web/src/components/canvas/outline-canvas.tsx apps/web/src/components/canvas/outline-conflict-modal.tsx apps/web/src/components/canvas/outline-conflict-modal.test.tsx apps/web/src/lib/canvas/use-outline-data.ts apps/web/src/lib/nexus/browser-client.ts apps/web/src/lib/nexus/query-keys.ts apps/web/src/lib/nexus/types.ts apps/web/src/pages/outline-page.tsx apps/web/src/App.tsx apps/web/DESIGN.md apps/web/DESIGN.dark.md`
- Working branch (verified): iteration/v1.72
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 27 paths (schemas, generated contracts, Rust handlers + tests, web components + hooks + client)
- Commit range: 92a1c07f..5aa00d50 (20 commits in range)
- Tools run: `cargo clippy --all -- -D warnings` (PASS), `cargo +nightly-2026-06-26 fmt --all --check` (PASS), `cargo test --all` (10 pre-existing failures in nexus-creator-memory, unrelated to Track A), `cargo test -p nexus-daemon-runtime --test outline_api` (5/5 PASS), `pnpm --filter web typecheck` (PASS), `pnpm --filter web build` (PASS), `pnpm --filter web test` (19 files / 153 tests PASS), `pnpm run codegen` (no drift, PASS)

## Findings

### 🟡 Warning

#### F-001: outline-canvas.tsx is monolithic (825 lines) — should be split into sibling modules like strategy-canvas

- **Trigger**: `outline-canvas.tsx` is a single file of 825 lines containing the orchestrator (`OutlineCanvas`), `OutlineStructurePanel`, `ChapterInspector`, `TimelinePanel`, `ChapterRow`, `VolumeSection`, `RevisionBadge`, and `changedFieldsOf` helper. This mirrors the pre-split `strategy-canvas.tsx` (~800 lines before P1 B2 refactored it to 187 lines across 7 modules under `strategy-canvas/`).
- **Impact scope**: Maintenance friction — editing the `ChapterInspector` or `TimelinePanel` requires navigating the full file. Testing individual sub-components is harder (no isolated imports). The project has an established pattern (P1 B2: `strategy-canvas/inspectors/`, `strategy-canvas/state-machine.tsx`, `strategy-canvas/canvas-layout.tsx`, `strategy-canvas/hooks/`).
- **Recommendation**: Extract `ChapterInspector`, `TimelinePanel`, `OutlineStructurePanel` (with `VolumeSection` + `ChapterRow`), and `changedFieldsOf` into separate files under `outline-canvas/`. Keep `OutlineCanvas` as the thin orchestrator. Line budget per module: ≤200 lines (same threshold used for strategy split in B2). This is not blocking for β but should be addressed before the surface stabilizes (i.e., before removing the β label) — the pattern mismatch creates confusion for future contributors.
- **Confidence**: High

#### F-002: `u64` / `i64` type mismatch in `outline_revision` — silent `unwrap_or(0)` fallback

- **Trigger**: `OutlineFrontmatter.outline_revision` is `u64` (line 36 of `outline.rs`), but `OutlinePatchResponse.new_revision` is `i64` (JSON Schema declares `type: integer`). The `patch_ok` helper (line 286) converts via `i64::try_from(new_revision).unwrap_or(0)`. If `u64` exceeds `i64::MAX` (practically impossible, but the type system allows it), the response would silently claim `new_revision: 0`. The schema `outline-patch-response.schema.json` declares `"minimum": 1` for `new_revision`, so `0` is a schema contract violation.
- **Impact scope**: Practical risk is negligible (needs 9×10^18 revisions), but the silent fallback to `0` is a correctness code smell. The type inconsistency between the internal model (`u64`) and the wire contract (`i64`) should be resolved to prevent future drift.
- **Recommendation**: Change `OutlineFrontmatter.outline_revision` to `i64` to match the wire contract, or use `u64` throughout and remove the `unwrap_or(0)` fallback (use `expect` or proper error handling). Option A (use `i64`) is simpler and consistent with `StrategyPatchResponse` (also `i64`), and `i64` range (9×10^18) far exceeds any realistic revision counter.
- **Confidence**: High

### 🟢 Suggestion

#### F-003: Generated TS type for `chapter_titles` is `Record<string, unknown>` — consumers must cast

- **Trigger**: JSON Schema `work-outline.schema.json` declares `"chapter_titles": { "type": "object", "additionalProperties": { "type": "string" } }`. The generated TypeScript (`WorkOutline.ts` line 18) produces `chapter_titles: Record<string, unknown>`. The outline-canvas.tsx (line 446) has to cast: `const titles = outline.chapter_titles as Record<string, string> | undefined`.
- **Impact scope**: TypeScript consumers lose type safety on chapter title values. The cast is localized and safe in the current code, but any new consumer of `WorkOutline.chapter_titles` in a different component must discover and replicate the cast.
- **Recommendation**: This is likely a codegen limitation (`quicktype` may not resolve `additionalProperties` with primitive types). Track as a codegen improvement task in a future iteration. If the codegen cannot be fixed, add a typed accessor to the contracts package.
- **Confidence**: Medium (unclear if codegen can resolve this; needs investigation)

#### F-004: Non-spatial alternate views not yet implemented

- **Trigger**: Compass §1.1 mentions "non-spatial alternate views" as part of Track A scope. The current implementation provides the spatial volume/chapter layout and timeline list, but no table view, outline-only view, or timeline-only view components.
- **Impact scope**: The surface is functional for β but misses the "alternate views" mentioned in scope. This appears to be an intentional deferral (the β label and compass §6 note suggest phased delivery).
- **Recommendation**: Add alternate view components (table view, timeline-only view, outline-tree view) in a follow-up slice. Ensure the `OutlineCanvas` orchestrator can switch between views without re-fetching data.
- **Confidence**: Medium (scope boundary is unclear — compass mentions it, but β label implies phased delivery)

## Source Trace

| Finding ID | Source Type | Source Reference | Confidence |
|-----------|-------------|-----------------|------------|
| F-001 | manual-reasoning | `outline-canvas.tsx` (825 lines) vs `strategy-canvas.tsx` (187 lines) + 6 sibling modules; P1 B2 commit `73ed508b` | High |
| F-002 | git-diff | `outline.rs:286` `patch_ok` uses `i64::try_from(new_revision).unwrap_or(0)`; `outline.rs:36` declares `outline_revision: u64`; `outline-patch-response.schema.json` `minimum: 1` | High |
| F-003 | git-diff | `work-outline.schema.json` vs `WorkOutline.ts:18`; `outline-canvas.tsx:446` cast | Medium |
| F-004 | doc-rule | compass §1.1 "non-spatial alternate views"; current implementation has no view-switching | Medium |

### Non-findings (verified OK)

1. **Patch-route contract coherence**: All 3 patch routes return `OutlinePatchResponse` with `new_revision` + `validation_summary`. Conflict returns `OutlineConflictError` (HTTP 409) with `current_revision` + `node_id` + `conflicting_path` + `recovery_hint`. Validation errors return `OutlineValidationError` (HTTP 422) with `validation_summary`. Pattern matches V1.71's `StrategyConflictError` / `StrategyValidationError`. ✓

2. **outline_revision storage**: Key name `outline_revision:` is consistently `snake_case` in YAML frontmatter (Rust `#[serde(rename_all = "snake_case")]`), JSON Schema, and TypeScript. Architect Phase 2b decision honored. ✓

3. **Atomic write**: `atomic_write_outline` (lines 209-263) implements temp file + write + fsync on temp + rename + fsync on target + fsync on parent dir. This matches V1.65 durability pattern. ✓

4. **Revision not incremented on failure**: All three handlers call `frontmatter.outline_revision += 1` ONLY after `apply_*_patch` succeeds. If the patch application fails, the error is returned before the increment. ✓

5. **Crash/failure path**: If `atomic_write_outline` fails after the `+= 1`, the error is returned before the lock is released, and the temp file is cleaned up (`let _ = tokio::fs::remove_file(&temp_path).await`). The in-memory `frontmatter` mutation is discarded (not committed to disk). No stale revision claim. ✓

6. **Lock acquire/release**: All three handlers follow Rule 1 (existence check before `acquire`) and Rule 2 (explicit `lock.release().await` on EVERY exit path — early conflict, patch failure, write failure, and success). Pattern matches V1.42.1 hotfix rules. ✓

7. **TOCTOU protection**: All three handlers double-check `base_revision` after acquiring the lock (re-reads the file under lock). This prevents the classic TOCTOU race where another writer sneaks in between the initial read and lock acquisition. ✓

8. **Shared conflict modal reuse**: `conflict-modal-base.tsx` is a clean generic shell with focus trapping, Escape handling, live-region announcements, overlap detection, and three actions (Use current / Reapply / Keep editing). `conflict-modal.tsx` (strategy) and `outline-conflict-modal.tsx` (outline) are both thin wrappers. Strategy canvas imports `ConflictModal` from the public facade (`@/components/canvas/conflict-modal`), not from internal split modules. ✓

9. **NexusClient integration**: `browser-client.ts` adds `getWorkOutline` / `patchOutlineStructure` / `patchOutlineChapter` / `patchTimelineEvent` methods, matching the `NexusClient` interface in `types.ts`. Query keys in `query-keys.ts` add `outline.detail(workId)` key. Mutations in `use-outline-data.ts` invalidate the outline query on success. ✓

10. **Test coverage**: `outline-conflict-modal.test.tsx` (5 tests covering render, field listing, button enable/disable, callbacks). `outline_api.rs` (5 integration tests covering default read, structure patch, chapter patch, timeline patch, stale revision rejection). `outline.rs` inline tests (4 unit tests for `split_frontmatter` and `validate_status_transition`). Web test suite (153 tests, 19 files — all passing). ✓

11. **Published chapter protection**: `has_chapter_structural_edit` gates status=publish protection in `patch_outline_chapter` (line 478). Schema declares `published` as a valid enum value but the handler blocks structural edits. ✓

12. **Status transition validation**: `validate_status_transition` allows forward moves only (not_started → outlined/draft/finalized → draft/finalized → finalized). Rejects reverse transitions and `published` transitions (which are blocked higher in the handler). Schema §3.2 lifecycle vocabulary honored. ✓

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 2 |
| 🟢 Suggestion | 2 |

**Verdict**: Request Changes

F-001 (module decomposition) and F-002 (type mismatch) must be addressed before this can move to Approve. Both are architectural consistency issues that, while not blocking β functionality, should not be deferred in the first QC round. They represent established project patterns (strategy split, type consistency with wire contract) that Track A should match.

If PM chooses to defer F-001 to a follow-up (e.g., as part of removing the β label) and fix only F-002 now, the verdict can be downgraded to Approve with Residuals. The architectural decision is PM's.

F-003 and F-004 are non-blocking suggestions.
