---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-06-29-v1.74-world-kb-relationships"
verdict: "Request Changes"
generated_at: "2026-06-29"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Runtime Model: xai/grok-build-0.1
- Review Perspective: Architecture coherence and maintainability risk — module boundaries, contract/schema coherence, `kb_relationships` fit with `kb_store`, A10 split quality (≤250-line cap, public re-exports preserved), hybrid taxonomy + directed/symmetric model coherence, B1/B5/B11 surgical-change verification, generated contract vs spec wording drift.
- Report Timestamp: 2026-06-29

## Scope
- plan_id: 2026-06-29-v1.74-world-kb-relationships (lead; consolidated review covers P0 world-kb-relationships + P1 hygiene-slate-clear + integration codegen)
- Review range / Diff basis: 0fed23f8..38cacda2 (origin/main merge-base..iteration/v1.74 HEAD; 26 commits). Equivalent to `git diff 0fed23f8..38cacda2`.
- Working branch (verified): iteration/v1.74
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 27 (P0 architecture surface: kb_relationships.rs, world_kb.rs handler, world-kb-canvas split modules, schemas, generated contracts; P1 hygiene surface: B1 strategy.rs, B2 adapter-contract.test.ts, B3/B4 schemas, B5 chapter-page.tsx, B8 codegen README, B11 inspector save-trigger)
- Commit range (if not identical to Review range line, explain): 0fed23f8..38cacda2 (identical)
- Tools run:
  - `git branch --show-current` + `git rev-parse HEAD` (branch/HEAD verification; HEAD = `38cacda270607aa58798c8f640b05531583754b3` on `iteration/v1.74`)
  - `git diff 0fed23f8..38cacda2 --stat` (overall diff map; 112 files, +6628 / -1019)
  - `git log --oneline 0fed23f8..38cacda2` (26 commits)
  - `cargo clippy -p nexus-daemon-runtime -p nexus-local-db -- -D warnings` (clean)
  - `pnpm --filter web typecheck` (clean)
  - `cargo test -p nexus-daemon-runtime --test world_kb_relationships` (11/11 pass)
  - `cargo test -p nexus-local-db --lib kb_relationships` (5/5 pass)
  - `pnpm --filter web test` (223/223 pass)
  - `wc -l` on every `.tsx`/`.ts`/`.rs` file in `apps/web/src/components/canvas/world-kb/` and `crates/nexus-local-db/src/kb_relationships.rs` to verify the A10 ≤250-line cap

## Findings

### 🔴 Critical

- *(none)*

### 🟡 Warning

- **F-001 — Three `world-kb` split modules exceed the V1.73 ≤250-line cap cited in the compass acceptance criterion (`compass §1.1 A10` and `§7 DoD #6`).**
  - `apps/web/src/components/canvas/world-kb/world-kb-canvas.tsx` — **268 lines** (18 over cap). The orchestrator facade split per `commit 56db7671` (R-V173QC1-S2) landed at 247 lines, then A6 relationship wiring (`commit da340517`) + A6 parity completion (`commit 6e7a323c`) + V1.74 β completion (`commit 8f45386b`) grew the facade back over the cap. The growth comes from the three `*ConflictHost` render blocks and the relationship delete handler; each is a small extraction candidate (e.g. extract `EntityConflictHost`/`PromoteConflictHost`/`RelationshipConflictHost` render props into a sibling host-rendering helper, or move the relationship-delete handler into the state hook).
  - `apps/web/src/components/canvas/world-kb/relationship-inspector.tsx` — **254 lines** (4 over cap). The 8 inspector `Field` blocks + submit/remove handlers + custom-label conditional render could fit under the cap with one small extraction (e.g. move the symmetric+confidence+anchor grid into a `RelationshipFieldGrid` component).
  - `apps/web/src/components/canvas/world-kb/world-kb-relationship-table.tsx` — **257 lines** (7 over cap). The `formatUpdated` and `SortHeader` helpers at the bottom of the file are natural extraction candidates (move `SortHeader` into `world-kb-canvas-utils.ts` next to `formatRelative`; move `formatUpdated` next to it so the relative-time formatters share a home).
  - Evidence: `wc -l apps/web/src/components/canvas/world-kb/*.tsx apps/web/src/components/canvas/world-kb/*.ts`; commit history `56db7671` (initial 247-line split) → `8f45386b` (current 268).
  - Why this matters: the cap is the architectural invariant for the file-disjoint parallel worktree pattern that P0 relied on for `feature/v1.74-world-kb-relationships` and P1 for `feature/v1.74-hygiene-slate-clear` (`compass §4`). Re-growing the orchestrator over the cap erodes the invariant and gives a future split-writer a single-line-overlap risk surface.
  - **Fix**: split each over-cap file by extracting the obvious helper components / form-grid sections / sort-header into neighboring modules. After the split, all `world-kb/**/*.{ts,tsx}` modules must be ≤ 250 lines.

