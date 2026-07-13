---
iteration_id: V1.115
start_date: 2026-07-13
status: completed
end_date: 2026-07-13
iteration_base_branch: main
target_branch: main
spec_integration_branch: iteration/v1.115
plans:
  - 2026-07-13-v1.115-canvas-adapter-completion
  - 2026-07-13-v1.115-canvas-ux-residual-cluster
  - 2026-07-13-v1.115-compute-manifest-reconciliation
---

# V1.115 Delivery Compass — Canvas & Compute Foundation Completion

## Product story

V1.114 made Canvas and Compute **visible as foundations**. V1.115 makes those
foundations **honest and reusable** so the next iteration can deep-dive without
copying a leaky recipe or trusting an unverified type bridge.

| Who | What they get when V1.115 is Done |
| --- | --- |
| **Authors** | Same canvas domain behavior as today, plus: sortable Outline/Timeline alt-views, World KB nav that always leads somewhere honest, hotkeys that survive React Flow upgrades |
| **Maintainers** | Every shipped canvas **orchestrator** on one adapter contract that means what it says; compute manifest drift fails the gate, not production |
| **Next iteration** | A 5th surface / Strategy `onConnect` / module SDK can land on a complete recipe and a single-source manifest |

**Direction lock mode: interactive** (grill-me with user; three decisions
recorded below). Do not re-litigate.

### Grill-me decisions (locked)

1. **Iteration main line**: A. Foundation completion first (adapter completion
   + contract convergence + Compute manifest reconciliation + residual
   cleanup). No new feature surface this iteration.
2. **Plan split**: 3-plan L budget. Three independent clusters, all Must,
   can run in parallel.
3. **Iteration ID / branch policy**: V1.115; `iteration_base_branch: main`;
   `spec_integration_branch: iteration/v1.115`; `target_branch: main`.

### Scope (one coherent bet)

V1.114 shipped the first foundation slice for the two product pillars (Canvas
adapter + dagre + compute registry + state read). The adapter recipe is real but
only covers **2 of 3 product canvas orchestrators** (Strategy + World KB; Outline
still outside) and has three known contract leaks; the Compute manifest has
deliberate dual-source drift. This iteration **completes and converges the
foundation** — not new product surface.

**Surface inventory (locked):** Normative product surfaces = 3 (Strategy,
Outline+Timeline, World KB per `canvas-strategy-surface.md` §3.3).
`CanvasSurfaceKind` has 4 keys including reserved `world-kb-relationships` —
Relationships today is World KB edges + alt-view, not a separate orchestrator.
The kind stays reserved as a viewport/cache identity key; a dedicated
orchestrator can adopt it later if a relationships route is ever built.
P0 migrates **Outline+Timeline**; it does **not** invent a Relationships canvas.

### Must justification (all three plans)

L budget allows 3–4 business plans. All three are **Must** because they form
one coherent "make the foundation honest" bet — each closes a concrete gap
that V1.114 itself flagged as deferred:

| Plan | Why Must (product) | What fails if deferred |
|------|--------------------|------------------------|
| **P0** Canvas Adapter Completion | Outline still outside the recipe; adapter contract leaks in 3 places (W001/W002/W003) + M001 | Next canvas feature / 5th surface builds on a passthrough + node-ignoring abstraction; debt compounds |
| **P1** Canvas UX Residual Cluster | Author-visible nav/sort debt + one RF-upgrade time bomb + cheap Studio/perf hygiene | RF upgrade silently re-breaks hotkeys; authors keep hitting disabled World KB nav + unsortable alt-views |
| **P2** Compute Manifest Reconciliation | Hand-written `ModuleManifest` ↔ generated `ModuleDetail` JSON-bridge masks drift | Module authoring SDK / more modules land on an unverified bridge |

None is Stretch: each is necessary for the iteration story "the foundation is
honest and reusable." If capacity forces a cut, cut depth *inside* a plan
(preferred cut order in each plan/spec), not an entire plan.

