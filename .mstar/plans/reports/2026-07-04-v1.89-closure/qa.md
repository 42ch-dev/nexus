# QA Report — V1.89 Deeper Manuscript Reading

**plan_id**: `2026-07-04-v1.89-closure`
**Working branch**: `iteration/v1.89`
**Review range / Diff basis**: `merge-base: e6f92049e9d725520cca529996edc89db5e82a96` → `tip: iteration/v1.89`
**Review cwd**: `/Users/bibi/workspace/organizations/42ch/nexus`
**Agent**: qa-engineer
**Date**: 2026-07-04

## Scope tested
V1.89 author-visible reading depth features (persisted scroll progress, text-selection highlights/annotations, annotation inspector, drift notice) plus technical invariants (body-ownership, creator isolation, migration hygiene, V1.79 UX regression protection).

## Automated test gates

| Gate | Command | Result | Evidence |
|------|---------|--------|----------|
| Rust crates | `cargo test -p nexus-daemon-runtime -p nexus-local-db -p nexus-contracts` | **PASS** | 1159+ tests passed (93 contracts + 402+ daemon + local-db reading tests + schema drift + reading_api 12/12). Full run clean. |
| Clippy | `cargo clippy --all -- -D warnings` | **PASS** | Clean (0 warnings). |
| Format | `cargo +nightly-2026-06-26 fmt --all --check` | **PASS** | No output (clean). |
| Schemas | `pnpm run validate-schemas` | **PASS** | 194/194 valid (incl. 8 new reading schemas). |
| Web typecheck | `pnpm --filter web run typecheck` | **PASS** | Clean after contracts/ui rebuild. |
| Web tests | `pnpm --filter web run test` | **PASS** | 53 files / 403 tests passed (incl. V1.89 reading coverage). |

All gates **PASS**.

## Author-visible outcomes (verified)

| Outcome | Evidence (test name / code location) | Status |
|---------|-------------------------------------|--------|
| Reopening a chapter restores the last scroll position | `chapter-page.test.tsx:601` "restores persisted scroll progress on load" — sets `scrollProgress: 5_000`, expects `window.scrollTo({ top: 500 })`.<br>`api/queries.ts:884-888` `useReadingProgressSync` restores ratio from `progress.data.scroll_progress`.<br>Backend: `get_reading_progress` + DAO round-trip. | **Verified** |
| Selecting text creates a highlight | `chapter-page.test.tsx:652` "creates a highlight from text selection" — `selectTextInProse(5,9)`, clicks toolbar, POST `/v1/local/reading/annotations` with offsets + `selected_text`.<br>`annotation-toolbar.tsx` + `use-text-selection.ts` + handler `create_annotation`. | **Verified** |
| Highlights persist after navigation and reappear on return | `annotation-inspector.test.tsx:30` "renders all annotations".<br>DAO `list_annotations` (WHERE creator+work+chapter) + handler `list_annotations`.<br>`HighlightLayer` reapplies marks on body/annotations change (layout effect).<br>Chapter-page re-fetches annotations per chapter. | **Verified** |
| Annotation inspector lists highlights with color + note | `annotation-inspector.test.tsx:30` + component renders swatch + `selected_text` + `note` + timestamp.<br>`annotation-inspector.tsx:140-154`. | **Verified** |
| Editing note/color updates the highlight | `annotation-inspector.test.tsx:47` "calls onUpdate with id and patch when note or color changes" — edits → `onUpdate('a-1', { note: 'new note', color: 'yellow' })`.<br>Handler `patch_annotation` → DAO `update_annotation`. | **Verified** |
| Deleting a highlight removes it | `annotation-inspector.test.tsx:69` "calls onDelete when delete is clicked".<br>`reading_api.rs:250-264` round-trip: create → list(1) → delete → list(0). | **Verified** |
| Drift notice appears when annotation offsets exceed current body length | `highlight-layer.test.tsx:34` "renders a non-blocking drift notice when an annotation is out of bounds".<br>`highlight-layer.tsx:75` `hasDrift = some(end_offset > bodyLength)`; renders `<div role="note">` with bilingual message; skips mark. | **Verified** |

