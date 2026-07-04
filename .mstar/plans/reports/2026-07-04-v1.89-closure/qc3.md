---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: "2026-07-04-v1.89-closure"
verdict: "Approve"
generated_at: "2026-07-04"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-3
- Runtime Agent ID: qc-specialist-3
- Runtime Model: ark-code-latest (volcengine-plan/ark-code-latest)
- Review Perspective: Performance, reliability, maintainability, and DESIGN.md token consistency (hot-path efficiency, DOM/query churn, debounce/flush behavior, migration idempotency, DAO/handler cohesion, token SSOT chain DESIGN.md -> index.css -> components, Voice & Content compliance).
- Report Timestamp: 2026-07-04

## Scope
- plan_id: 2026-07-04-v1.89-closure
- Review range / Diff basis: merge-base `e6f92049e9d725520cca529996edc89db5e82a96` -> tip `94dac9a3b01da6bddcad7a34fb0fd744da16aeeb` on `iteration/v1.89` (14 commits including qc1/qc2 report commits).
- Working branch (verified): iteration/v1.89
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: full V1.89 diff, with focused inspection of `crates/nexus-local-db/src/reading.rs`, migration `202607040002_reading_progress_and_annotations.sql`, `crates/nexus-daemon-runtime/src/api/handlers/reading.rs`, `apps/web/src/components/reading/*`, `apps/web/src/pages/chapter-page.tsx`, `apps/web/src/api/queries.ts`, `apps/web/src/lib/nexus/browser-client.ts`, and the token SSOT chain (`apps/web/DESIGN.md`, `apps/web/DESIGN.dark.md`, `apps/web/src/index.css`). Prior reports qc1.md, qc2.md cross-referenced.
- Tools run (all green):
  - `cargo clippy --all -- -D warnings` -> 0 warnings
  - `cargo test -p nexus-daemon-runtime -p nexus-local-db -p nexus-contracts` -> 1159 passed, 0 failed, 1 ignored
  - `pnpm run validate-schemas` -> 194/194 valid
  - `pnpm --filter web run typecheck` -> clean
  - `pnpm --filter web run test` -> all suites passing (pre-existing React Router v7 future-flag warnings only)

## Findings

### Critical
None.

### Warning
None.

### Suggestion
(details in appended section below)

- **S1 (Low, UX/reliability) — Success toast fires on every debounced scroll save.** `apps/web/src/api/queries.ts:793` unconditionally calls `toast({ variant: 'success', title: 'Progress saved' })` in the `useSaveReadingProgress` mutation. `useReadingProgressSync` fires this every ~500 ms during scroll (line 899) plus on `beforeunload` and `visibilitychange -> hidden`. Repeated toasts undermine the silent-persistence model the sync hook implements. The hook already declares an unused `showSavedToast?: boolean` option (line 869) that is never threaded into the mutation. Non-blocking. Follow-up: drop the toast in the sync path by default and keep it only for explicit user-triggered saves, or actually consume `showSavedToast` at the call site.

- **S2 (Low, performance) — Highlight rendering is O(annotations x text-nodes) per change.** `HighlightLayer` (`highlight-layer.tsx:77-81`) does a full `clearHighlights` DOM walk then `applyHighlights` on every annotation-list or body-length change; `rangeFromOffsets` (`use-text-selection.ts:119`) walks the entire text-node tree. Fine at MVP scale. Post-MVP: apply deltas against a memoized in-bounds list, or adopt the CSS Custom Highlight API.

- **S3 (Low, maintainability) — Amber colors in drift notice bypass the reading-namespace token pattern.** `highlight-layer.tsx:87` uses `border-amber-700/30 bg-amber-700/10 text-amber-1000`. Values are valid DESIGN.md palette entries and match the DESIGN.md warning recipe (rgba(183,110,0,0.12) / amber-1000, lines 133, 235, 395). But every other V1.89 reading affordance uses namespaced CSS variables (`--color-reading-annotation-highlight-*`, `--color-reading-annotation-inspector-*`, `--color-reading-selection-toolbar-*`). Follow-up: add `reading-drift-notice-{background,border,text}` in DESIGN.md/DESIGN.dark.md/`index.css`, or project `components.warning` as a CSS variable.

- **S4 (Info, Voice & Content) — Drift notice is bilingual and diverges from sibling copy.** `highlight-layer.tsx:91` renders a bilingual Chinese-plus-English drift message. Sibling reading strings ("Loading chapter...", "Progress saved", "Highlight deleted", "Could not create highlight") are English-only per `apps/web/AGENTS.md` Voice & Content. Follow-up: standardize on English-only, or centralize a bilingual pattern in a future i18n slice.

- **S5 (Info) — Future-mode note.** `useReadingProgress` fires on every chapter open with `enabled: Boolean(workId) && chapter > 0` — correct for the current always-loopback model. If an offline/stub-creator mode is introduced later, guard against 401/404 chatter here. No change today.