1. **P0 - Canvas Adapter Completion & Contract Convergence (Must, L):**
   Migrate Outline+Timeline (the remaining unmigrated product orchestrator) to
   the adapter. Fix contract leaks: Strategy `projectGraph` passthrough
   (R-V1114P0QC1-W001), World KB `renderInspector`/`renderAltView` ignoring the
   node parameter (R-V1114P0QC1-W002), `useAutoLayout` first-run override
   (R-V1114P0QC1-W003), and non-null assertion (R-V1114P0QC2-M001).
2. **P1 - Canvas UX Residual Cluster Closure (Must, M):** Close the remaining
   post-V1.114 canvas UX/stability residuals: alt-view sort
   (R-V1108P0QC1-M001), hotkey `.react-flow` CSS-class dependency
   (R-V1111P0QC2-W001), World KB sidebar when no worldId
   (R-V1111P1-WORLDS-PICKER), Studio node chrome extract
   (R-V1108P1QC1-S002), `beatParentSceneTitle` O(n)
   (R-V1109-P0-QC3-W002). Capacity cut order: Studio → O(n) before
   author-visible items.
3. **P2 - Compute Manifest Bridge Reconciliation (Must, M):** Reconcile
   hand-written `ModuleManifest` with generated `ModuleDetail`; eliminate the
   JSON round-trip bridge in `registry.rs` (R-V1114P2QC1-W002). Validate via
   schema drift gate. Modules panel behavior unchanged for authors.

### Priority ordering (product)

| Priority | Plan | Why this order (even though parallel) |
| --- | --- | --- |
| **P0** | Adapter completion | Highest leverage for "foundation honest"; unlocks every later canvas deep-dive |
| **P1** | UX residual cluster | Author-visible honesty + RF risk; independent of adapter migration |
| **P2** | Compute manifest | Independent pillar hygiene; enables SDK later; no canvas file overlap |

Do **not** reorder to ship UX polish before adapter honesty if sequencing is
required — adapter contract is the iteration spine. Parallel execution is still
correct (below).

### Plan dependencies

| Plan | Priority | Depends on | Parallel? |
|------|----------|------------|-----------|
| P0 canvas adapter completion | Must | — | Baseline track (adapter + shared shell + Outline) |
| P1 canvas UX residual cluster | Must | — | **Product-independent of P0** — coordinate at integration merge if both touch `outline-canvas*` |
| P2 compute manifest reconciliation | Must | — | **Independent of P0/P1** — compute crate only; no canvas overlap |

## Plans

| plan_id | Name | Status | Notes |
|---------|------|--------|-------|
| 2026-07-13-v1.115-canvas-adapter-completion | P0 - Canvas Adapter Completion & Contract Convergence | Done | Must; Outline migration + W001/W002/W003/M001. QC Approve with residuals (6 low/nit); QA Pass |
| 2026-07-13-v1.115-canvas-ux-residual-cluster | P1 - Canvas UX Residual Cluster Closure | Done | Must; sort + hotkey + sidebar + studio + O(n). QC Approve after fix wave; QA Pass; 5 residuals closed |
| 2026-07-13-v1.115-compute-manifest-reconciliation | P2 - Compute Manifest Bridge Reconciliation | Done | Must; ModuleManifest ↔ ModuleDetail typed From. QC Approve after fix wave; QA Pass; R-V1114P2QC1-W002 closed |

Status values: `Todo` | `InProgress` | `InReview` | `Done` | `Blocked`

## Milestones

| Milestone | Target date | Status |
|-----------|-------------|--------|
| Spec freeze (Review chain done) | 2026-07-13 | pending |
| P0 Dev complete | 2026-07-13 | pending |
| P1 Dev complete | 2026-07-13 | pending |
| P2 Dev complete | 2026-07-13 | pending |
| QC complete | 2026-07-13 | pending |
| Iteration close | 2026-07-13 | pending |