## Technical invariants (verified)

| Invariant | Evidence | Status |
|-----------|----------|--------|
| Reading surface has no body/outline write routes | `chapter-page.test.tsx:540` "does not offer any write affordance — only the canvas redirect (body-ownership invariant)".<br>`chapter-page.test.tsx:772` same assertion post-V1.89.<br>Only routes: GET body, PUT reading progress (separate table), annotation CRUD. No `putChapter`, no body editor, no outline writes. Canvas is sole authoring surface (per web-ui.md §28). | **Verified** |
| DAO/handlers enforce creator isolation | `reading.rs:111-116` `load_annotation_for_creator` → 403 if `row.creator_id != creator_id`.<br>All handlers: `require_active_scope` + `verify_work_ownership`.<br>DAO queries: `WHERE creator_id = ?1`.<br>Tests: `progress_requires_creator`, `annotation_cross_creator_returns_403`, `annotations_requires_creator`, `reading_api.rs:195`. | **Verified** |
| Migration applies cleanly to fresh and existing V1.88 DBs | Migration `202607040002_reading_progress_and_annotations.sql`: `CREATE TABLE IF NOT EXISTS`, `CREATE INDEX IF NOT EXISTS` (idempotent, additive).<br>QC3: "Migration is idempotent ... purely additive; compatible with V1.88 DBs."<br>Full test suite (incl. prior migration tests) + no breakage on V1.89 DB init. | **Verified** |
| No regression in V1.79 reading UX (chapter nav, keyboard nav, maturation indicators) | `chapter-nav.test.tsx` (8 tests), `chapter-keyboard-nav.test.ts` (20 tests) still pass.<br>`chapter-page.test.tsx:520-570` maturation indicators (KB density + findings counts) render from existing data.<br>`pnpm --filter web run test` 403/403 passed (no V1.79 breakage introduced). | **Verified** |

## Known QC suggestions (non-blocking; observed but not gating)

- S1 (qc3): Progress-saved toast fires on every debounced scroll save (`queries.ts:793`).
- S2 (qc3): Highlight rendering is O(annotations × text-nodes) (`highlight-layer.tsx` + `rangeFromOffsets`).
- S3 (qc3): Drift notice uses inline amber colors instead of namespaced token.
- S4 (qc3): Drift notice is bilingual while sibling strings are English-only.
- W-001 (qc1): Compile-time sqlx macro conformance warning in DAO (`reading.rs` uses runtime `query_as` for static SQL — project-wide pre-existing pattern).

None of these are blocking for V1.89 (explicitly noted as non-blocking in QC reports).

## Not tested (out of scope for this QA)
- Full end-to-end with real Tauri/desktop shell (browser MSW tests cover the surface).
- Large-manuscript performance (O(n) highlight walk is documented as MVP).
- Offline / stub-creator modes (future guard noted in qc3 S5).
- Volume-aware progress filter (plan scope reduced; chapter number is already volume-aware).

## Verdict
**PASS**

All automated gates green. All 7 author-visible outcomes verified via test names + code locations. All 4 technical invariants verified. No regressions in prior reading UX. Non-blocking suggestions documented but do not prevent sign-off.

## Artifacts
- QA report: `.mstar/plans/reports/2026-07-04-v1.89-closure/qa.md` (this file)
- QC reports (already present): `qc1.md`, `qc2.md`, `qc3.md` (all Approve)
- Status: P0/P1/P-last Done per prior commits

## Recommended owners (if follow-ups)
- S1–S4 / W-001: future "reading polish" or "DAO compile-time macro" plan (non-blocking for V1.89 closure).
- Bilingual drift notice: align with Voice & Content policy when i18n slice lands.

**QA complete. Ready for plan closure + merge to target.**
