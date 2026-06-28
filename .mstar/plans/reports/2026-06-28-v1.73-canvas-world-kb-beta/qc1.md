---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-06-28-v1.73-canvas-world-kb-beta"
verdict: "Approve"
generated_at: "2026-06-29"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist (Seat 1)
- Runtime Agent ID: qc-specialist
- Runtime Model: minimax-cn-coding-plan/MiniMax-M3
- Review Perspective: Architecture coherence and maintainability risk
- Report Timestamp: 2026-06-29T02:30:00Z

## Scope
- plan_id: 2026-06-28-v1.73-canvas-world-kb-beta
- Review range / Diff basis: merge-base: 87ab75bb (origin/main) ... tip: d04a6b4e (HEAD) — equivalent to `git diff 87ab75bb...d04a6b4e`
- Working branch (verified): iteration/v1.73
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus (per `git rev-parse --show-toplevel`)
- Files reviewed: 95 changed, +7708 / −660 lines (P0 world-kb 30+ new files; P1 outline-canvas split 7 new files)
- Commit range (if not identical to Review range line, explain): identical to Review range
- Tools run:
  - `cargo clippy -p nexus-daemon-runtime -p nexus-local-db -p nexus-contracts -- -D warnings` → CLEAN
  - `pnpm --filter web typecheck` → CLEAN
  - `cargo test -p nexus-daemon-runtime --test world_kb_patch` → 9/9 PASS
  - `cargo test -p nexus-daemon-runtime --test outline_patch` → 15/15 PASS
  - `cargo test -p nexus-local-db --lib kb_store` → 25/25 PASS
  - `cargo test -p nexus-contracts --test schema_drift_detection` → 4/4 PASS
  - `pnpm --filter web test` → 191/191 PASS (includes 25 world-kb vitest + 10 outline-canvas vitest)

## Findings

### 🔴 Critical
None.

### 🟡 Warning
None.

### 🟢 Suggestion

- **F-S1**: Naming consistency drift between JSON Schema `title` and generated type names for World KB request DTOs. The schemas at `schemas/local-api/canvas/world-kb/world-kb-patch-entity-request.schema.json` declare `"title": "Nexus PatchWorldKbEntityRequest"` (verb-prefix style, matching the V1.71/V1.72 `StrategyPatchStateRequest` / `OutlinePatchStructureRequest` convention), but `pnpm run codegen` emits the struct `WorldKbPatchEntityRequest` and TS interface `WorldKbPatchEntityRequest` (entity-prefix style). The same shape lives under two names in the repository: schema `title` says `Patch…` while the Rust/TS code says `…Patch…`. All 13 World KB schema titles appear affected. The implementer noted the discrepancy in the plan and let codegen output govern; the schema `title` fields still mismatch the V1.71/V1.72 convention. **Fix (non-blocking)**: align either (a) update the schema `title` strings to match the generated names (`Nexus WorldKbPatchEntityRequest` etc.), or (b) update the codegen template to read the schema `title` as the canonical struct name. (b) would restore V1.71/V1.72 naming across all four canvas surfaces. The implementer's documented choice is acceptable as a tech-debt note, but it is worth a one-line decision in a follow-up plan.

- **F-S2**: `apps/web/src/components/canvas/world-kb/world-kb-canvas.tsx` is **546 lines** and includes 4 inline sub-components (`WorldKbHeader` 62 lines, `InspectorPanel` 69 lines, `EntityConflictHost` 24 lines, `PromoteConflictHost` 24 lines) plus 4 helper functions (`nodesToData`, `patchFromForm`, `safeJson`, `formatRelative`). The main `WorldKbCanvas` orchestrator function is itself ~240 lines (within the ≤250-line cap). This deviates from the V1.71 strategy-canvas pattern (orchestrator 187 lines, sub-modules in `strategy-canvas/*.tsx`) and the V1.72 outline-canvas B5 split (orchestrator 228 lines, sub-modules in `outline-canvas/*.tsx` sibling files). Per the plan scope, B5 was only required for outline-canvas.tsx; world-kb-canvas.tsx is not bound by an explicit line cap. **Fix (non-blocking)**: extract the four inline sub-components into focused sibling modules mirroring the B5 pattern (e.g. `world-kb/world-kb-header.tsx`, `world-kb/world-kb-inspector-panel.tsx`, `world-kb/world-kb-conflict-hosts.tsx`) to restore visual symmetry with V1.71/V1.72 and bring each file under ≤250 lines.