## Acceptance Criteria

**Iteration-level Done gate** — prefer author/maintainer-observable outcomes;
engineering checks are evidence, not the story. IDs map to plan ACs.

### Author/maintainer-observable (product)

- [ ] **AC-I-1 / AC-P0-1..6 — Adapter complete & honest:** Outline+Timeline
  consumes the adapter with no domain-behavior regression; Strategy
  `projectGraph` projects (not passthrough); World KB inspector uses the
  selected node; supplied positions survive first open; no dagre non-null
  assertions; all shipped **orchestrators** on `useCanvasSurface()`
- [ ] **AC-I-2 / AC-P1-1..2 — Alt-views sortable:** chapter list (number,
  volume) and timeline events (event time) offer client-side sort toggles
- [ ] **AC-I-3 / AC-P1-3 — Hotkeys RF-safe:** command-palette conflict-avoidance
  does not depend on undocumented `.react-flow` CSS class
- [ ] **AC-I-4 / AC-P1-4 — World KB nav reachable:** sidebar item is not
  permanently disabled when no world is active (minimal world list or honest
  empty state — not auto-select first world)
- [ ] **AC-I-5 / AC-P1-5..6 — Hygiene closed:** Design Studio consumes node-chrome
  extract; `beatParentSceneTitle` is O(1)/memoized
- [ ] **AC-I-6 / AC-P2-1..5 — Compute manifest single-source:** typed conversion
  (no JSON bridge); drift fails gate; Modules panel results unchanged for
  `basic-combat`

### Engineering evidence (supports the product gate)

- [ ] Strategy, World KB, and Outline+Timeline orchestrators consume
  `CanvasSurfaceAdapter` / `useCanvasSurface()`
- [ ] `useAutoLayout` has no non-null assertions on dagre output (defensive
  fallback)
- [ ] All residuals listed in plan scopes are closed (or accepted with rationale
  + durable tracking)
- [ ] Existing canvas + compute tests pass (no domain-behavior regression);
  new tests cover migrated Outline + reconciled manifest + P1 behaviors
- [ ] `status.json` updated + `tech_debt_summary` refreshed; `wc -c` trending
  down (net residual reduction)

## Non-Goals

Honest freeze — do not let these creep into V1.115 "while we're here":

- **New canvas product surface** (compute graph, session replay, 5th
  orchestrator) — foundation completion only
- **Separate World KB Relationships orchestrator** — kind stays reserved
  (viewport/cache identity key; not a product surface); do not invent a second
  World KB surface this iteration
- **Strategy `onConnect` for inner-graph groups** — deep-dive feature; next
  canvas capability wave
- **Canvas ↔ compute intersection UI** (battle reports / state deltas on
  canvas nodes) — needs converged foundation first
- **Compute state editor** (write/edit `body.state` in UI) — read foundation
  shipped in V1.114; editor is a follow-on deep-dive
- **Compute module authoring SDK / tooling** — future iteration; this iteration
  only reconciles the manifest type so the SDK has a clean target
- **New compute modules** beyond existing `basic-combat`
- **Multi-module composition / chaining** (V2.0+ ABI)
- **CDN module distribution + signing** (V2.0+)
- **Module marketplace / public registry** (V3.0+)
- **Persisted layout positions** to the daemon (still ephemeral; save-layout is
  a follow-on)
- **ELK / alternate layout engines** — dagre only (carried over from V1.114)
- **`preset` → `strategy` CLI/schema rename** (still deferred; UI terminology only)
- **Auto-select first world** when none active (prefer honest picker/empty)
- **Server-side list sort** (F-F1) — client-side alt-view sort only
- **Closing the entire residual slate** (bounded to the post-V1.114 cluster +
  named residuals; not a full burn-down)
- **Dynamic locale bundle splitting** (R-P0-003 — separate future iteration)

## Roadmap Position

