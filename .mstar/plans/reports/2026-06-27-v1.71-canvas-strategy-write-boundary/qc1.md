---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: 2026-06-27-v1.71-canvas-strategy-write-boundary
verdict: Request Changes
generated_at: 2026-06-28T05:12:00Z
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Runtime Model: k2p7
- Review Perspective: Architecture coherence and maintainability risk (Reviewer #1)
- Report Timestamp: 2026-06-28T05:12:00Z

## Scope
- plan_id: 2026-06-27-v1.71-canvas-strategy-write-boundary
- Review range / Diff basis: `git diff 39493026..HEAD -- schemas/local-api/canvas/ crates/nexus-contracts/src/generated/local_api/canvas/ crates/nexus-contracts/src/generated/local_api/mod.rs crates/nexus-contracts/src/generated/mod.rs crates/nexus-contracts/tests/schema_drift_detection.rs crates/nexus-daemon-runtime/src/api/errors.rs crates/nexus-daemon-runtime/src/api/handlers/mod.rs crates/nexus-daemon-runtime/src/api/handlers/strategy.rs crates/nexus-daemon-runtime/src/api/mod.rs packages/nexus-contracts/package.json packages/nexus-contracts/src/generated/index.ts packages/nexus-contracts/src/generated/local-api/canvas/ apps/web/DESIGN.md apps/web/DESIGN.dark.md apps/web/src/index.css apps/web/src/components/canvas/strategy-canvas.tsx apps/web/src/lib/canvas/preset-yaml.ts apps/web/src/lib/canvas/use-strategy-data.ts apps/web/src/lib/nexus/browser-client.test.ts apps/web/src/lib/nexus/browser-client.ts apps/web/src/lib/nexus/types.ts apps/web/tailwind.config.ts`
- Working branch (verified): iteration/v1.71
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 37 changed paths + 5 supporting spec/harness docs
- Commit range: 39493026..HEAD
- Tools run:
  - `cargo +nightly-2026-06-26 fmt --all --check` (passed)
  - `cargo clippy --all -- -D warnings` (passed; pre-existing warnings only in unrelated integration tests)
  - `cargo test -p nexus-daemon-runtime` (336 unit + integration tests passed)
  - `cargo test -p nexus-contracts --test schema_drift_detection` (passed)
  - `pnpm --filter @42ch/nexus-contracts run build` (required before web typecheck because `dist/` is not committed)
  - `pnpm --filter web typecheck` (passed after contracts build)
  - `pnpm --filter web test` (139 tests passed)

## Findings

### 🔴 Critical

- **R-V171P0-QC1-001 — Conflict modal does not implement the specified UX contract.**
  The current `ConflictModal` in `apps/web/src/components/canvas/strategy-canvas.tsx` is a simplified two-button dialog ("Strategy changed elsewhere" / "Refetch graph" / "Keep editing"). It does not implement the acceptance-mandated headline "This node changed while you were editing.", the field-level **What changed** / **What you were about to do** summaries, the three actions **Use current**, **Reapply my edit**, and **Review side-by-side** with safe-merge disabled-state logic, nor the A8 accessibility requirements (ARIA live region, focus trap, return focus, reduced-motion handling).
  -> Re-implement `ConflictModal` to match `canvas-strategy-surface.md` §3.5 and the compass §6.3 conflict policy. Extract it to its own module and add component tests.

- **R-V171P0-QC1-002 — Missing integration/regression tests for the Strategy patch routes.**
  Plan acceptance A9 requires unit, integration, e2e, and regression tests (including a concurrent daemon-write 409 scenario). The only new Rust tests are inline unit tests in `handlers/strategy.rs`. No `crates/nexus-daemon-runtime/tests/strategy_patch.rs` integration file exists, and there is no regression test proving that two concurrent daemon-side writers cannot silently last-write-win.
  -> Add full-stack integration tests for all three `POST` routes and a regression test that exercises the concurrent-write path.

### 🟡 Warning

- **R-V171P0-QC1-003 — Prompt-template patch writes the file before domain validation.**
  In `crates/nexus-daemon-runtime/src/api/handlers/strategy.rs::patch_prompt_template`, `std::fs::write(&canonical_template, body)` executes before `validate_preset_yaml`. If validation fails with 422, the bundle is left with a mutated prompt-template file but an unchanged `preset.yaml`/`revision:`, producing a partial/inconsistent bundle state.
  -> Validate the manifest against the updated template content first (in-memory or via a temp file promoted only after validation succeeds).

- **R-V171P0-QC1-004 — `handleSave` chains non-atomic state/transition/prompt mutations.**
  `apps/web/src/components/canvas/strategy-canvas.tsx::handleSave` can issue up to three sequential `mutateAsync` calls (`patchState`, `patchTransition`, `patchPrompt`). If an early call succeeds and a later one fails, the graph is partially persisted with no rollback or unified transaction boundary.
  -> Split saves per inspector section (matching the separate route semantics) or add compensating logic and partial-failure UI. Document the chosen behavior in the component.

- **R-V171P0-QC1-005 — No serialization around the read-modify-write of `preset.yaml`.**
  Two concurrent requests with the same `base_revision` can both pass the precondition, both compute `new_revision = current + 1`, and both rename their temp files. The last rename wins, and both responses claim the same revision. This undermines the graphRevision guarantee for multi-writer scenarios.
  -> Add an advisory file lock or a crate-level mutex around `load_user_preset_yaml` → mutation → `write_preset_yaml` so only one patch commits at a time.

- **R-V171P0-QC1-006 — `strategy-canvas.tsx` mixes multiple concerns in one file.**
  The file now contains graph sync, live overlay, edit-form state, validation panel, conflict modal, artifacts list, and helper components (~570 lines). This reduces testability and makes future UI iteration harder.
  -> Extract `InspectorOverlay`, `ConflictModal`, `ValidationPanel`, and `ArtifactsList` into focused modules under `apps/web/src/components/canvas/`.

### 🟢 Suggestion

- **R-V171P0-QC1-007 — `canvas-write-stale-bg` uses a raw rgba literal.**
  Both `apps/web/DESIGN.md` and `apps/web/DESIGN.dark.md` hardcode `rgba(183,110,0,...)`. Prefer a reference to `{colors.amber-700}` or a derived alpha token so the value stays tied to the color scale and dark/light values remain consistent.
  -> Replace the raw rgba with a reference-based token or add an explicit amber-alpha component token.

- **R-V171P0-QC1-008 — Add a UI e2e/component test for edit→save→refetch.**
  Web coverage is currently limited to `BrowserClient` msw tests in `browser-client.test.ts`. A component-level test for `StrategyCanvas` (or the inspector) that submits a patch and verifies query invalidation/refetch would close part of the A9 e2e gap.
  -> Add a focused test for the inspector save flow and conflict-modal trigger.

- **R-V171P0-QC1-009 — Route registration and generated-artifact discipline is otherwise solid.**
  `crates/nexus-daemon-runtime/src/api/mod.rs` registers the three `POST` routes under protected routes, `src/api/errors.rs` adds canonical `StrategyConflict`/`StrategyValidationFailed` variants with correct status codes, and `crates/nexus-contracts/tests/schema_drift_detection.rs` registers all five new schemas in `CheckMode::Strict`. The `@42ch/nexus-contracts` version was bumped to `0.7.0` as required.

- **R-V171P0-QC1-010 — Clarify the `apps/web/src/api-types/` codegen target.**
  The plan/compass mention generated TypeScript output in `apps/web/src/api-types/canvas/`, but the actual generated TypeScript lives in `packages/nexus-contracts/src/generated/` and is consumed via the workspace package. The web app imports correctly from `@42ch/nexus-contracts`. Confirm this is the intended architecture and update the plan/spec if the `apps/web/src/api-types/` path is no longer used.

## Source Trace

| Finding ID | Source Type | Source Reference | Confidence |
|---|---|---|---|
| R-V171P0-QC1-001 | manual-reasoning + doc-rule | `apps/web/src/components/canvas/strategy-canvas.tsx` lines 478–522; `canvas-strategy-surface.md` §3.5; compass §6.3 | High |
| R-V171P0-QC1-002 | manual-reasoning + static-analysis | `crates/nexus-daemon-runtime/src/api/handlers/strategy.rs` lines 783–987; `glob crates/nexus-daemon-runtime/tests/strategy_patch*` returned no files | High |
| R-V171P0-QC1-003 | manual-reasoning | `crates/nexus-daemon-runtime/src/api/handlers/strategy.rs` lines 755–767 | High |
| R-V171P0-QC1-004 | manual-reasoning | `apps/web/src/components/canvas/strategy-canvas.tsx` lines 153–211 | High |
| R-V171P0-QC1-005 | manual-reasoning | `crates/nexus-daemon-runtime/src/api/handlers/strategy.rs` lines 390–399, 454–459, 601–608, 658–663, 769–774, 327–369 | Medium |
| R-V171P0-QC1-006 | static-analysis | `apps/web/src/components/canvas/strategy-canvas.tsx` total 568 lines | High |
| R-V171P0-QC1-007 | static-analysis | `apps/web/DESIGN.md` lines 199–202; `apps/web/DESIGN.dark.md` lines 199–202 | High |
| R-V171P0-QC1-008 | static-analysis | `apps/web/src/lib/nexus/browser-client.test.ts` lines 278–371 | High |
| R-V171P0-QC1-009 | static-analysis | `crates/nexus-daemon-runtime/src/api/mod.rs` lines 230–249; `crates/nexus-daemon-runtime/src/api/errors.rs` lines 155–173; `crates/nexus-contracts/tests/schema_drift_detection.rs` lines 609–634 | High |
| R-V171P0-QC1-010 | doc-rule | plan A1; compass §1.1 A1; actual generated TS in `packages/nexus-contracts/src/generated/local-api/canvas/strategy/` | High |

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 2 |
| 🟡 Warning | 4 |
| 🟢 Suggestion | 4 |

**Verdict**: Request Changes

The schema/codegen bump, route registration, error-envelope extension, and generated-artifact discipline are sound. However, the conflict-modal UX does not meet the locked acceptance criteria, and the test matrix is missing the integration/regression coverage required by A9. These two items are blocking. The remaining warnings (prompt-template write order, chained UI mutations, concurrent-write serialization, and component-file size) should be addressed or explicitly deferred with residual tracking before the iteration is approved.