- **F-S3**: `idempotency_key: Option<String>` is declared on `WorldKbPatchEntityRequest` and `WorldKbPromoteCandidateRequest` (schema `idempotency-key`) but is unused by the handler layer (the schema description says "reserved; V1.73 accepts and ignores"). This is a small forward-compatibility hook and does not break anything, but it ships as dead wire until activated. **Fix (non-blocking)**: either remove the field for V1.73 and add it back when an idempotency cache lands, or document the activation trigger in the schema description (e.g. "reserved for V1.75 idempotency cache; accepted and ignored today").

## Source Trace

- Finding ID: F-S1
  - Source Type: manual-reasoning (cross-file schema vs codegen comparison)
  - Source Reference: `schemas/local-api/canvas/world-kb/world-kb-patch-entity-request.schema.json:5` (`"title": "Nexus PatchWorldKbEntityRequest"`) vs `crates/nexus-contracts/src/generated/local_api/canvas/world_kb/world_kb_patch_entity_request.rs:14` (`pub struct WorldKbPatchEntityRequest`) and `packages/nexus-contracts/src/generated/local-api/canvas/world-kb/WorldKbPatchEntityRequest.ts:12` (`export interface WorldKbPatchEntityRequest`). Compare V1.71 baseline: `schemas/local-api/canvas/strategy/strategy-patch-state-request.schema.json:5` (`"title": "Nexus StrategyPatchStateRequest"`) ↔ `StrategyPatchStateRequest` struct (consistent).
  - Confidence: High

- Finding ID: F-S2
  - Source Type: static-analysis (line count)
  - Source Reference: `wc -l apps/web/src/components/canvas/world-kb/world-kb-canvas.tsx` = 546. Compare V1.71 `apps/web/src/components/canvas/strategy-canvas.tsx` = 187 lines + sibling modules (strategy-canvas/canvas-layout.tsx 80, inspector-panel.tsx 152, state-machine.tsx 161, hooks/use-strategy-canvas.ts 169). Compare V1.73 outline-canvas.tsx = 228 lines + 6 sibling modules in outline-canvas/.
  - Confidence: High

- Finding ID: F-S3
  - Source Type: manual-reasoning (handler code vs schema)
  - Source Reference: `schemas/local-api/canvas/world-kb/world-kb-patch-entity-request.schema.json:13` (`idempotency_key`) is not read in `crates/nexus-daemon-runtime/src/api/handlers/world_kb.rs::patch_entity` or `promote_candidate` (lines 209-428 of the handler). Schema description at line 13: `"Optional idempotency key (reserved; V1.73 accepts and ignores)."`
  - Confidence: High

## Architecture & Maintainability Highlights (non-finding observations)

These are recorded for the consolidated record but are **not** findings — they document strengths that may help downstream QC seats.