### 🟢 Suggestion

- **F-002 — `crates/nexus-local-db/src/kb_relationships.rs` duplicates the inline "rows-affected mismatch → re-read revision → emit `VersionMismatch`" pattern that already exists in two other places (`kb_store::cas_update_key_block_fields` and `kb_extract_job::mark_confirmed_in_tx_with_cas`) and that `crates/nexus-local-db/src/cas.rs::cas_check` (V1.51 T-B P1) provides as a reusable helper.**
  - Evidence: `kb_relationships.rs:172-183` (update), `kb_relationships.rs:211-222` (delete), `kb_store.rs:811-823`, `kb_extract_job.rs` CAS path, `cas.rs:54-80`.
  - Why this matters: three independent re-read paths drift independently over time. `cas_check` already re-reads the column on mismatch. The signatures differ (kb_relationships uses `&mut Transaction`, others use `&SqlitePool`), so a small `cas::cas_check_in_tx` variant would unify all three. Keeping the duplication until then is acceptable but should be tracked.
  - **Fix**: extract a `cas::cas_check_in_tx` that accepts the transaction + table name + PK column + id value + expected_version; have `update_relationship_in_tx`, `delete_relationship_in_tx`, and `cas_update_key_block_fields` call it instead of re-implementing the re-read query.

- **F-003 — `apps/web/src/components/canvas/world-kb/relationship-inspector-logic.ts:89-102` hand-writes a `RelationshipPatchRequest` interface that structurally mirrors the generated `WorldKbPatchRelationshipRequest` from `@42ch/nexus-contracts`. This violates `apps/web/AGENTS.md` rule: "**Never** hand-write a second source of wire DTO types in this package — all wire types come from the generated contracts (web-ui.md §12.6)."**
  - The handwritten type is only used as the return-type annotation of `buildRelationshipPatchRequest` and `buildRelationshipRemoveRequest`; the structural shape is identical, so callers work. But the rule violation is real: a future schema bump that adds a field to `WorldKbPatchRelationshipRequest` would not flow through these builders, and a rename in the generated type would silently break the typed return.
  - **Fix**: `import type { WorldKbPatchRelationshipRequest as RelationshipPatchRequest } from '@42ch/nexus-contracts';` and use the generated alias everywhere. Drop the hand-written interface.

- **F-004 — Codegen policy change in `tooling/codegen/src/rust-generator.ts` (commit `269fb31a`) drops `Eq` from ALL generated structs, not just those that contain `f64` fields. The change was correctly required by `WorldKbRelationshipProjection.confidence: Option<f64>` (f64 is not `Eq`), but the codegen now strips `Eq` from structs that DO have only `Eq`-compatible fields.**
  - Evidence: `tooling/codegen/src/rust-generator.ts:550-555` (struct derive) — `Eq` removed unconditionally. `git diff 269fb31a --stat` shows the change applied to 21 generated files including `Creator`, `VersionRef`, `User`, `World`, `OutlineFrontmatter`, etc. None of these struct definitions were modified to add f64 fields — they lost `Eq` only because the codegen policy now strips it.
  - Current consumer impact: zero (no consumer uses these as `HashMap`/`HashSet` keys today; verified via `rg`). Future surface: a caller that adds `HashMap<WorldKbEntityProjection, _>` will hit a confusing compile error.
  - **Fix**: narrow the derive in the generator — emit `#[derive(..., Eq)]` only when the schema has no `f64`/`f32` fields; emit only `PartialEq` otherwise. Re-run `pnpm run codegen` to restore `Eq` on the integer/string-only structs.

