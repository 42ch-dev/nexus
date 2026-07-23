---
iteration_id: V1.133
start_date: 2026-07-23
end_date: 2026-07-23
status: completed
iteration_base_branch: main
target_branch: main
spec_integration_branch: iteration/v1.133
scale: M
direction_lock_mode: autonomous
plans:
  - 2026-07-23-v1.133-p0-brace-param-route-sweep
  - 2026-07-23-v1.133-p1-legacy-residual-burndown
---

# V1.133 Delivery Compass - Legacy false-Done repair + residual burn-down

## Scope

**Locked direction (autonomous):** Fix legacy issues and false-Done content from iterations V1.128–V1.132. Two priorities:

1. **Brace-param route sweep (P0)** — Restore ID-based daemon surfaces that creators actually use today. V1.132 P0 fixed 404s on 4 surfaces (presets/sessions/modules/strategy) by converting `{param}` → `:param` (matchit 0.7 syntax), but ~31 other daemon routes still use the broken brace form. Those routes return framework 404 for any real ID: works, worlds, KB, findings, memory, agent-host sessions, orchestration schedules, reading annotations. The web client actively calls these endpoints. **User impact now:** opening or mutating a work/world/KB entry/finding/session by ID fails with an opaque not-found, even when the entity exists — the largest remaining false-Done from V1.128–V1.132.

2. **High-impact residual burn-down (P1)** — Close the most impactful open residuals from V1.126–V1.132 that represent actual bugs, incomplete features, or stale/incorrect residual descriptions. Focus on low+medium severity items with clear fix paths; nits are closed opportunistically when the code is already being touched. **User impact now:** residual debt hides real reliability/security gaps (daemon restart, profile switch, display-name XSS) and stale rows mislead prioritization so creators keep hitting known rough edges while the backlog looks “parked for polish.”

**Rationale:** User direction: "继续修复遗留问题，检查之前几个迭代（尤其是128~上一个）是否有遗留没解决或者假Done的内容，如果有就彻底解决。" The brace-param sweep is the most critical false-Done: V1.132 P0 identified the root cause (matchit 0.7 brace vs colon syntax) but only fixed 4 of ~35 affected routes. The remaining ~31 routes silently 404 for real IDs, breaking core Control Room / canvas data paths. P1 makes the residual ledger trustworthy again and lands the small set of bugs that still bite users.

### Priority of pain (user-value rank)

1. **P0 - Brace-param route sweep** (highest). Creators cannot reliably load or edit ID-scoped resources (works, worlds, KB, findings, memory review, agent sessions, schedules, reading annotations) because the daemon never matches the path. Every endpoint that takes an ID in the path returns framework 404. This is product-broken today, not polish.
2. **P1 - Legacy residual burn-down**. Close high-impact open residuals from V1.126–V1.132: fix actual bugs (not just nits), correct stale residual descriptions (e.g., R-V1130P1-QC1-C-002 claims missing IPC that exists), and archive resolved items so future iterations do not re-discover the same false debt.

## Architecture Locks

**Lock date:** 2026-07-23
**Architect-locked:** 2026-07-23 (Seat 2)
**Branch path:** `main` -> `iteration/v1.133` -> `main`

### P0 - Route syntax ownership

All daemon route paths in `crates/nexus-daemon-runtime/src/api/mod.rs` must use matchit 0.7 `:param` capture syntax. The brace form `{param}` is a literal path segment and never matches real IDs. The fix is mechanical for ~33 of ~34 routes: replace `{param_name}` with `:param_name` in all `.route()` path strings. Most handler signatures do not change — `axum::extract::Path` already works with `:param` captures. Regression tests must exercise at least one route from each affected route group with a real ID to prove the 404 is fixed.