- **Patch-route convention adherence**: The four World KB routes (`POST .../kb/patch-entity`, `POST .../kb/promote-candidate`, `GET .../kb/graph`, `GET .../kb/candidates`) follow the V1.71 strategy-routes / V1.72 canvas-outline-routes pattern (nested under the world-scoped `/v1/local/worlds/{world_id}/kb/*` prefix, registered in `api/mod.rs::world_kb_routes()` and merged into the protected router). ✓
- **Per-row OCC mechanism**: `cas_update_key_block_fields` in `nexus-local-db/src/kb_store.rs` (lines 758-823) mirrors the V1.51 `kb_extract_jobs.version` CAS pattern. The new function is added to the concrete `SqliteKbStore` rather than the abstract `KbStore` trait (line 338) — this is the right call: the trait stays general-purpose, and the entity-specific OCC lives at the storage layer where it belongs. The abstract KbStore trait is untouched. NULL-normalization of `revision` (architect Phase 2b lock) is enforced via `COALESCE(revision, 0)` in the WHERE clause. ✓
- **Atomic CAS persistence**: The adopt path (`promote_adopt` in handlers/world_kb.rs:506-585) wraps `insert_key_block_in_tx` + `mark_confirmed_in_tx_with_cas` in a single transaction with explicit rollback on the orphan-KeyBlock race (lines 551-562). The reject/merge paths use the same atomic-CAS pattern. The early return paths correctly release the implicit transaction (via `tx.rollback()` or commit). ✓
- **Error envelope single-source rule**: Two new variants `WorldKbConflict` (409) and `WorldKbValidationFailed` (422) are added to `NexusApiError` (`api/errors.rs:200-215`), each with a `world_kb_*` constructor. The IntoResponse trait emits the canonical `{success:false, error:{code,message,details,request_id}}` envelope. No ad-hoc JSON error bodies in the new handlers. The AGENTS.md error-envelope single-source rule is respected. ✓
- **ConflictModalBase reuse**: Both World KB conflict modals (`patch_entity` variant + `promote_candidate` variant) compose `ConflictModalBase` with the **additive** `descriptionSuffix` prop (conflict-modal-base.tsx:34-40, comments at 35-39). The base class is unchanged for V1.71/V1.72 callers; Strategy/Outline descriptions render exactly as before. KB-flavored copy matches the compass §1.1 A6 verbatim for both variants. ✓
- **B5 outline-canvas split**: 825-line monolith → 228-line orchestrator (`outline-canvas.tsx`) + 6 sibling modules (`outline-canvas/canvas-layout.tsx` 35 lines, `conflict-modal.tsx` 45, `graph-projection.ts` 127, `inspectors/chapter-inspector.tsx` 238, `inspectors/event-inspector.tsx` 143, `inspectors/structure-inspector.tsx` 174). Largest single file is `chapter-inspector.tsx` at 238 lines, just under the 250-line cap. Public `OutlineCanvas` export is unchanged. ✓
- **Wire-contract additive-only**: 0.8.0 → 0.9.0 adds 13 new schema→Rust→TS types. No existing types renamed or removed. `crates/nexus-contracts/src/generated/mod.rs` SCHEMA_VERSIONS list appends 13 entries (`WorldKbCandidateProjection` … `WorldKbValidationError`) without disturbing existing entries. Drift-detection test (`tests/schema_drift_detection.rs`) registers all 13 with `Strict` mode and passes. ✓
- **File-disjoint verification**: P0 owns new `world-kb/**` (13 files in apps/web/src/components/canvas/world-kb/, 1 page in pages/, 13 schemas, 13 Rust contract files, 1 handler file, 1 data-hook file, 1 browser-client update); P1 owns new `outline-canvas/**` (7 files in apps/web/src/components/canvas/outline-canvas/, plus outline.rs hardening). Zero overlap between the two scopes. ✓
- **DESIGN.md token surface**: 17 `canvas-worldkb-*` tokens added (concrete light values in DESIGN.md, dark values in DESIGN.dark.md, Tailwind utility bindings in `tailwind.config.ts` and `index.css`). All token names match the architect Phase 2b lock verbatim. ✓
- **No new migration**: `kb_key_blocks.revision INTEGER` column already existed (migration `20260525_kb_key_blocks.sql`); the V1.73 OCC work is purely application-layer + codegen. Zero schema migrations in the diff. ✓
- **CLI thin-shell refactor**: `apps/nexus42/src/commands/creator/world/kb.rs` (1624 lines) is **deliberately deferred** to a future batch per the plan; the Local API routes are fully functional and the CLI's direct-DB path continues to work without regression. Noted as documented tech debt, not a regression. ✓
- **CI hardening (B7)**: `desktop-release.yml` adds `Swatinem/rust-cache@v2` to the Rust build legs so release rebuilds reuse compilation artifacts (matches desktop-build.yml cache wiring). ✓

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 3 |

**Verdict**: Approve

The V1.73 implementation is architecturally sound and follows the V1.71/V1.72 patch-route conventions. Per-row OCC on `kb_key_blocks.revision` is well-isolated in the storage layer with NULL-normalization per the architect Phase 2b lock; atomic CAS persistence covers the adopt/reject/merge race correctly. The error envelope single-source rule, the CanvasShell adapter pattern, and the additive ConflictModalBase extension are all respected. The B5 outline-canvas split is clean (228-line orchestrator + 6 focused sibling modules, each ≤250 lines). 9 backend integration tests, 15 outline regression tests, 4 schema-drift tests, 25 World KB vitest tests, and 10 outline-canvas vitest tests all pass. The three Suggestions are non-blocking improvements (naming consistency between schema titles and codegen output, world-kb-canvas.tsx sub-component extraction to mirror the V1.71/V1.72 split pattern, and the unused `idempotency_key` field) that can be addressed in a follow-up plan if desired.