- **F-005 — `apps/web/src/components/canvas/world-kb/world-kb-relationship-conflict-modal.tsx:67-78` renders the relationship-conflict body copy by splitting the description into `description="…(version"` + `descriptionSuffix={") while you were editing it."}`, which the base modal (`conflict-modal-base.tsx:159-164`) joins with `currentRevision` between them. The copy diverges from the compass-specified text in `compass §1.1 A6 conflict modal UX`.**
  - Compass text: `"Nexus updated the relationship **{source_entity_name} → {target_entity_name}** to version **{current_version}** while you were editing its **{field}**. Your change is still in the inspector."`
  - Implementation renders: `"Nexus updated the relationship between **A** and **B** (version 5) while you were editing it. Your change is still in the inspector."`
  - Differences: arrow (`→` vs "between"), missing field placeholder (`{field}`), and "while you were editing it" instead of "while you were editing its X". Functionally correct but copy-drift from the spec.
  - **Fix**: either align the copy to the compass wording (add the `{field}` interpolation even though the relationship has one effective "field" concept), or amend the compass wording at P-last to match the implementation. Either way, close the divergence explicitly so future reviewers don't see drift.

- **F-006 — `apps/web/src/components/canvas/world-kb/graph-projection.ts:11-13` module docstring still describes V1.73 read-only relationship behavior:**
  > "Relationship edges (`world-kb-relationship-edge`) are derived read-only from source anchors in V1.73; `world_kb.patch_relationship` + a `kb_relationships` table are deferred to V1.74."
  - V1.74 has shipped the `kb_relationships` table and the relationship-edge derivation now lives in `relationship-projection.ts`. The docstring is stale and misleads future readers.
  - **Fix**: update the docstring to point to V1.74 and `relationship-projection.ts` as the source of relationship edges; keep the rest of the V1.73 read-only source-anchor provenance language intact.

- **F-007 — `crates/nexus-daemon-runtime/src/api/handlers/world_kb.rs:1437-1440` `project_relationship` falls back to `WorldKbRelationshipKind::Custom` when parsing a stored `relation_type` string fails.**
  - Edge case: if a future client writes a core enum value that's been deprecated or renamed in a schema migration, the read projection silently coerces it to `Custom` with no `custom_label` (the original `custom_label` column is `NULL` for core-enum rows). Round-tripping the row through `validate_relationship_input` would then fail with "custom relation_type requires custom_label" — surfacing an opaque error.
  - Today this is unreachable because the wire `relation_type` is validated at write time and the enum values are additive-only (per repo SemVer policy for `pre-1.0` local-only). Worth noting for defense-in-depth.
  - **Fix**: on unknown `relation_type` from storage, log a `warn!` and either preserve the raw string in a `metadata.unknown_relation_type` field or return an explicit error response. Document the choice.

- **F-008 — Compass §2 (`§2.3` references the spec `world-kb-runtime-architecture.md` for amendment; `§6.5` does the same in the risk notes) points to a file that does NOT exist under `.mstar/knowledge/specs/`. Verified via `ls .mstar/knowledge/specs/`: no file matching `world-kb*`.**
  - The compass also references lift-ready wording for `entity-scope-model.md §5.6` (file exists) and `local-api-surface-conventions.md §7.6` (file exists).
  - P-last must either (a) create `world-kb-runtime-architecture.md` with the locked wording, or (b) amend an existing spec (e.g. `daemon-runtime.md` or `local-db-schema.md`) and update the compass to point there. Closing the reference in the compass is mandatory before the PR lands.
  - **Fix**: at P-last, decide the target file, apply the amendment, and either create `world-kb-runtime-architecture.md` or update compass §2 to point to the chosen existing spec.

## Source Trace

