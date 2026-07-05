# Completion Report v2 — V1.72 P0 Outline/Timeline Fix-Wave

**Plan:** `2026-06-28-v1.72-canvas-outline-timeline-beta`  
**Branch:** `iteration/v1.72`  
**Date:** 2026-06-28  
**Author:** fullstack-dev

---

## 1. What Changed

| Residual | Severity | Scope | Fix | Commit |
|----------|----------|-------|-----|--------|
| **R-V172P0-QC3-001** | critical | `crates/nexus-daemon-runtime/src/api/handlers/outline.rs` | The three outline write handlers (`patch_outline_structure`, `patch_outline_chapter`, `patch_timeline_event`) no longer persist the body snapshot captured before acquiring `RuntimeLockGuard`. They now re-read **both frontmatter and body under the lock**, closing the TOCTOU window where concurrent edits to the outline markdown body could be overwritten. | `78670211` |
| **R-V172P0-QC1-001** | medium | `crates/nexus-daemon-runtime/src/api/handlers/outline.rs` | `OutlineFrontmatter.outline_revision` changed from `u64` to `i64` to match the `OutlinePatchResponse.new_revision` wire contract. Removed the silent `i64::try_from(new_revision).unwrap_or(0)` fallback in `patch_ok`. Added `outline_revision_u64()` helper for boundaries that still require unsigned values, returning an internal-error if the invariant is violated. | `78670211` |
| **R-V172P0-QC3-002** | medium | `apps/web/src/lib/canvas/use-outline-data.ts` | Mutation `onSuccess` callbacks now invalidate chapter queries in addition to the outline detail query. `usePatchOutlineStructure` and `usePatchOutlineChapter` invalidate `queryKeys.chapters.lists()`; `usePatchOutlineChapter` also invalidates `queryKeys.chapters.detail(workId, chapter)`. | `a6981dc8` |
| **R-V172P0-QC3-003** | medium | `apps/web/src/App.tsx` | `OutlinePage` is now route-split with `React.lazy` + `Suspense`, mirroring the `StrategyPage` pattern. The route `/works/:workId/outline` renders a loading fallback and loads the outline canvas as a separate chunk. | `7f615654` |

A regression unit test `patch_write_uses_body_from_locked_re_read` was added to `outline.rs` for R-V172P0-QC3-001.

---

## 2. Verification Evidence

All commands executed from `/Users/bibi/workspace/organizations/42ch/nexus` on branch `iteration/v1.72`.

### Rust

```text
$ cargo test -p nexus-daemon-runtime --test outline_api
   Compiling nexus-daemon-runtime v0.1.0
    Finished `test` profile
     Running tests/outline_api.rs
... all tests passed

$ cargo test -p nexus-daemon-runtime --lib outline::tests
    Finished `test` profile
     Running unittests src/lib.rs
... patch_write_uses_body_from_locked_re_read ... ok

$ cargo clippy --all -- -D warnings
    Finished `dev` profile

$ cargo +nightly-2026-06-26 fmt --all --check
(no output)

$ cargo test --all
... test result: ok. 762 passed; 0 failed
```

### TypeScript / Web

```text
$ pnpm --filter @42ch/nexus-contracts run build
✓ Built CJS + ESM + DTS

$ pnpm --filter web typecheck
> tsc --noEmit
(no errors)

$ pnpm --filter web build
... dist/assets/outline-page-CSOmw105.js  20.77 kB │ gzip: 5.69 kB
✓ built

$ pnpm --filter web test -- --run
 Test Files  20 passed (20)
      Tests  156 passed (156)
```

### Codegen

```text
$ pnpm run codegen
[INFO] Processed 153 schemas → TypeScript + Rust
✓ Codegen complete
```

No generated files changed; schema/contracts remain consistent.

### GitNexus

`gitnexus_detect_changes` after the final commit reported:

```text
No changes detected.
```

Pre-commit detection reported `high` risk, which is expected because the changed symbols are core outline patch handlers and the `App` route component; all affected processes fall within the intended V1.72 canvas/outline scope.

---

## 3. Known Limitations / Deferred Items

The following V1.72 P0/P1 residuals remain **open/deferred** and were **not** touched in this fix-wave:

| Residual | Severity | Decision | Target |
|----------|----------|----------|--------|
| R-V172P0-QC1-002 | low | defer | V1.73 canvas-outline-split backlog — `outline-canvas.tsx` monolith refactor |
| R-V172P0-QC3-004 | low | defer | V1.73 hygiene backlog — Tauri/adapter parity test count for new outline methods |
| R-V172P0-QC2-001 | medium | defer | V1.73 hygiene backlog — slug format validation |
| R-V172P0-QC2-002 | medium | defer | V1.73 hygiene backlog — target volume existence validation |
| R-V172P0-QC2-003 | medium | defer | V1.73 hygiene backlog — foreshadow temporal-order guard |
| R-V172P0-QC2-004 | medium | defer | V1.73 hygiene backlog — centralize published-chapter structural guard |

The production build still warns that `index-*.js` is >500 kB; this is a pre-existing Control Room bootstrap chunk size observation, not introduced by the outline lazy-split (which produced a separate 20.77 kB chunk).

---

## 4. Recommended Next Actions

1. **Targeted re-review** — Route the four fixed residuals to the original QC reviewers (qc1 for R-V172P0-QC1-001, qc3 for the other three) for focused re-review on `iteration/v1.72`.
2. **Regression test plan** — The new unit test `patch_write_uses_body_from_locked_re_read` should be run in CI as part of `nexus-daemon-runtime` tests. No additional manual QA is required for the frontend cache invalidation or route-split changes; existing web tests cover the hooks and routing infrastructure.
3. **Merge readiness** — Once qc1/qc3 approve, the `2026-06-28-v1.72-canvas-outline-timeline-beta` plan can advance to `Done` and be merged into `iteration/v1.72` (already committed there). Coordinate with the V1.72 hygiene plan before the integration branch merges to `main` via PR.
4. **Backlog grooming** — Move the six deferred residuals above into the V1.73 plan backlog and assign owners.

---

**Status:** V1.72 P0 fix-wave implementation complete; awaiting QC re-review.
