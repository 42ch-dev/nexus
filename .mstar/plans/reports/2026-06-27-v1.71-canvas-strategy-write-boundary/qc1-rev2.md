---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: 2026-06-27-v1.71-canvas-strategy-write-boundary
verdict: Approved with suggestions
generated_at: 2026-06-28
---

# Code Review Report — V1.71 P0 Fix-Wave Re-review

## Reviewer Metadata
- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Runtime Model: k2p7
- Review Perspective: Architecture coherence and maintainability risk (Reviewer #1)
- Report Timestamp: 2026-06-28T08:45:00Z

## Scope
- plan_id: 2026-06-27-v1.71-canvas-strategy-write-boundary
- Review range / Diff basis: `git diff 1afdd592..5ed2ee6c` (P0 fix-wave merge `5ed2ee6c`)
- Working branch (verified): iteration/v1.71
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 13 changed paths in the fix wave; focused on `apps/web/src/components/canvas/conflict-modal.tsx`, `apps/web/src/components/canvas/strategy-canvas.tsx`, `crates/nexus-daemon-runtime/src/api/handlers/strategy.rs`, `crates/nexus-daemon-runtime/tests/strategy_patch.rs`, `apps/web/DESIGN.md`, `apps/web/DESIGN.dark.md`
- Commit range: 1afdd592..5ed2ee6c
- Tools run:
  - `cargo +nightly-2026-06-26 fmt --all --check` (passed)
  - `cargo clippy --workspace -- -D warnings` (passed)
  - `cargo test --workspace` (passed)
  - `pnpm --filter @42ch/nexus-contracts run build` (passed)
  - `pnpm --filter web typecheck` (passed)
  - `pnpm --filter web test` (147 tests passed, including 8 ConflictModal tests)
  - `pnpm --filter web build` (passed)

## Revalidation of Original Findings

| ID | Original severity | Disposition | Evidence |
|---|---|---|---|
| **R-V171P0-QC1-001** | Critical | **Resolved** | `ConflictModal` extracted to `apps/web/src/components/canvas/conflict-modal.tsx` with focus trap, `aria-live="polite"` status region, Escape-to-dismiss, return-focus on unmount, and side-by-side review panel. "Reapply my edit" is disabled when server and draft fields overlap. Eight component tests in `conflict-modal.test.tsx` cover headline, overlap, live region, and actions. |
| **R-V171P0-QC1-002** | Critical | **Resolved** | `crates/nexus-daemon-runtime/tests/strategy_patch.rs` added 5 integration tests: state rename + revision bump, stale revision conflict, invalid transition condition, prompt-template rollback on validation failure, and concurrent serialization. Additional unit tests in `strategy.rs` cover rename references, rollback, and concurrent lock behavior. |
| **R-V171P0-QC1-003** | Warning | **Resolved** | `patch_prompt_template_inner` now stages the template via temp file + rename, then runs full `validate_preset_yaml`. On validation failure it calls `rollback_template_write` to restore the previous file contents and leaves `preset.yaml` / `revision:` untouched. |
| **R-V171P0-QC1-004** | Warning | **Partially resolved** | `handleSave` still chains up to three sequential `mutateAsync` calls. The canvas now tracks `workingRevision`, bumps it after each successful partial patch, and refetches canonical state on conflict, which prevents self-conflict and keeps the UI fresh. However, there is still no rollback if a later patch fails after earlier ones succeeded, so a multi-field save can leave the graph partially persisted. |
| **R-V171P0-QC1-005** | Warning | **Resolved** | All three mutation handlers run inside `tokio::task::spawn_blocking`, acquire an advisory `flock` on `bundle/.strategy-lock`, re-check `base_revision` after lock acquisition, and write `preset.yaml` atomically with temp + rename + fsync. The concurrent test proves exactly one of two same-revision writers succeeds. |
| **R-V171P0-QC1-006** | Warning | **Partially resolved** | `ConflictModal` is now extracted, but `InspectorOverlay`, `ValidationPanel`, and `ArtifactsList` remain inline in `strategy-canvas.tsx` (~571 lines). The file still mixes graph sync, overlay, edit-form state, validation, conflict handling, and helpers. |
| **R-V171P0-QC1-007** | Suggestion | **Resolved** | `canvas-write-stale-bg` now uses `color-mix(in srgb, {colors.amber-700} N%, transparent)` in both `DESIGN.md` and `DESIGN.dark.md`; `src/index.css` maps it to `var(--color-amber-700)`. |
| **R-V171P0-QC1-008** | Suggestion | **Still open** | `StrategyCanvas` auto-refetch on conflict is implemented, but there is still no automated component or e2e test that exercises the full edit → save → refetch flow. |
| **R-V171P0-QC1-009** | Suggestion | **Resolved** | Route registration, error-envelope variants, schema drift detection, and the `@42ch/nexus-contracts` 0.7.0 bump were already sound and remain unchanged. |
| **R-V171P0-QC1-010** | Suggestion | **Still open** | The plan and compass still reference generated TypeScript output in `apps/web/src/api-types/canvas/`, but that directory does not exist; the actual generated TS lives in `packages/nexus-contracts/src/generated/` and is consumed via the workspace package. |

## Findings

### 🔴 Critical

None.

### 🟡 Warning

- **R-V171P0-QC1-004 — `handleSave` can still leave the graph partially persisted (Partially resolved).**
  The fix wave added per-patch revision tracking and conflict refetch, which removes the self-conflict case, but the UI still submits up to three independent mutations without a compensating transaction. If the state patch succeeds and the transition patch fails (e.g., validation), the graph now has a mutated state and an unchanged transition. The user sees a toast and the refetched graph, but there is no explicit partial-failure UI or rollback. Before V1.71 ships, either split saves per inspector section or document the chosen multi-patch behavior and add a partial-failure indicator.

- **R-V171P0-QC1-006 — `strategy-canvas.tsx` still mixes several UI concerns (Partially resolved).**
  `ConflictModal` is now a focused module with tests, but `InspectorOverlay`, `ValidationPanel`, and `ArtifactsList` remain inline. The file is still ~570 lines and combines graph rendering, live overlay, form state, validation, conflict handling, and artifact display. Extract the remaining sub-components in a follow-up cleanup to keep the canvas maintainable as the surface grows.

### 🟢 Suggestion

- **R-V171P0-QC1-008 — Add a UI test for edit → save → refetch (Still open).**
  The `ConflictModal` tests do not replace a higher-level test that verifies `StrategyCanvas` invalidates the preset query after a successful patch and refetches on conflict. Add a focused component test before closing this residual.

- **R-V171P0-QC1-010 — Correct the documented TypeScript codegen target (Still open).**
  Update `2026-06-27-v1.71-canvas-strategy-write-boundary.md` A1 and the compass §1.1 A1 to state that generated TypeScript is in `packages/nexus-contracts/src/generated/` and consumed via `@42ch/nexus-contracts`, not in `apps/web/src/api-types/`.

- **Copy divergence in the conflict modal headline/body.**
  The compass and `canvas-strategy-surface.md` §3.5 specify the headline "This node changed while you were editing." and a body sentence naming the node label, field, and revision. The implementation uses "This state changed while you were editing." and a shorter "Server revision is now N. Choose how to reconcile your changes." The structural UX is correct, but the product copy differs from the locked acceptance text. Verify with `@product-manager` whether the current wording is acceptable for β or should be reconciled to the spec.

## Status.json Update

Updated `.mstar/status.json` as part of this re-review:
- Marked **R-V171P0-QC1-004** and **R-V171P0-QC1-006** as `Partially resolved` and retargeted them to `V1.72 or V1.71 P-last cleanup`.
- Kept **R-V171P0-QC1-008** and **R-V171P0-QC1-010** as `Still open` with updated notes and the same target.
- Corrected the `severity` field for the two open warning-class residuals from the invalid legacy value `"warning"` to the canonical enum (`high` for R-004, `medium` for R-006), per `.mstar/AGENTS.md` / `mstar-plan-artifacts` severity SSOT.
- Refreshed `metadata.tech_debt_summary` to match the computed rollup (8 open: 1 high, 1 medium, 5 low, 1 nit).

## Source Trace

| Finding ID | Source Type | Source Reference | Confidence |
|---|---|---|---|
| R-V171P0-QC1-001 | manual-reasoning + test-run | `apps/web/src/components/canvas/conflict-modal.tsx` lines 36–285; `conflict-modal.test.tsx` 8 tests passing | High |
| R-V171P0-QC1-002 | manual-reasoning + test-run | `crates/nexus-daemon-runtime/tests/strategy_patch.rs`; `cargo test --workspace` green | High |
| R-V171P0-QC1-003 | manual-reasoning | `crates/nexus-daemon-runtime/src/api/handlers/strategy.rs` lines 977–1012 (stage → validate → rollback) | High |
| R-V171P0-QC1-004 | manual-reasoning | `apps/web/src/components/canvas/strategy-canvas.tsx` lines 163–223 | High |
| R-V171P0-QC1-005 | manual-reasoning + test-run | `strategy.rs` lines 63–92 (flock), 562–577 (re-check), tests `concurrent_patch_state_serializes_on_lock` green | High |
| R-V171P0-QC1-006 | static-analysis | `apps/web/src/components/canvas/strategy-canvas.tsx` 571 lines; `ConflictModal` extracted to separate file | High |
| R-V171P0-QC1-007 | static-analysis | `apps/web/DESIGN.md` lines 199–202; `apps/web/DESIGN.dark.md` lines 199–202; `apps/web/src/index.css` lines 100–103, 185–188 | High |
| R-V171P0-QC1-008 | static-analysis | `apps/web/src/components/canvas/strategy-canvas.tsx` lines 316–335; no new `strategy-canvas` component test | High |
| R-V171P0-QC1-010 | doc-rule | `.mstar/plans/2026-06-27-v1.71-canvas-strategy-write-boundary.md` A1; `apps/web/src/api-types/` does not exist | High |
| Copy divergence | doc-rule | `canvas-strategy-surface.md` §3.5 vs `conflict-modal.tsx` lines 148–154 | Medium |

## Summary

| Severity | Count |
|---|---|
| 🔴 Critical | 0 |
| 🟡 Warning | 2 (1 partially resolved, 1 partially resolved) |
| 🟢 Suggestion | 4 (2 still open, 1 copy divergence, 1 resolved) |

**Verdict**: Approved with suggestions

The two original Critical findings are resolved. The backend serialization and atomicity guarantee for `preset.yaml` is now in place and tested. The remaining open items are non-blocking architecture/maintainability debt that should be tracked through P-last / V1.72 cleanup rather than blocking the β write-boundary ship. The product copy divergence in the conflict modal should be confirmed by `@product-manager` before final sign-off.
