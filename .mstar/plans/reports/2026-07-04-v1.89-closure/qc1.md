---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-07-04-v1.89-closure"
verdict: "Approve"
generated_at: "2026-07-04"
---

# Code Review Report — V1.89 Deeper Manuscript Reading

## Reviewer Metadata

- Reviewer: `@qc-specialist` (Reviewer #1)
- Runtime Agent ID: `qc-specialist`
- Runtime Model: MiniMax-M3 (Minimax)
- Review Perspective: Architecture coherence and maintainability risk (DAO/handler design, body-ownership invariant, creator isolation, schema/contract consistency)
- Report Timestamp: 2026-07-04
- Deep review: triggered (S1: 4190 insertions / 67 files; S2: `migrations/` + `local_db/reading.rs`; S4: new SQLite DDL migration; S6: spans Rust backend + web frontend + wire contracts)

## Scope

- plan_id: `2026-07-04-v1.89-closure`
- Review range / Diff basis: `merge-base: e6f92049e9d725520cca529996edc89db5e82a96` → `tip: iteration/v1.89` (equivalent to `git diff e6f92049...iteration/v1.89`)
- Working branch (verified): `iteration/v1.89`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus` (from `git rev-parse --show-toplevel`)
- Files reviewed (in depth):
  - Rust backend: `crates/nexus-local-db/src/reading.rs`, `crates/nexus-local-db/src/lib.rs`, `crates/nexus-local-db/src/error.rs`, `crates/nexus-local-db/migrations/202607040002_reading_progress_and_annotations.sql`, `crates/nexus-daemon-runtime/src/api/handlers/reading.rs`, `crates/nexus-daemon-runtime/src/api/mod.rs`, `crates/nexus-daemon-runtime/src/api/errors.rs`, `crates/nexus-daemon-runtime/src/api/handlers/mod.rs`, `crates/nexus-daemon-runtime/tests/reading_api.rs`
  - Wire contracts: `schemas/local-api/reading/*.schema.json` (×8), `crates/nexus-contracts/src/generated/local_api/reading/*.rs` (×8), `crates/nexus-contracts/src/generated/local_api/mod.rs`, `crates/nexus-contracts/src/generated/mod.rs`, `crates/nexus-contracts/tests/schema_drift_detection.rs`
  - Web frontend: `apps/web/src/components/reading/{highlight-layer.tsx,annotation-toolbar.tsx,annotation-inspector.tsx,use-text-selection.ts,reading-prose.tsx}`, `apps/web/src/pages/chapter-page.tsx`, `apps/web/src/api/queries.ts`, `apps/web/src/lib/nexus/types.ts`, `apps/web/src/lib/nexus/browser-client.ts`, `apps/web/src/lib/nexus/query-keys.ts`
  - Spec/docs: `.mstar/knowledge/specs/web-ui.md` §28, `.mstar/plans/2026-07-04-v1.89-{closure,reading-depth-backend}.md`
- Files reviewed (spot-check): `apps/web/DESIGN.md`, `apps/web/DESIGN.dark.md`, `.mstar/status.json`, `.sqlx/*.json` (×2 new), `.mstar/iterations/v1.89-deeper-manuscript-reading-delivery-compass-v1.md`
- Commit range (12 commits since merge-base):
  - `b7980c3b docs(iteration): V1.89 iteration-start`
  - `8d0a512c docs(spec): §28 V1.89 Amendment`
  - `d64bb8d3 feat(schemas): reading progress and annotation contracts`
  - `0145d74e fix(migration): remove work_chapters FK per P-1 plan`
  - `f5883764 feat(iteration): merge P-1 Prepare for V1.89`
  - `8db7f1fd docs(status): P-1 Done, P0/P1 InProgress`
  - `7a014ded feat(reading): V1.89 reading depth Local API + web client surface`
  - `a44eb111 feat(iteration): merge P0 reading depth backend`
  - `6062046a feat(web): V1.89 Deeper Manuscript Reading frontend`
  - `8486855b feat(web): swap reading-api shim for real NexusClient methods`
  - `763f0def feat(iteration): merge P1 reading depth frontend`
  - `451b9024 docs(status): P0/P1 Done, P-last InProgress, QC milestone started`
- Tools run:
  - `cargo clippy --all -- -D warnings` → finished in 0.32s (clean)
  - `cargo test -p nexus-daemon-runtime -p nexus-local-db -p nexus-contracts` → 402 + 26 + 6 + 3 + … tests passed, 0 failed
  - `cargo test --test schema_drift_detection` → 4/4 passed (incl. `drift_detection_known_matched_passes` — all 8 reading schemas registered with `Strict` mode match)
  - `cargo test --test reading_api` → 12/12 passed (`get_progress_defaults_to_zero`, `put_and_get_progress_round_trip`, `delete_progress_clears_row`, `progress_unknown_work_returns_404`, `progress_requires_creator`, `annotation_create_list_patch_delete_round_trip`, `patch_annotation_can_clear_note`, `create_annotation_rejects_invalid_color`, `create_annotation_rejects_invalid_offsets`, `annotation_not_found_returns_404`, `annotation_cross_creator_returns_403`, `annotations_requires_creator`)
  - `cargo test -p nexus-local-db --lib reading::tests::…` → 4/4 passed (`test_progress_crud`, `test_progress_validation`, `test_annotation_crud`, `test_annotation_offset_validation`)
  - `pnpm run validate-schemas` → 194/194 valid (incl. 8 new reading schemas)
  - `pnpm --filter web run typecheck` → clean
  - `pnpm --filter web run test` → 53 test files / 403 tests passed (incl. `highlight-layer.test.tsx` (4), `chapter-page.test.tsx` (added coverage), `adapter-contract.test.ts` (25), `chapter-keyboard-nav.test.ts` (20))

## Findings

### 🔴 Critical

None.

### 🟡 Warning

- **[W-001] DAO `reading.rs` uses runtime `sqlx::query_as::<_, T>()` / `sqlx::query_scalar::<_, T>()` for static INSERT/UPDATE/SELECT** — `crates/nexus-local-db/src/reading.rs` lines 30, 63, 121, 159, 213, 273 use the runtime API for static SQL with no `// SAFETY:` justification. Project `crates/nexus-local-db/AGENTS.md` mandates: *"Compile-time checked queries only — use `sqlx::query!()` / `sqlx::query_as!()` for all static SQL. Runtime `sqlx::query()` only for DDL, PRAGMAs, or truly dynamic SQL with a `// SAFETY:` comment."* Only the two DELETE statements use `sqlx::query!()` (compile-time macro).
  - Source Type: doc-rule (`crates/nexus-local-db/AGENTS.md`)
  - Source Reference: `crates/nexus-local-db/src/reading.rs:30-46` (upsert_reading_progress), `:63-81` (get_reading_progress), `:121-148` (list_annotations), `:159-183` (get_annotation), `:213-247` (create_annotation), `:273-302` (update_annotation)
  - Confidence: High
  - Risk: Without compile-time validation, schema drift between DAO and DB will only surface at runtime (failed integration tests or 500s on the Local API). Project has a `WAIVER` for the V1.41 sqlx cache gap (see `nexus-local-db/AGENTS.md`), but that does not waive this rule — the rule is for the per-query compile-time macro itself, not just the cache. Pre-existing project-wide deviation (same pattern in `findings.rs`, `kb_extract_job.rs`, `kb_store.rs`, `knowledge_store.rs`, `narrative_gateway.rs` — `grep -c "sqlx::query_as::<\|sqlx::query_scalar::<"` returns 6 files with this pattern, all pre-V1.89).
  - Fix suggestion (non-blocking for V1.89): Migrate the six runtime queries in `reading.rs` to `sqlx::query_as!()` / `sqlx::query_scalar!()` in a follow-up "DAO compile-time macro conformance" plan. Or, accept the project-wide deviation and amend `nexus-local-db/AGENTS.md` to acknowledge the runtime-form pattern is the de-facto convention for `query_as`-typed results. Either way, **do not extend the deviation silently** — leaving it undocumented is the worse outcome.
  - **Severity rationale (non-blocking for this PR)**: The runtime queries are functionally correct (covered by 12 handler tests + 4 DAO tests + 402 other passing tests). This is a project-wide pattern; V1.89 merely conforms to the existing convention. The fix belongs in a separate plan, not in V1.89.

- **[W-002] Plan-vs-reality: migration filename** — Plan `.mstar/plans/2026-07-04-v1.89-reading-depth-backend.md` line 20 specifies the migration filename as `20260704_000002_reading_progress_and_annotations.sql` (with underscore separator, 6-digit sequence), but the actual file in the repo is `202607040002_reading_progress_and_annotations.sql` (no underscore, 4-digit sequence). The migration ordering still works lexicographically (`20260704_000001_*` < `202607040002_*`), but the inconsistency is a documentation drift.
  - Source Type: manual-reasoning
  - Source Reference: `.mstar/plans/2026-07-04-v1.89-reading-depth-backend.md:20` vs `crates/nexus-local-db/migrations/202607040002_reading_progress_and_annotations.sql`
  - Confidence: High
  - Fix suggestion: Update plan §A row "Migration" to reflect the actual filename, OR rename the file to `20260704_000002_reading_progress_and_annotations.sql` to match the documented convention. Either is acceptable; this is non-blocking.

- **[W-003] Plan-vs-reality: `volume` filter on progress GET not implemented** — Plan `.mstar/plans/2026-07-04-v1.89-reading-depth-backend.md` line 29 says `GET /v1/local/reading/progress` accepts an optional `volume` query parameter: *"`volume` (optional filter for edge cases — the primary key is `(creator_id, work_id, chapter)`)."* The actual implementation has no `volume` parameter in `ReadingProgressQuery`, the migration `reading_progress` table has no `volume` column, the DAO signature has no `volume`, and the handler does not consume it. The contract `schemas/local-api/reading/reading-progress-query.schema.json` likewise has no `volume` field. Scope was reduced without a plan update.
  - Source Type: manual-reasoning
  - Source Reference: `.mstar/plans/2026-07-04-v1.89-reading-depth-backend.md:29` vs `schemas/local-api/reading/reading-progress-query.schema.json` (no `volume`)
  - Confidence: High
  - Risk: Low — the chapter number is already volume-aware (see `work_chapters` module's `next_chapter_volume_aware`), so the absence of an explicit `volume` filter is consistent with the rest of the codebase.
  - Fix suggestion: Update plan §B row "GET /v1/local/reading/progress" to remove the `volume` reference, or add an explicit "deferred from V1.89" note. This is a doc-only fix.

### 🟢 Suggestion

- **[S-001] Migration filename format consistency** — Recent migrations follow the pattern `YYYYMMDD_NNNNNN_description.sql` (e.g., `20260704_000001_memory_soul_narratives_composite_world_key.sql`, `202606070001_work_chapters.sql`). The new migration `202607040002_reading_progress_and_annotations.sql` uses `YYYYMMDDNNNN_description.sql` (no underscore, 4-digit seq). Both lex orders sort the same, but the pattern drift is worth normalizing.
  - Fix: Rename to `20260704_000002_reading_progress_and_annotations.sql` (and update the plan reference). Non-blocking.

- **[S-002] `update_annotation` DAO uses two round-trips** — `crates/nexus-local-db/src/reading.rs:262-303` calls `get_annotation(pool, annotation_id)` first, then `fetch_one` an UPDATE with the merged color/note. For high-frequency patches this is two DB round-trips. Could be merged into a single `UPDATE … WHERE annotation_id = ?1 RETURNING …` with `COALESCE(?, color)` / `COALESCE(?, note)` for partial updates, returning `None` when the row doesn't exist.
  - Source Type: manual-reasoning
  - Confidence: Medium
  - Risk: Low — patches are user-driven, not high-frequency. Optimization is premature for V1.89.
  - Fix: Defer to a future DAO optimization pass.

- **[S-003] Defense-in-depth `u64_to_i64` helper in handler** — `crates/nexus-daemon-runtime/src/api/handlers/reading.rs:34-39` converts `u64 → i64` for `start_offset`, `end_offset`, `scroll_progress`. The schema bounds (`max 10000` for progress, offset semantics) and DAO validation (`0..=10000`) make overflow practically impossible. The defensive check is harmless and makes the wire→DB contract explicit.
  - Fix: Keep as-is; the doc comment ("defense-in-depth; schema bounds keep values well within range") makes the intent clear.

- **[S-004] `ReadingProgressResponse.updated_at` semantics on GET** — `crates/nexus-daemon-runtime/src/api/handlers/reading.rs:139-142` returns `chrono::Utc::now()` as `updated_at` when no row exists. This means successive GETs of the same chapter (without intervening PUT) yield different `updated_at` strings, which may surprise caching clients. Two reasonable behaviors:
  1. Return a stable sentinel (e.g., empty string or a fixed epoch) when no row exists.
  2. Document the "live updated_at" behavior explicitly in `ReadingProgressResponse` schema description.
  - Source Type: manual-reasoning
  - Confidence: Medium
  - Risk: Low — client caching of the "no progress" sentinel is unlikely; the web client re-reads progress on chapter navigation, not on a timer.
  - Fix: Discuss with PM. If clients are expected to cache the "no progress" GET, return a stable string; otherwise document the live-clock behavior.

## Source Trace (representative)

| Finding | Source Type | Source Reference | Confidence |
|---------|-------------|------------------|------------|
| W-001 | doc-rule | `crates/nexus-local-db/AGENTS.md` ("Compile-time checked queries only") + `crates/nexus-local-db/src/reading.rs:30,63,121,159,213,273` | High |
| W-002 | manual-reasoning | `.mstar/plans/2026-07-04-v1.89-reading-depth-backend.md:20` vs `crates/nexus-local-db/migrations/202607040002_reading_progress_and_annotations.sql` | High |
| W-003 | manual-reasoning | `.mstar/plans/2026-07-04-v1.89-reading-depth-backend.md:29` vs `schemas/local-api/reading/reading-progress-query.schema.json` | High |
| S-001 | manual-reasoning | recent migration pattern `20260704_000001_*.sql` vs new `202607040002_*.sql` | High |
| S-002 | manual-reasoning | `crates/nexus-local-db/src/reading.rs:262-303` | Medium |
| S-003 | manual-reasoning | `crates/nexus-daemon-runtime/src/api/handlers/reading.rs:34-39` | Medium |
| S-004 | manual-reasoning | `crates/nexus-daemon-runtime/src/api/handlers/reading.rs:139-142` | Medium |

## Lenses Applied

As QC #1 (architecture/maintainability), I applied:

- **Modularity Lens** (default): DAO + handler separation is clean. The DAO has zero dependency on HTTP types; the handler has zero dependency on filesystem or body-writing APIs. Routes are isolated in a dedicated `reading_routes()` function in `crates/nexus-daemon-runtime/src/api/mod.rs:253-269`, merged under `protected_routes` (auth-gated). No circular deps; no shared mutable state. `AnnotationRow` is the only cross-crate struct and is correctly re-exported via `nexus_local_db::reading::AnnotationRow`.
- **Contract Lens** (default): All 8 schemas registered in `build_schema_map()` with `Strict` mode. Generated Rust types committed. Generated TypeScript types committed (in `packages/nexus-contracts/src/generated/local-api/reading/`). Schema drift detection passes. Handler DTOs (e.g., `to_annotation_dto` in `reading.rs:82-95`) match the schema fields exactly, including the optional `note` skip-serialization pattern.
- **Data Migration Lens** (S4 trigger): Migration `202607040002_reading_progress_and_annotations.sql` is clean — `CREATE TABLE IF NOT EXISTS`, `CHECK` constraints matching the DAO validation, foreign keys to `works(work_id) ON DELETE CASCADE`, and two indexes (`idx_reading_progress_work_chapter`, `idx_reading_annotations_work_chapter`, `idx_reading_annotations_creator`). The migration is idempotent and reversible in spirit (no data migration; pure schema). `run_migrations` already runs `PRAGMA foreign_key_check` (V1.67 P2 fix), and the test `migrations_leave_no_foreign_key_violations` continues to pass.
- **Auth Lens** (S2 trigger): All handlers call `require_active_scope(state)` to derive `creator_id` from the active session (returns 401 if no active creator). All work-scoped handlers call `verify_work_ownership` (returns 404 if work doesn't exist for this creator). Annotation PATCH/DELETE call `load_annotation_for_creator` which checks `row.creator_id != creator_id` and returns 403 (verified by integration test `annotation_cross_creator_returns_403`). No cross-creator leakage possible.
- **Input Validation Lens** (S2 trigger): Handler validates `color` against the `{yellow, blue, green, pink}` enum (422 on miss), validates `start_offset < end_offset` via DAO (422 on miss via `LocalDbError::ValidationError → BadRequest(invalid_input) → 422`), validates `scroll_progress` range (DAO-side), and converts `u64 → i64` with explicit error message. All errors flow through `NexusApiError::BadRequest` with code `invalid_input` (status 422 per `errors.rs:254`).
- **Error Handling Lens**: Handlers exclusively use `NexusApiError` (per `crates/nexus-daemon-runtime/AGENTS.md` R-V167P0-QC1-S-AGENTS). No ad-hoc JSON error bodies. `From<LocalDbError>` is in `errors.rs:528-541`, mapping `ValidationError → BadRequest(invalid_input) → 422` and other DB errors → `Internal(DATABASE_ERROR) → 500`. The `query!`/`query_as!` macros both propagate errors via `?` which then `From`s into `NexusApiError` via the existing `From<sqlx::Error>` impl.

## Body-Ownership Invariant Verification

The plan states: *"no route writes to `body_path`, outline files, or chapter content. The body-ownership invariant (canvas is the sole authoring surface) is non-negotiable."*

**Verified**: `grep -n "body_path\|outline\|patchChapter\|body.write\|bodyFile\|patch_work\|fs::write\|tokio::fs" crates/nexus-daemon-runtime/src/api/handlers/reading.rs crates/nexus-local-db/src/reading.rs` returns **zero matches**. The handler only imports:
- `crate::api::errors::NexusApiError`
- `crate::api::handlers::works::{read_active_creator_id, read_active_workspace_slug}`
- `crate::workspace::WorkspaceState`
- `axum::extract::{Path, Query, State}`, `axum::Json`
- `nexus_contracts::local_api::reading::*` (read-side DTOs)
- `nexus_local_db::reading::{self, AnnotationRow}` (DAO functions)
- `nexus_local_db::works` (only for `verify_work_ownership` work-existence check)
- `uuid::Uuid`

No filesystem imports. No body-mutation imports. The DAO only touches `reading_progress` and `reading_annotations` tables (verified by reading the migration — both have `FOREIGN KEY (work_id) REFERENCES works(work_id) ON DELETE CASCADE`, but no FK to `work_chapters` or any body file).

On the web side, `apps/web/src/components/reading/` has zero references to `patchChapter`, `patch_work`, or any write-path client method. `HighlightLayer` only wraps DOM ranges in `<mark>` elements (visual only, no body mutation). `useTextSelection` only reads DOM selection. The reading surface is strictly read-side.

**Conclusion**: Body-ownership invariant upheld end-to-end across the Rust handler, DAO, and web components.

## Creator Isolation Verification

The plan states: *"Auth gate | All DAO calls are creator-scoped; handlers enforce `active_creator_id` matches."*

**Verified**:
1. `require_active_scope(&state)` at the top of every handler — returns 401 if no active creator.
2. `verify_work_ownership(&state, &creator_id, &work_id)` for work-scoped reads/writes — returns 404 if work doesn't belong to active creator (does not leak existence to wrong-creator calls).
3. `load_annotation_for_creator(&state, &creator_id, &annotation_id)` for annotation PATCH/DELETE — returns 404 if annotation doesn't exist (generic), 403 if exists but `row.creator_id != active creator_id`. The split prevents enumeration.
4. DAO queries use `WHERE creator_id = ?1` for `reading_progress` (read) and `WHERE creator_id = ?1 AND work_id = ?2 AND chapter = ?3` for `reading_annotations` list. Cross-creator row creation is impossible because the handler binds `&creator_id` from the active session (never from request body).
5. Integration test `annotation_cross_creator_returns_403` verifies the cross-creator patch/delete path returns 403.

**Conclusion**: Creator isolation enforced at handler boundary and DAO query level; verified by integration tests.

## Schema/Contract Consistency Verification

| Schema | Generated Rust type | Generated TS type | Drift check | Status |
|--------|---------------------|-------------------|-------------|--------|
| `reading-progress-query.schema.json` | `ReadingProgressQuery { work_id: String, chapter: i64 }` | `ReadingProgressQuery` | Strict | ✓ |
| `reading-progress-request.schema.json` | `ReadingProgressRequest { work_id, chapter, scroll_progress }` | `ReadingProgressRequest` | Strict | ✓ |
| `reading-progress-response.schema.json` | `ReadingProgressResponse { work_id, chapter, scroll_progress, updated_at }` | `ReadingProgressResponse` | Strict | ✓ |
| `reading-annotation.schema.json` | `ReadingAnnotation { annotation_id, work_id, chapter, start_offset, end_offset, selected_text, color, note?, created_at, updated_at }` | `ReadingAnnotation` | Strict | ✓ |
| `reading-annotation-create-request.schema.json` | `ReadingAnnotationCreateRequest { work_id, chapter, start_offset, end_offset, selected_text, color, note? }` | `ReadingAnnotationCreateRequest` | Strict | ✓ |
| `reading-annotation-patch-request.schema.json` | `ReadingAnnotationPatchRequest { color?, note? }` | `ReadingAnnotationPatchRequest` | Strict | ✓ |
| `reading-annotation-list-query.schema.json` | `ReadingAnnotationListQuery { work_id, chapter }` | `ReadingAnnotationListQuery` | Strict | ✓ |
| `reading-annotation-list-response.schema.json` | `ReadingAnnotationListResponse { items: Vec<ReadingAnnotation> }` | `ReadingAnnotationListResponse` | Strict | ✓ |

All 8 schemas registered in `crates/nexus-contracts/tests/schema_drift_detection.rs:448-488` with `Strict` mode. `cargo test --test schema_drift_detection` passes 4/4. `pnpm run validate-schemas` reports 194/194 valid.

Handler DTO mapping (`to_annotation_dto` in `reading.rs:82-95`):
- DB `AnnotationRow.start_offset` (i64) → DTO `start_offset` (u64) via `u64::try_from(...).unwrap_or_default()` — acceptable; i64→u64 widening is well-defined for non-negative values and `CHECK (start_offset >= 0)` is enforced at DB level.
- DB `selected_text` (String) → DTO `selected_text` (String) — direct.
- DB `note` (Option<String>) → DTO `note` (Option<String>) — direct, `#[serde(skip_serializing_if = "Option::is_none")]` matches generated type's skip rule.
- DB `created_at`/`updated_at` (DateTime<Utc>) → DTO `created_at`/`updated_at` (String) via RFC3339 — schema `description: "ISO 8601 creation timestamp"`.

**Conclusion**: Schema/contract consistency verified end-to-end. No drift between schemas, generated types, and handler DTO mapping.

## CI Gates Verification

| Gate | Command | Result |
|------|---------|--------|
| Rust clippy (strict) | `cargo clippy --all -- -D warnings` | ✓ clean |
| Rust tests (3 crates) | `cargo test -p nexus-daemon-runtime -p nexus-local-db -p nexus-contracts` | ✓ 402 + 26 + 6 + 3 + … tests pass, 0 fail |
| Rust schema drift | `cargo test --test schema_drift_detection` | ✓ 4/4 |
| Rust reading API integration | `cargo test --test reading_api` | ✓ 12/12 |
| Rust DAO unit | `cargo test -p nexus-local-db reading::` | ✓ 4/4 |
| TS schema validation | `pnpm run validate-schemas` | ✓ 194/194 valid |
| TS web typecheck | `pnpm --filter web run typecheck` | ✓ clean |
| TS web test | `pnpm --filter web run test` | ✓ 403/403 (53 files) |

All verification commands in the Assignment pass. No CI gate blockers.

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 3 |
| 🟢 Suggestion | 4 |

**Verdict**: **Approve**

The V1.89 implementation is functionally complete, schema-clean, clippy-clean, and well-tested across both backend (12 handler tests + 4 DAO tests + 4 drift tests + 402 other backend tests) and frontend (403 web tests including 4 highlight-layer tests). The body-ownership invariant is upheld end-to-end — handler has zero filesystem or body-mutation imports; web components only read DOM and use the `NexusClient` interface. Creator isolation is enforced at the handler boundary (`require_active_scope` → `verify_work_ownership` → `load_annotation_for_creator`) and verified by the cross-creator 403 test. Schema/contract consistency is verified: all 8 schemas are `Strict`-drift-clean, generated types committed, handler DTO mapping aligns field-for-field.

The three warnings are documentation drifts (plan filename mismatch, `volume` filter documented but not implemented) and a pre-existing project-wide convention deviation (runtime `sqlx::query_as`/`query_scalar` for static SQL) that V1.89 merely conforms to. None block approval — they belong in a follow-up plan to either normalize the DAO macro convention project-wide or amend `nexus-local-db/AGENTS.md` to acknowledge the de-facto pattern.