- Finding ID: F-001
  - Source Type: doc-rule (compass §1.1 A10 acceptance criterion + §7 DoD #6)
  - Source Reference: `wc -l apps/web/src/components/canvas/world-kb/{world-kb-canvas,relationship-inspector,world-kb-relationship-table}.tsx`; `git log --oneline -- apps/web/src/components/canvas/world-kb/world-kb-canvas.tsx` (56db7671 → 8f45386b → 38cacda2)
  - Confidence: High

- Finding ID: F-002
  - Source Type: manual-reasoning + `rg "VersionMismatch" crates/nexus-local-db/src/`
  - Source Reference: `crates/nexus-local-db/src/{kb_relationships,kb_store,kb_extract_job,cas}.rs`
  - Confidence: High

- Finding ID: F-003
  - Source Type: doc-rule (`apps/web/AGENTS.md` "Consumes `@42ch/nexus-contracts` via `workspace:*`. **Never** hand-write a second source of wire DTO types in this package…")
  - Source Reference: `apps/web/src/components/canvas/world-kb/relationship-inspector-logic.ts:89-102`
  - Confidence: High

- Finding ID: F-004
  - Source Type: git-diff + codegen policy inspection
  - Source Reference: `git show 269fb31a -- tooling/codegen/src/rust-generator.ts`; `git show 269fb31a --stat` (21 files changed)
  - Confidence: High

- Finding ID: F-005
  - Source Type: doc-rule (compass §1.1 A6 conflict modal UX)
  - Source Reference: `apps/web/src/components/canvas/world-kb/world-kb-relationship-conflict-modal.tsx:67-78` vs compass §1.1 A6 body copy
  - Confidence: High

- Finding ID: F-006
  - Source Type: doc-rule (stale docstring)
  - Source Reference: `apps/web/src/components/canvas/world-kb/graph-projection.ts:11-13`
  - Confidence: High

- Finding ID: F-007
  - Source Type: manual-reasoning (defense-in-depth)
  - Source Reference: `crates/nexus-daemon-runtime/src/api/handlers/world_kb.rs:1437-1440`
  - Confidence: Medium

- Finding ID: F-008
  - Source Type: doc-rule (compass-vs-reality mismatch)
  - Source Reference: `ls .mstar/knowledge/specs/` (no `world-kb*` file); compass §2 + §6.5 references
  - Confidence: High

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 1 |
| 🟢 Suggestion | 7 |

**Verdict**: Request Changes

## Checklist Execution (architecture/maintainability lens)

- [x] Module boundary quality — `kb_relationships.rs` (561 lines, new module) cleanly separated from `kb_store.rs` (1411 lines); public re-exports added in `crates/nexus-local-db/src/lib.rs:94-99`. ✅
- [x] Public re-exports preserved — `world-kb-canvas.tsx` still re-exports `EntityField` + `patchFromForm` for back-compat with `world-kb-canvas.test.tsx:18` and `world-kb-alt-view.test.tsx:13`. ✅
- [x] Symmetric read projection is single-row + derived-reverse — `project_relationships_for_world` (`world_kb.rs:884-904`) loops stored rows, pushes one `stored` projection, and if `symmetric != 0` swaps source/target on a clone and pushes a `symmetric_reverse` projection sharing the same `relationship_id`. No double-write path. ✅
- [x] Hybrid taxonomy coherence — `WorldKbRelationshipKind` enum is the locked core (11 values + `custom` sentinel); `validate_relationship_input` (`world_kb.rs:1256-1278`) rejects `custom` without `custom_label`, accepts core values with `custom_label` ignored; cross-checked against `world-kb-relationship-input.schema.json` (`required: symmetric`, `relation_type` refs the kind enum, `custom_label` minLength 1). ✅
- [x] New DTOs consistent with V1.73 entity/candidate DTOs — `WorldKbRelationshipProjection` uses wire `version` field (not `revision`) matching V1.73 (`world_kb_relationship_projection.rs:28`), carries `projection_direction` discriminator, and follows the V1.73 filename-derived `WorldKb...` naming (per B3 reconciliation). ✅
- [x] B3 reconciliation — V1.73 schema `title` strings now read `Nexus WorldKb...` (entity-prefix) matching generated symbols. `git diff 0fed23f8..38cacda2 -- schemas/local-api/canvas/world-kb/` shows 4 title changes (`patch-entity-request`, `patch-entity-response`, `promote-candidate-request`, `promote-candidate-response`). ✅
- [x] B4 `idempotency_key` removed — `idempotency_key` no longer appears in any V1.74-generated DTO or any patch-handler path. ✅
- [x] B5 surgical — `chapter-page.tsx:70` one-character flip `?? true` → `?? false`; regression test added in `chapter-page.test.tsx`. Behavior-preserving (absent = non-editable now matches the absent-field semantics). ✅
- [x] B6 virtualization — `world-kb-entity-table.tsx:9` (`useLayoutEffect` + scroll-windowing with `ROW_HEIGHT = 44`); `world-kb-relationship-table.tsx` does NOT use react-window (the entity table does). The relationship table caps height at `calc(100vh - 420px)` (`world-kb-relationship-table.tsx:119`) which is fine for relationship counts in single-work Worlds. ✅
- [x] B11 surgical — `useRef(handleSave)` pattern in `state-inspector.tsx:118-122` (and matching in `edge-inspector.tsx` and `prompt-inspector.tsx`) avoids effect-deps churn; effect now depends only on `saveTrigger`. ✅
- [x] Clippy clean — `cargo clippy -p nexus-daemon-runtime -p nexus-local-db -- -D warnings` exits 0. ✅
- [x] Typecheck clean — `pnpm --filter web typecheck` exits 0. ✅
- [x] Test coverage — 11 `world_kb_relationships.rs` integration tests + 5 `kb_relationships` unit tests + 9 `relationship-inspector.test.tsx` + 7 `world-kb-relationship-table.test.tsx` + existing entity/promotion/alt-view suites all pass. ✅
- [ ] **≤250-line cap on every `world-kb/` module** — 3 modules exceed the cap (F-001). ❌
- [x] Conflict-modal UX reuse — `world-kb-relationship-conflict-modal.tsx` consumes `ConflictModalBase` (shared shell, focus trap, ARIA live region, action tray); only copy is per-variant. ✅
- [x] Anchor validation — `require_valid_source_anchors` (`world_kb.rs:1343-1399`) strips `sa_` prefix, validates the underlying `kb_key_blocks.key_block_id` exists in the world, and rejects anchors whose entity has no `source_work_id`. ✅
- [x] Self-loop guard — three-layer defense (canvas `handleConnect` skips same-id; inspector `targetEntities` filter excludes source; daemon `validate_relationship_input` returns 422). ✅
- [x] OCC semantics — `update_relationship_in_tx` / `delete_relationship_in_tx` use `WHERE relationship_id = ? AND revision = ?`; `rows_affected() == 1` returns new version; on miss, re-read current revision and return `LocalDbError::VersionMismatch` (handled by `map_relationship_cas_err` to 409 `WorldKbConflictError`). ✅
- [x] Codegen determinism — `.sqlx/` JSON cache regenerated; `pnpm run codegen` produces identical output to committed `*/generated/` (CI `verify-codegen` gates merge). ✅
- [x] Generated contract re-exports — `packages/nexus-contracts/src/generated/index.ts` and `crates/nexus-contracts/src/generated/local_api/canvas/world_kb/mod.rs` both export the new `WorldKbRelationship*` symbols. ✅

## Evidence Notes

- The compass §1.1 A10 acceptance "each split file ≤ 250 lines" was met at commit `56db7671` (247 lines), but later A6 work pushed the orchestrator and two new modules back over the cap. This is a known growth pattern in this codebase (strategy-canvas grew past 570 → re-split to 190 lines in V1.72 P1, then re-grew); the fix is the same iterative split, not a regression.
- The kb_relationships module's CAS duplication is a real but low-impact concern: it does not change semantics, only the implementation. `cas_check` in `cas.rs` would unify three callers but requires either an `&mut Transaction` variant or a non-pool variant — small refactor with no behavior change.
- The relationship conflict modal's copy divergence (F-005) is the kind of small drift that compounds across reviews; closing it now at P-last (the `compass §2` amendment slot for `local-api-surface-conventions.md`) keeps the contract-doc ↔ UI-copy contract honest.

## Plan Update

- **PM action**: Convert F-001 to a tracking residual `R-V174QC1-A1` (severity low, scope `apps/web/src/components/canvas/world-kb/`, plan_id `2026-06-29-v1.74-world-kb-relationships`, owner `@frontend-dev`) before the fix-wave dispatch. F-002–F-008 can be tracked as lower-priority residuals or accepted as polish — none block the iteration closure since the slate-clear goal is the 12 V1.74 residuals, not new findings.
- **P-last spec amendments**: F-008 (missing `world-kb-runtime-architecture.md`) must be resolved at P-last by either creating the file with the locked wording from compass §2 or amending an existing spec and updating the compass reference. F-005 (copy divergence) should be settled at P-last alongside the compass amendment.

## Handoff

- This is reviewer #1 of three (qc1, qc2, qc3) — qc2 has already filed `Request Approve` (security/correctness lens, 0 Critical + 0 Warning + 3 Suggestions). My `Request Changes` is solely on F-001 (architecture/maintainability lens: split cap not fully held).
- PM consolidates after qc3 lands and dispatches a fix wave for F-001 only (Suggestions are non-blocking per `compass §7` slate-clear discipline). After the fix wave, qc1 targeted re-review (this report file) verifies the three files are ≤250 lines and updates the verdict to `Approve`.
- The 12 V1.74 residuals remain `lifecycle: open` in `.mstar/status.json` (verified via `grep -B1 -A2` for each `R-V173QC1-S*` / `R-V171*` / `R-V172*` id). PM owns the closure writes at P-last; this is consistent with the wave-1 review timing.

---

*No revalidation yet — initial tri-review report.*