- **Current (V1.115): delivered** — Canvas & Compute Foundation Completion —
  the adapter is honest and complete across all 3 shipped orchestrators, the
  post-V1.114 UX residual cluster is closed, and the compute manifest is
  single-source typed. Both pillars' foundations are *reusable*, not only
  *implemented*.
- **Immediate next (pick at next iteration-start; foundation enables all):**
  1. **Canvas deep-dive** — Strategy `onConnect` for inner-graph groups; or a
     5th canvas surface (compute graph / session replay) using the complete
     adapter recipe
  2. **Compute depth** — state editor / human-readable state views; module
     authoring SDK (manifest is single-source)
  3. **Intersection** — surface compute outcomes on canvas (both foundations
     converged)

  Trigger: V1.115 Done + author priority. Owner: PM at iteration-start.

- **North star:** Local-first AI-autonomous creative executor — **Canvas is the
  steering surface**, **Compute Modules are the deterministic engine** — both
  comprehensible, stable, and extensible without premature abstraction.

## Delivery Branch Policy

> Mirror of frontmatter; keep in sync with `.mstar/status.json` `metadata`.

| Field | Value |
|-------|-------|
| `iteration_base_branch` | `main` |
| `spec_integration_branch` | `iteration/v1.115` |
| `target_branch` | `main` |

## Risk Register

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| Outline+Timeline migration introduces projection regression (largest unmigrated orchestrator; complex parentId/extent nesting) | Med | High | SDD per-task review + QC tri-review; all existing outline/timeline tests must pass; projection equivalence diff |
| Fixing Strategy passthrough (W001) changes projection timing (adapter vs useStrategyCanvas) | Med | Med | Keep projection result identical; move the logic, don't rewrite it; diff-test before/after. Architect confirmed timing preserved: both old+new paths use `useMemo` over same `parsed` dep; `useCanvasSurface` memoizes `projectGraph` exactly as the old hook memoized `buildStrategyGraph`. `danglingTargets` surfaced via `ctxRef` (existing pattern). |
| World KB renderInspector node-param fix (W002) changes inspector data source | Med | Med | Inspector currently works via ctxRef; verify the node-param path returns the same data before removing side-channel. Architect classified ctxRef: callbacks + cross-node context stay; only `selection`-driven routing moves to `node`. Edge-selection (relationship) inspector is a known contract gap — stays on orchestrator path, documented. |
| useAutoLayout first-run fix (W003) breaks the "auto-layout on open" UX | Low | Med | Only surfaces that supply positions are affected; today none do (ephemeral layout); guard with has-positions check |
| Outline migration loses selection coordination (chapter/scene/beat sync) — `useCanvasSurface` does NOT provide Outline's selection-sync effect | Med | Med | Architect flagged: orchestrator must add a thin selection-resolver reading `selectedNodeId` from `useCanvasSurface()`. `useCanvasSurface` DOES provide position-merge (do not re-implement). SDD per-task review verifies selection behavior unchanged. |
| `host_functions` enum representation mismatch between hand-written `HostFunction` and generated enum after typed `From` | Low | Low | Architect flagged: both use `snake_case`; T2 verifies the generated enum maps correctly. Compile-time catch if wrong. |
| V1.114 did NOT ship a node-chrome extract — P1 T4 must create it (scope underestimate) | Med | Low | Architect corrected: T4 creates `NodeChromeShell` presentational component first, then consumes it. Capacity cut order still applies (T4 is first cut). |
| Reconciling ModuleManifest ↔ ModuleDetail surfaces field-shape mismatches | Med | Low | JSON round-trip currently masks them; reconciliation targets generated type; run schema drift gate |
| P0 and P1 both touch outline-canvas files (integration conflict) | Med | Low | P0 = adapter migration; P1 = alt-view sort + O(n) — coordinate at integration merge; PM resolves conflicts |
| Misreading "4 kinds" as "4 product surfaces" expands P0 to invent Relationships canvas | Low | Med | Surface inventory locked above; product DoD = 3 orchestrators |

