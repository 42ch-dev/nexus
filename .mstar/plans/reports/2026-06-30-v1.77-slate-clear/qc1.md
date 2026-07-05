---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-06-30-v1.77-slate-clear"
verdict: "Approve"
generated_at: "2026-06-30"
---

# Code Review Report

## Reviewer Metadata

- Reviewer: @qc-specialist
- Runtime Agent ID: `qc-specialist`
- Runtime Model: minimax-cn-coding-plan/MiniMax-M3
- Review Perspective: Architecture coherence and maintainability risk (Reviewer #1)
- Report Timestamp: 2026-06-30
- Deep review: **triggered** for the **iteration as a whole** (P1 alone is surgical, but the cross-track combination with P0 of 10 commits / 2018+/87- lines / 3 file-disjoint subsystems meets ≥2 trigger signals — change size + multi-module coupling). P1 lens applied here:

Lenses applied (single-reviewer, no subagents):

- Architecture coherence (default)
- Module-boundary (the three P1 fixes are file-disjoint with P0 by design — verified)
- SSOT-duplication (none — P1 closes pre-existing residuals rather than introducing shared state)
- Surgical-discipline (`mstar-coding-behavior`)` — the central lens for P1 since the plan is explicitly defined as "three root-cause fixes; no piggyback refactor"
- Spec ↔ implementation drift
- Maintainability (naming, comment intent, durable-roadmap evidence)

## Scope

- plan_id: `2026-06-30-v1.77-slate-clear`
- Review range / Diff basis: `git diff ba71d9167f6269cd0175b86f202baa3e19b517a6...a2571381b2a9865c6a98ffec461d4a99051a39f0` (10 commits; merge-base `ba71d916` = origin/main, tip `a2571381` = HEAD)
- Working branch (verified): `iteration/v1.77`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus`
- Files reviewed (this plan): `crates/nexus-orchestration/src/tasks/mod.rs` (B1) + `crates/nexus-daemon-runtime/src/api/handlers/world_kb.rs` (B2) + `apps/web/src/components/canvas/world-kb/world-kb-canvas.tsx` (B3). These three are file-disjoint with P0; verified by `git diff --stat ba71d916...HEAD` (no P0 path overlaps with any P1 file).
- Commit range (matches Review range exactly)
- Tools run: `git rev-parse --show-toplevel`; `git branch --show-current`; `git log --oneline ba71d916..HEAD`; `git diff --stat ba71d916...HEAD`; targeted `grep -n` + `read` on each touched file; `git log --all --grep "R-V176QC1-S002" --oneline` (verifies the durable-roadmap entry); structural verification of the symmetric-reverse cap argument at `world_kb.rs:931-948`

## Findings

### 🔴 Critical

(none)

### 🟡 Warning

(none)

### 🟢 Suggestion

- **S-101 — B2 durable-roadmap placement is split across two locations; consider a follow-up consolidation.** The pagination TODO is durable in two places: the `world_kb.rs` source comment (`world_kb.rs:892-899`, references `R-V176QC1-S002`, names the cap as "interim" + explains the wire-contract blocker); and the compass's residual registry (`.mstar/iterations/v1.77-findings-remediation-ui-and-post-canvas-inflection-compass-v1.md` line 201, registers `R-V176QC1-S002` as P1 (B2) with `low` severity). The two cross-reference each other via the residual ID `R-V176QC1-S002`, which is the same ID the B2 commit message uses (`fix(v1.77): cap relationship graph projection payload` per `git log --oneline ba71d916..HEAD` shows `988f7335`). The "deferred road" therefore lives in `status.json`'s `residual_findings` (per `mstar-plan-artifacts`/`mstar-plan-conventions` SSOT) once the plan is registered as Done — **but at present** the residual still exists in the source comment rather than as a registered open residual. **Not blocking** for V1.77 sign-off — the change is correct, and the comment is durable enough for a one-iteration deferral window. Recommended follow-up at V1.78 hygiene: register the open `R-V176QC1-S002` (or its successor ID) in `status.json` `residual_findings` as an open row with `owner` and `target milestone` so the SSOT for residual lifecycle matches the compass cite. This is a PM-owned registration, not an executor fix.

- **S-102 — B2 cap value `GRAPH_RELATIONSHIP_CAP=1000` is reasonable but lacks an instrumented regression test.** The cap is applied via `rows.into_iter().take(GRAPH_RELATIONSHIP_CAP)` (`world_kb.rs:946-948`) and the symmetric-reverse capacity hint follows at `Vec::with_capacity(rows.len().min(GRAPH_RELATIONSHIP_CAP) * 2)` (`world_kb.rs:944`). The doc comment argues it is "well under this" for pre-1.0 local-first datasets, with a future pagination replacement named in the TODO. A regression test that inserts >`GRAPH_RELATIONSHIP_CAP` suggested rows and asserts the response is bounded would close the gap between cap intent and cap behavior. **Not blocking** — the existing `world_kb.rs` tests cover the projection shape with the cap in place; a dedicated cap-exhaustion test is a V1.78+ hygiene add.

- **S-103 — B3 batching constant `PROMOTE_BATCH_SIZE=5` is reasonable but the value choice has no documented why-5 rationale.** The doc comment explains **what** the constant does (unbounded-burst mitigation) and **where** the residual originated (`qc3 S-QC3-001 / R-V176QC3-S001`), but does not justify 5 specifically (i.e. why not 3, why not 10). Recommended **non-blocking** follow-up: a one-line note on the constant declaration ("5 = safe ceiling for Tier-1 concurrent requests against the daemon's request handler; revisit at V1.78+ if a server-side bulk-promote route ships, replacing this entirely"). This is a maintainability nicety, not a defect.

- **S-104 — B1 doc fix is exemplary surgical discipline and should be cited as the field-discoverability template for similar latent-fields in `quality_loop.rs`.** `crates/nexus-orchestration/src/tasks/mod.rs:498-530` rewrites only the `LlmExtractTask` doc comment to:
  - Replace `Vec<quality_loop::KbCandidate>` with `[crate::quality_loop::LlmExtractOutcome]` (correcting the prior doc drift at `mod.rs:514`).
  - Update the parse target from `{ candidates: [...] }` to `{ candidates: [...], relationships: [...] }` (correct).
  - Add a new paragraph (lines 519-528) **warning callers** that the task itself does not persist — and that callers wiring `LlmExtractTask` directly **MUST destructure** `Candidates { candidates, relationships, .. }` to avoid silently losing extracted relationships.

  The comment explicitly references the originating residual (`qc1 F-002 / R-V176QC1-S001`), so future readers can trace from doc → residual → plan. **Not blocking** — this is a model for surgical documentation in the codebase.

### Verdict-supporting notes (informational; not findings)

- **Surgical discipline — exemplary across all three fixes:**
  - **B1** (`crates/nexus-orchestration/src/tasks/mod.rs`) — net change is doc-only. No code paths, no signatures, no tests, no migrations. The change touches only the doc comment block, at the same file location where the drift originated. No piggyback refactor (no re-ordering of sibling items, no tightening of doc style across the file). ✓
  - **B2** (`crates/nexus-daemon-runtime/src/api/handlers/world_kb.rs`) — adds 9 lines for the constant + doc comment, then 4 lines for the cap application (`Vec::with_capacity` arg + `into_iter().take(...)`), and 7 lines for an updated in-function TODO. No unrelated cleanup. The cap is applied **before** symmetric-reverse derivation so a stored edge and its reverse are guaranteed to stay together (`world_kb.rs:941-948`); this is exactly the right architecture decision and is documented in the new comment. ✓
  - **B3** (`apps/web/src/components/canvas/world-kb/world-kb-canvas.tsx`) — replaces the unbounded `Promise.allSettled(rels.map(...))` with a `for` loop batching by `PROMOTE_BATCH_SIZE=5`. The constant declaration + doc comment is 12 lines; the `for` loop is 24 lines replacing the previous 18-line `Promise.allSettled`. No behaviour change for callers (outcomes still collected; failed-count warning still fires; TanStack v5 `mutate()` last-callback pitfall still addressed by `mutateAsync`). ✓

- **No piggyback refactor observed.** A `git diff` on each touched file shows the changes are tightly bounded to the closed residuals. The P1 plan's "surgical" gate is satisfied:
  - B1 changes: 16 lines net (+doc only).
  - B2 changes: 25 lines net (constant + cap + 7-line TODO update).
  - B3 changes: 61 lines net (constant declaration + doc + the `for`-loop rewrite), all in the bulk-promote path.

- **File-disjointness with P0 — verified.** The three P1 files do not overlap with any P0 file:
  - `crates/nexus-orchestration/src/tasks/mod.rs` — Rust backend (LLM task doc only).
  - `crates/nexus-daemon-runtime/src/api/handlers/world_kb.rs` — Rust backend (world-kb graph endpoint).
  - `apps/web/src/components/canvas/world-kb/world-kb-canvas.tsx` — Web SPA (canvas bulk-promote path).

  No P0 file (`apps/web/src/lib/findings-lifecycle.*`, `apps/web/src/lib/nexus/types.ts`, `apps/web/src/lib/nexus/browser-client.ts`, `apps/web/src/lib/nexus/query-keys.ts`, `apps/web/src/api/queries.ts`, `apps/web/src/api/findings-mutation.test.tsx`, `apps/web/src/components/findings/finding-detail-panel.*`, `apps/web/src/components/status-badge.tsx`, `apps/web/src/pages/findings-page.tsx`, `apps/web/src/lib/nexus/adapter-contract.test.ts`, `apps/web/DESIGN.md`, `apps/web/DESIGN.dark.md`, `.mstar/knowledge/specs/findings-lifecycle.md`, `.mstar/knowledge/specs/local-api-surface-conventions.md`, `.mstar/knowledge/specs/web-ui.md`) is touched by any P1 commit (`git show --stat 0369d391 988f7335 2e7ed2bc`).

- **`wire_contracts_changed: FALSE` — verified.** No `schemas/**` files in the P1 diff (`git diff --stat ba71d916...HEAD -- schemas/` returns no P1 paths). No `@42ch/nexus-contracts` regenerated types. The plan's `wire_contracts_changed: false` claim holds.

- **Spec / implementation drift — none for P1.** No spec files are touched by P1. The slates each close a single bug class:
  - B1 = doc-only fix; the implementation behavior (`LlmExtractTask::evaluate` returning `LlmExtractOutcome`) is unchanged and already matches the spec naming.
  - B2 = cap-only fix; the implementation behavior (projection shape, OC-conflict semantics) is unchanged.
  - B3 = batching-only fix; the per-promotion semantic (PATCH/expected_version/update) is unchanged.

- **Residual closure evidence — partially automated, partially architectural:**
  - B1 (`R-V176QC1-S001`): closed via doc fix only. The warning in `mod.rs:519-528` is the durable record for future `LlmExtractTask` direct callers.
  - B2 (`R-V176QC1-S002`): closed via cap with a TODO pointing to future pagination as the durable direction; needs status.json SSOT registration (S-101 follow-up).
  - B3 (`R-V176QC3-S001`): closed via batching; the constant + its doc are the durable record.

## Source Trace

| Finding ID | Source Type | Source Reference | Confidence |
|-----------|-------------|------------------|------------|
| S-101 | manual-reasoning + static-analysis | compass residual registry at `.mstar/iterations/v1.77-findings-remediation-ui-and-post-canvas-inflection-compass-v1.md:201`; source comment `world_kb.rs:892-899`; commit message `988f7335` cites `R-V176QC1-S002` | High (referenced everywhere correctly; gap is the SSOT registration itself, PM-owned) |
| S-102 | manual-reasoning | cap at `world_kb.rs:46-50`; apply at `world_kb.rs:946-948`; doc rationale at `world_kb.rs:50-58` | Medium (cap value is judgment-based; documented in pre-1.0 scope; defense-in-depth on a future pagination route) |
| S-103 | manual-reasoning | constant declaration at `world-kb-canvas.tsx:39-50`; loop at `world-kb-canvas.tsx:226-256` | Medium (constant rationale is absent; the surrounding safety guarantee is in the residual cite) |
| S-104 | manual-reasoning | doc rewrite at `crates/nexus-orchestration/src/tasks/mod.rs:498-530` (lines 519-528 are the new warning paragraph) | High (exemplary surgical doc; cites both residual and task responsibility) |

P1 surgical-discipline verification (all High confidence):

| Plan item | Source | Verification |
|-----------|--------|--------------|
| B1 doc-only | `git diff ba71d916...HEAD -- crates/nexus-orchestration/src/tasks/mod.rs` | Only doc comment lines; no code paths |
| B2 cap | `git diff ba71d916...HEAD -- crates/nexus-daemon-runtime/src/api/handlers/world_kb.rs` (`+25/-0`) | Constant + cap + TODO only; no unrelated cleanup |
| B3 batch | `git diff ba71d916...HEAD -- apps/web/src/components/canvas/world-kb/world-kb-canvas.tsx` | Constant + for-loop rewrite; no unrelated cleanup |
| File-disjoint with P0 | `git show --stat 0369d391 988f7335 2e7ed2bc` | Three files, none overlapping the P0 set |
| `wire_contracts_changed: false` | `git diff --stat ba71d916...HEAD -- schemas/` (empty for P1) | No schema files touched |

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 4 |

**Verdict**: Approve