## Source Trace
- S1 -> `apps/web/src/api/queries.ts:779-797, 866-926`
- S2 -> `apps/web/src/components/reading/highlight-layer.tsx:35-81` + `use-text-selection.ts:119-157`
- S3 -> `apps/web/src/components/reading/highlight-layer.tsx:85-93` + `apps/web/DESIGN.md:40-43,133,235,395` + `apps/web/src/index.css:170-184,334-344`
- S4 -> `apps/web/src/components/reading/highlight-layer.tsx:91` + `apps/web/AGENTS.md` (Voice & Content section)
- S5 -> `apps/web/src/api/queries.ts:770-777`
- Source Type: manual code inspection + verification commands (clippy, workspace tests, schema validation, web typecheck + tests) + token SSOT trace (DESIGN.md -> index.css -> component consumption).
- Confidence: High. All findings are low-severity, non-blocking observations.

## Summary
| Severity | Count |
|----------|-------|
| Critical | 0 |
| Warning  | 0 |
| Suggestion | 5 (all non-blocking) |

**Verdict**: Approve

## Detailed Review Notes

### Performance & hot paths
- Reading-progress upsert is a single `INSERT ... ON CONFLICT(creator_id, work_id, chapter) DO UPDATE ... RETURNING updated_at` — one round-trip, index-backed by the primary key. Efficient at the ~2 Hz frontend debounce.
- Annotation list is filtered by `(creator_id, work_id, chapter)`; the migration declares an index on the same columns for `reading_annotations`. No table scan on the read path.
- 500 ms scroll debounce with `beforeunload` + `visibilitychange -> hidden` flush survives tab close and SPA navigation. `SCROLL_PROGRESS_UNIT` (0-10000 integer) keeps wire payloads compact and matches the DB CHECK.
- `useSaveReadingProgress` writes the response back into the query cache via `qc.setQueryData` instead of invalidating — avoids a follow-up GET round-trip.

### Reliability
- Migration is idempotent (`CREATE TABLE IF NOT EXISTS`, `CREATE INDEX IF NOT EXISTS`) and purely additive; compatible with V1.88 DBs.
- CHECK constraints (`scroll_progress BETWEEN 0 AND 10000`, `start_offset >= 0`, `end_offset > start_offset`) provide defense-in-depth.
- Handler helpers `u64_to_i64` (400 on overflow) and `validate_color` (whitelist yellow/blue/green/pink) prevent malformed writes reaching the DAO.
- `ON DELETE CASCADE` on `work_id` FK guarantees no orphaned reading state after work deletion.
- Drift semantics: highlights whose `end_offset > current bodyLength` are skipped and a non-blocking notice is shown; the annotation row survives so a future body restore re-projects it. Matches the spec's "no auto-reconcile in MVP" stance.

### Maintainability
- DAO / handler split is clean: DAO returns typed rows; handlers own auth (`require_active_scope`, `verify_work_ownership`, `load_annotation_for_creator`) and input validation.
- New `BrowserClient` methods (7) follow the existing shape under a clearly commented "Reading depth (V1.89)" section.
- React components are small and single-purpose (`HighlightLayer`, `AnnotationToolbar`, `AnnotationInspector`, `useTextSelection`); state ownership sits at `ChapterPage`.
- Two low-cost drifts (S3 amber tokens, S4 bilingual copy) do not affect correctness but are worth cleaning up in a follow-up pass.

### DESIGN.md token consistency
- V1.89 introduces `reading-annotation-highlight-{yellow,blue,green,pink}`, `reading-annotation-inspector-*`, and `reading-selection-toolbar-*` in DESIGN.md + DESIGN.dark.md; `apps/web/src/index.css` projects them as CSS variables in both light (lines 170-184) and dark (lines 334-344) blocks. Components consume via `bg-[var(--color-reading-annotation-highlight-<color>-background)]` — no fabricated values, no hard-coded hexes. SSOT chain intact.
- Sole drift: drift-notice inline utilities (S3) — small scope, non-blocking.
- Dark-mode tokens raise color-mix percentages (18-22 %) for highlight backgrounds so accents stay legible on the dark surface.

## Conclusion
V1.89 (persisted reading progress + character-offset annotations) is well-engineered on the axes I own: hot paths are index-backed and O(1)-per-operation, debounce/flush semantics are correct across tab-close and navigation, the migration is idempotent and additive with strong CHECK constraints and FK CASCADE, module boundaries are clean, and the DESIGN.md token SSOT chain is preserved throughout the reading surface. Five low-severity suggestions surfaced (one UX/reliability, one perf, one token drift, one copy consistency, one future-mode note) — none block the closure gate.

**Verdict**: Approve