## Iteration workspace

- Workspace: `v1.115/` — iteration specs for canvas adapter completion + canvas UX residuals + compute manifest reconciliation
- Specs: `v1.115/specs/canvas-adapter-completion.md`, `canvas-ux-residual-cluster.md`, `compute-manifest-reconciliation.md`

## Quality Gate Summary

| plan_id | QC decision | QA gate | Residuals | Durable summary |
|---------|-------------|---------|-----------|-----------------|
| 2026-07-13-v1.115-canvas-adapter-completion | Approve with residuals | Pass (mandatory) | 6 R# (all low/nit deferred) | QC tri: adapter honest across 3 orchestrators; W001/W002/W003/M001 fixed; 1038 tests |
| 2026-07-13-v1.115-canvas-ux-residual-cluster | Approve (after fix wave) | Pass (mandatory) | 3 R# (all nit deferred); 5 prior residuals closed | QC tri: 5 UX residuals closed + NodeChromeShell extract; tailwind glob fix wave; 1067 web + 110 studio tests |
| 2026-07-13-v1.115-compute-manifest-reconciliation | Approve (after fix wave) | Pass (mandatory) | 2 R# (all nit deferred); R-V1114P2QC1-W002 closed | QC tri: typed From eliminates JSON bridge; saturating cast fix wave; 53 tests + 4/4 drift gate |

Notes:

- Raw review bundle: `{SDD_DIR}/review/` (ephemeral; do not rely on it after Done).
- Open residual SSOT: `{HARNESS_DIR}/status.json` root `residual_findings[<plan-id>]`.

## Compound Round Summary

- 结晶文档数：1 updated (`knowledge/architecture-patterns/canvas-surface-implementation-pattern.md` — layer 14: adapter contract convergence, written during P0 T5)
- 新增 CONCEPTS.md 条目：0 (adapter convergence + manifest reconciliation are standard vocabulary already documented in code/specs)
- 触发 compound-refresh：否
- Workspace 盘点：`v1.115/specs/*.md` (3 files) — all **kept as iteration snapshots**; cross-iteration knowledge already promoted to layer 14 (canvas pattern) + `compute-module-abi.md` §7.6 (manifest wire-vs-runtime). No workspace promotion needed.

## Iteration Retrospective (minimal)

- 做得好的：(1) Foundation-completion-first direction (grill-me locked) produced a coherent 3-plan iteration that closed exactly the gaps V1.114 flagged — adapter on 2/4 surfaces + 3 contract leaks + manifest JSON bridge; (2) SDD per-task loop caught the dagre compound-graph crash (T1b) and the factual error in P1 T4 (V1.114 did not ship node-chrome extract — architect corrected before implement); (3) Three independent plans ran cleanly in sequence with zero cross-plan merge conflicts; (4) 11 prior residuals closed (5 P1 UX cluster + 6 from earlier iterations) — net slate reduction; (5) Compute manifest now single-source typed — compile-time drift guarantee is stronger than any runtime gate.
- 可改进的：(1) P2 QC found the `expect()` panic surface + missing deliberate-drift test in one fix wave — the implementer should have proactively addressed these given the architect's explicit `From` (not `TryFrom`) directive; (2) P1 QC1 found the Design Studio tailwind content-glob miss (T4 created a new `presentational/` dir but didn't update the Studio tailwind scan) — the implementer should verify build outputs when adding shared modules; (3) `status.json` grew to 55KB — closeout should archive resolved residuals to trend size down.
- 下迭代建议：(1) Resume canvas trajectory with converged foundation — Strategy `onConnect` for inner-graph groups, or a 5th canvas surface (compute graph / session replay) using the now-complete adapter recipe; (2) Compute module authoring SDK / tooling (the manifest is now single-source typed — the SDK has a clean target); (3) Canvas ↔ compute intersection UI (both foundations are now converged); (4) Further residual closure wave + status.json archival.