**Exception: `{operation_id}:cancel` (line 70).** This is the only route where a brace param immediately precedes a `:suffix` without a `/` separator. Simple `{operation_id}` → `:operation_id` would produce `:operation_id:cancel`, which matchit 0.7 rejects (consecutive `:a:b` captures are not allowed). The fix uses the same pattern as `logout_creator`: change the route to `/:operation_id` (no suffix) and have the handler strip the trailing `:cancel` at the top of its body before UUID parsing. This is a **minor** handler change (add 2-3 lines of strip logic), not a signature or contract change.

**Chapter `{n}` → `:n`.** The nested chapter router (lines 448-456) uses `/{n}`, `/{n}/outline`, `/{n}/body`. Since `:param` captures only until the next `/`, `/:n` matches `42` but not `42/outline` — route ordering within the nested router is unambiguous and requires no reordering.

`wire_contracts_changed: false` — route path syntax is a framework-layer concern; the wire contract (URL path shape) does not change because clients already send real IDs. The `:cancel` handler change is internal request-path parsing only.

### P1 - Residual burn-down ownership

PM triages all open residuals in `status.json` `residual_findings` and identifies the subset that represents actual bugs or incomplete features (not just nits/polish). Each fix is surgical: touch only the code needed to close the residual. Stale residuals (describing issues that no longer exist) are corrected and closed. Nit-level residuals are closed opportunistically when the code is already being touched for a higher-severity fix.

## Plans

| Wave | plan_id | Name | Status | blocked_by |
|------|---------|------|--------|------------|
| 1 | `2026-07-23-v1.133-p0-brace-param-route-sweep` | Brace-param route sweep | Todo | - |
| 1∥ | `2026-07-23-v1.133-p1-legacy-residual-burndown` | Legacy residual burn-down | Todo | - |

**Scale budget:** M -> **2** business plans.

## Milestones

| Milestone | Target date | Status |
|-----------|-------------|--------|
| Spec freeze (Review & Edit lock) | 2026-07-23 | pending |
| Wave 1 (P0 ∥ P1) | 2026-07-23 | pending |
| Iteration close | 2026-07-23 | pending |

## Acceptance Criteria

### Brace-param sweep (P0) — user-observable + verification

- **AC-0 (user-observable).** For each affected product surface that the web client already calls with a real ID (works, worlds, KB entries, findings, memory pending-review, agent-host sessions, orchestration schedules, reading annotations, narrative worlds), a request with a known-good ID is **not** rejected with framework path-match 404 solely because the route still uses brace syntax. Unknown IDs may still 404/4xx from handler logic — that is correct behavior.
- **AC-1 (ledger).** All daemon routes in `crates/nexus-daemon-runtime/src/api/mod.rs` use `:param` matchit 0.7 syntax. Zero `{param}` brace-form route paths remain (`rg` / grep clean on route path strings).
- **AC-2 (regression).** Automated tests prove at least one route from each affected group returns a non-framework-404 status for a real ID (200 or expected domain 4xx/empty), covering: agent-host, schedules, KB, narrative worlds, reading, memory, works, worlds+KB, findings.
- **AC-3 (crate health).** `cargo test -p nexus-daemon-runtime` passes.
- **AC-4 (crate health).** `cargo clippy -p nexus-daemon-runtime -- -D warnings` passes.

### Residual burn-down (P1) — user-observable + ledger

- **AC-5 (triage complete).** PM triages all open residuals in `status.json`; each is classified as **fix-now**, **stale-close**, or **defer-with-rationale** (human-verification smokes stay open with explicit note). No residual left unclassified.
- **AC-6 (user-facing bugs closed).** Every fix-now item is resolved in code with regression evidence; after close, the user-visible failure mode described by that residual no longer reproduces on the integration branch (or residual is reclassified with evidence if already fixed).
- **AC-7 (trustworthy ledger).** Stale items are corrected and closed; deferred items carry a current rationale (not “V1.12x+ polish” boilerplate). `status.json` `residual_findings` and `archived/residuals/` reflect the updated lifecycle.
- **AC-8 (debt rollup).** Tech-debt summary refreshed via `tech-debt-rollup.sh` so open counts match post-burn-down reality.

## Non-Goals

Explicit and defensible for this M-scale iteration:

| Non-goal | Why out |
|----------|---------|
| Full 64-residual burn-down (every nit) | Scale M; only high-impact bugs + stale ledger repair. Nits stay deferred with rationale unless code is already open. |
| New features or visual redesign | Direction is false-Done / residual repair, not net-new product surface. |
| Human-verification smokes (R-VI-003 Dock, R-V1131P0-QC2-W-001 Overlay) | Require human interactive smoke; keep open, do not fake close. |
| Engine auto-start, platform/cloud, Timeline / Fork / Computable deferred features | Separate product tracks; not residual false-Done of V1.128–V1.132. |
| DF-70 execution-mode matrix, DF-71 menu-bar daemon | Named deferred features; not in this iteration’s locked direction. |
| Wire-contract / schema version bumps for route paths | Clients already send real IDs; framework path syntax only. |
| Broad daemon or web refactor beyond surgical residual/route fixes | YAGNI; false-Done repair must stay mechanical. |

## Roadmap Position

- **Current (V1.133):** **delivered** - brace-param route sweep (34 routes fixed) + legacy residual burn-down (5 closed, 2 superseded archived, 3 fix-now code fixes).
- **Next:** Deeper Creator entity Chat; Orchestrator 功能区 beyond interim menu; opportunistic DF-70/71; open human smokes (`R-VI-003` Dock, titlebar Overlay guide); remaining 62 open residuals (mostly nits).
- **Prior:** V1.132 dogfood load blocker + titlebar drag + VI retune + Create-only hub (#170).

## Delivery Branch Policy

| Field | Value |
|-------|-------|
| `iteration_base_branch` | `main` |
| `spec_integration_branch` | `iteration/v1.133` |
| `target_branch` | `main` |

## Risk Register

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Route fix breaks existing tests that mock brace-form URLs | Low | Med | Search test files for `{param}` patterns; update mocks |
| Residual burn-down scope creep | Med | Med | PM triage before implement; fix-now list frozen at plan lock |
| P0 and P1 touch overlapping code | Low | Low | P0 is daemon-only; P1 is web+daemon; coordinate at integration |
| “Fixed” residual still fails for users (wrong root cause) | Med | High | Prefer user-repro or automated path with real ID; stale-close only with code evidence |


## Quality Gate Summary

| Plan | QC | QA | Merge |
|------|----|----|-------|
| P0 brace-param-route-sweep | Pass (tri; fix wave for regression tests) | Pass | Done |
| P1 legacy-residual-burndown | Pass (tri; all suggestions) | Pass | Done |

## Compound Round Summary

**Package inventory (`v1.133/`):**

| Artifact | Disposition |
|----------|-------------|
| `specs/brace-param-route-sweep.md` | **Keep snapshot** (normative iteration spec; knowledge already in `daemon-matchit-colon-capture.md`) |
| `specs/legacy-residual-burndown.md` | **Keep snapshot** (iteration-specific triage; not reusable) |

**Knowledge doc updated:** `knowledge/architecture-patterns/daemon-matchit-colon-capture.md` - updated with V1.133 P0 full sweep findings (34 routes, `:cancel` edge case, regression test approach, false-Done history).

**No new knowledge docs created** - the brace-param sweep updates an existing doc (Q5=Yes, high overlap). The residual burn-down was standard triage (Q1-Q8 score <=2, skip).

## Iteration Retrospective (minimal)

- **What worked:** Code-first research caught the massive brace-param false-Done (31 routes beyond V1.132 P0's 4); SDD per-task review + QC tri caught the missing regression test coverage; PM-thread triage efficiently closed stale residuals.
- **Friction:** status.json grew to 37KB (over 20KB cap) - Profile B compaction needed at close; tech_debt_summary manual refresh missed by_target/by_plan fields initially.
- **Carry:** 62 open residuals (mostly nits); human-verification smokes (Dock, Overlay); handler doc comments retain brace form notation.

