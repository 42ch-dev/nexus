---
iteration_id: V1.126
start_date: 2026-07-20
status: locked
iteration_base_branch: main
target_branch: main
spec_integration_branch: iteration/v1.126
plans:
  - 2026-07-20-v1.126-p0-shell-selection-submenu
  - 2026-07-20-v1.126-p1-canvas-directed-axis
  - 2026-07-20-v1.126-p2-composite-timeline-endpoint
  - 2026-07-20-v1.126-p3-status-compaction-residual-cleanup
---

# V1.126 Delivery Compass — Shell + Canvas deepening + Tech-debt gate

> **Direction lock mode: autonomous** (`/iteration-loop`, scale **L** — 4 business plans).
> Caller direction: 以之前迭代 Dogfood feedback 的问题为核心调研方向，探索继续推进深化的路线，继续着手优化和重构.
>
> **Phase 1 Review & Edit chain:** product-manager seat 1 → architect seat 2 → writing-specialist seat 3 (empty-response fallback to PM inline hygiene per V1.124 pattern) → PM lock. Direction is **locked** — do not re-question the V1.125 Non-Goal follow-up bundle (selection→submenu shell + canvas directed axis) + rolled-forward composite Timeline route + tech-debt gate.

## Autonomous direction lock record

**Scale budget:** L = 4 business plans (harness process not counted).

**Caller direction mapping:**

| Caller phrase | Candidate coverage |
|---------------|--------------------|
| 之前迭代 Dogfood feedback 的问题 | V1.125 Non-Goal follow-ups (selection shell + canvas axis); Timeline dogfood N+1 + Moment wire residuals from V1.123; shell/profiles polish residuals from V1.117/V1.120 |
| 探索继续推进深化的路线 | Canvas directed-axis visual deepening (Brief/Narrative/Moment spine); Composite Timeline daemon endpoint (deeper World-building + global Timeline perf) |
| 继续着手优化和重构 | status.json compaction (128KB → <20KB); V1.121+V1.122+V1.123+V1.124+V1.125 residual cleanup (~263 open low/nit); shell/canvas code hygiene derived from residual cluster |

**Branch policy (autonomous resolve per `references/autonomous-direction-lock.md`):**

- `iteration_base_branch: main` — resolved from `status.json` root `metadata.iteration_base_branch`.
- `target_branch: main` — resolved from `status.json` root `metadata.target_branch` (matches V1.122 PR #156 / V1.123 PR #157 / V1.125 PR #159 documented project policy).
- `spec_integration_branch: iteration/v1.126` — new branch cut from `main`.

This is the documented project policy; not a silent `main` default.

### Candidates evaluated

Research base: V1.125 ship artifacts (`delivery-compass.md` Non-Goals line 80–83 + Roadmap Position line 88 + Compound Round Summary line 143), V1.124 P3 promotion classification + Roadmap Position, V1.123 retrospective ("下迭代建议：dogfood + residual cleanup + status.json compaction + composite endpoint evaluation"), `knowledge/deferred-features-cross-version-tracker.md` (`DF-V1122-DEEPER-WB` rolled forward V1.122→V1.123→V1.124+→V1.125+; `DF-V1123-COMPOSITE-ENDPOINT`, `DF-V1123-STATUS-COMPACT`, `DF-V1123-RESIDUAL-CLEANUP`), `status.json` (128 KB; 263 open low/nit residuals; `tech_debt_summary.total_open: 77` rolling count), V1.125 implementation evidence (`apps/web/src/pages/worlds-page.tsx::handleCreateWorldClick` empty — selection mode unimplemented; `apps/web/src/components/canvas/timeline-canvas/timeline-canvas-adapter.tsx` — `when-axis` exists but no "directed center axis" visual treatment per Brief/Narrative layer).

| # | Candidate | Trade-off | Verdict |
|---|-----------|-----------|---------|
| A | **Shell + Canvas deepening + Tech-debt gate** — picks up V1.125 Non-Goal follow-ups (selection→submenu shell, canvas directed axis), rolled-forward composite Timeline route (`DF-V1122-DEEPER-WB`), and deferred tech-debt gate (status.json compaction + residual cleanup). 4 plans within L budget. | Largest surface breadth (shell + canvas + daemon route + harness hygiene); but matches every clause of caller direction and is dominated by V1.125 explicitly-named next items. | **LOCKED** — directly implements every clause of caller direction; every plan has evidence-ranked roadmap priority (Non-Goal follow-up or DF-* rolled forward ≥ 2 iterations). |
| B | Shell-only (selection→submenu + shell polish) | Smaller scope; but leaves Canvas axis + composite endpoint + tech-debt gate still deferred — wrong direction fit ("deepening + refactoring"). | Rejected (partial — skips caller's "deepening" and "refactoring" clauses). |
| C | Canvas-only deepening (directed axis + composite endpoint + DF-V1123-MOMENT-WIRE) | Strong product deepening; but skips caller's "refactoring" clause and the V1.125 Non-Goal shell follow-up. | Rejected (partial — skips shell follow-up + tech-debt gate). |
| D | Fork UI (`DF-V1122-FORK-UI`) | Highly ranked in V1.124 P3 roadmap; but Fork is **L** standalone and crowds out the cleanup gate the caller asked for. | Rejected (this iteration) — stays roadmap; pick when Fork becomes the top dogfood ask. |
| E | Computable pillar UI (`DF-V1122-COMPUTABLE-UI`) | Elevates the third STRATEGY pillar; but pillar surfacing is independent of dogfood deepening and not a V1.125 Non-Goal. | Rejected (this iteration) — stays roadmap. |
| F | Harness UI rename (`DF-V1122-HARNESS-RENAME`) | Polish/IA honesty; lower PMF urgency than A and not a V1.125 follow-up. | Rejected (this iteration) — stays roadmap. |
| G | status.json compaction + residual cleanup only | Real tech-debt pressure (128 KB; 263 open); but caller said "继续推进深化的路线" — pure cleanup misses the deepening clause. | Rejected (as standalone); folded into A as **P3** so the gate lands alongside the deepening work. |

### Evidence base for A

- **V1.125 Non-Goal → V1.126+ next iteration is explicit.** V1.125 `delivery-compass.md` § Non-Goals lines 80–83 lists four V1.126+ items; § Roadmap Position line 88 names the same set; § Compound Round Summary line 143 repeats. This is the highest evidence rank per autonomous ranking heuristics (deferred / roadmap next).
- **`DF-V1122-DEEPER-WB` has rolled forward three times** (V1.122 → V1.123 → V1.124+ → V1.125+). The V1.123 architect lock (`iterations/v1.123/specs/three-layer-architecture.md` §5.4) made the route promotion **conditional on Brief/Moment wire choice**; V1.123 picked Brief-on-KeyBlock so the route was not required for PMF. V1.124 / V1.125 did not pick it up. V1.123 P3 global-Timeline `useQueries` N=5 fan-out + per-World graph fetch (cited in V1.123 P3 residuals as the N+1 risk) is the concrete pressure point — composite endpoint addresses it.
- **`DF-V1123-COMPOSITE-ENDPOINT` is the V1.123-registered sibling** (V1.123 compass line 413 item (j)) — composite daemon endpoints for global Timeline + cross-surface fan-out. P2 closes both DF rows together.
- **status.json is at 128 KB, 6.4× over the 20 KB threshold** (`wc -c`; `.mstar/AGENTS.md` § Pre-merge checklist #5). `DF-V1123-STATUS-COMPACT` + `DF-V1122-STATUS-COMPACT` (superseded but tracked) both name this. `DF-V1123-RESIDUAL-CLEANUP` names the V1.121+V1.122+V1.123 accumulation; V1.124 + V1.125 added 7 + ~12 more.
- **V1.125 shell IA work left `worlds-page.tsx::handleCreateWorldClick` as a no-op stub** (line 39–41 — `// CreateWorldDialog wires here when the wire contract ships`). The V1.125 Non-Goal "Selection → submenu shell" is the structural answer to "what does the author do **after** they pick a World or Work in the sidebar" — currently nothing happens in the sidebar row itself beyond navigation. This is the most concrete dogfood follow-up.
- **V1.123 P2 Work Timeline adapter `client?: unknown` slot + WorkTimelineLayer closed-union residuals** (`status.json::residual_findings[2026-07-18-v1.123-work-timeline-narrative-moment]`) flag layer-extensibility friction that the directed-axis visual refactor (P1) can address in the same pass.
- **Canvas visual fidelity gap (V1.124 P0 fixtures accepted with residuals):** Studio Timeline fixtures shipped, but no "directed center axis" visual treatment differentiates Brief + Narrative layers in the gallery — both layers project to the same horizontal `when-axis` (`timeline-canvas-adapter.tsx` line 438–441). The V1.125 Non-Goal "canvas Brief/Narrative directed center axis" is the deferred visual bet.
- **STRATEGY alignment — Three Pillars:** Canvas (directed axis + composite endpoint) is the active hero pillar post-V1.123; Harness (selection shell) supports author flow into Canvas; cross-cutting tech-debt gate keeps the codebase shippable. No new pillar invented.

### Locked direction (single sentence)

Deepen the post-V1.125 Control Room and Canvas bet along the V1.125 Non-Goal follow-ups: give authors a **selection-mode submenu shell** (P0) and a **canvas-directed center axis** that visually differentiates Brief / Narrative / Moment layers (P1); ship the **composite Timeline daemon endpoint** so global Timeline + World Timeline reads stop fanning out N=5+ client fetches (P2); and **compact `status.json` + close the V1.121+…+V1.125 low/nit residual cluster** so the harness stops tripping the 20 KB hygiene line (P3).

### Dependency graph (locked)

```
P0 (shell selection submenu)         ← Must; no upstream; frontend-only
   ├── P1 (canvas directed axis)     ← Must; independent (canvas/ files; visual-only)
   ├── P2 (composite endpoint)       ← Must; independent (daemon Rust + schemas + web consumer)
   └── P3 (status.json + residual cleanup)  ← Must; harness hygiene; independent (no product code)
```

P0, P1, P2, P3 touch disjoint files (shell layout vs canvas adapters vs daemon routes + web queries vs harness status.json). P0–P3 Prepare in parallel; Execute **serially** (P0 → P1 → P2 → P3) per `mstar-iteration` §2.6 per-plan loop. P3 runs last so residual cleanup can include residuals opened by P0/P1/P2.

## Scope

本迭代锁定的 spec 点（**post-V1.125 dogfood deepening + tech-debt gate**）：

- **S1 (P0 — Must)**: Sidebar selection-mode submenu — picking a World or Work in the Creator tab opens a contextual **popover submenu** anchored to the row, exposing Timeline entry, KB (World) / Outline (Work) entry, agent assignment, rename, and delete. **Pain today:** the sidebar row only navigates — the author has no in-place chooser, and the agent-assignment flow currently requires a round-trip to Settings. Trigger contract is product-locked in P0 spec ND-1 (click navigates; `Enter` / `⌘.` / `Ctrl+.` / `•••` button open the submenu). Closes V1.125 Non-Goal "Selection → submenu shell (World/Work selected mode + agent dialog)".
- **S2 (P1 — Must)**: Canvas **directed center axis** — Brief, Narrative, and Moment layers each get a differentiated visual spine (era-spanning arrow vs discrete event pins vs chapter-scoped micro-axis; layer-color accent already in tokens) instead of sharing one flat `when-axis`. Studio fixtures gain the directed-axis treatment per layer. **Pain today:** all three layers project onto the same undifferentiated horizontal line, so a glance does not reveal the active layer's scale or arrow-of-time. Closes V1.125 Non-Goal "Canvas Brief/Narrative directed center axis".
- **S3 (P2 — Must)**: Composite Timeline daemon endpoint — `GET /v1/daemon/timeline/overview` returns per-World counts (`era_count`, `event_count`, `last_event_at`) and cursor pagination across visible Worlds. **Pain today:** `global-timeline-view.tsx` fans out N=5–10 parallel `kb/graph` calls (V1.123 P3 N+1 residual) and `worlds-page.tsx` shows only "last edited Xh ago" because per-World era/event counts would add another N+1 round. The composite endpoint pre-aggregates the counts the UI already needs — no event rows in the response (deep-link per-World Timeline still calls `kb/graph` for the detail). Closes `DF-V1122-DEEPER-WB` (route promotion slice — overview only) + `DF-V1123-COMPOSITE-ENDPOINT` (implicit registration; see tracker-discipline note in P2 spec ND-9).
- **S4 (P3 — Must)**: `status.json` < 20 KB after compaction + ≥ 50 low/nit residuals closed (V1.121 → V1.125 cluster). **Pain today:** `status.json` is 128 KB (6.4× the 20 KB hygiene line) and 263 open low/nit residuals keep tripping the pre-merge checklist. Tech-debt gate lands in the same iteration as the deepening work so the harness stops blocking P-last hygiene checks; P3 runs last so residuals opened by P0/P1/P2 stay eligible for archival in a future iteration, not this one.

## Plans

| plan_id | Name | Status | Notes |
|---------|------|--------|-------|
| `2026-07-20-v1.126-p0-shell-selection-submenu` | P0 — Sidebar selection-mode submenu shell | Todo | **Must** (plan). Tasks: T1 trigger+dismiss Must · T2 contents Must · T3 a11y+i18n Must · T4 Studio fixture Should (V1.106 invariant — must land in-iteration; "Should" only flags that the fixture is supporting, not blocking, ship). V1.125 Non-Goal follow-up. |
| `2026-07-20-v1.126-p1-canvas-directed-axis` | P1 — Canvas directed center axis (Brief/Narrative/Moment) | Todo | **Must** (plan). Tasks: T1 Brief Must · T2 Narrative Must · T3 Moment+Work-Narrative Must · T4 Studio fixture Should. V1.125 Non-Goal follow-up. |
| `2026-07-20-v1.126-p2-composite-timeline-endpoint` | P2 — Composite Timeline daemon endpoint + web consumer | Todo | **Must** (plan). Tasks: T1 schema+codegen Must · T2 daemon handler Must · T3 web consumer migration Must · T4 tracker update Must. Closes `DF-V1122-DEEPER-WB` overview slice + `DF-V1123-COMPOSITE-ENDPOINT` (implicit registration). |
| `2026-07-20-v1.126-p3-status-compaction-residual-cleanup` | P3 — status.json compaction + V1.121+…+V1.125 residual cleanup | Todo | **Must** (plan; tech-debt gate — without it the iteration fails pre-merge checklist #5). Tasks: T1 compaction Must · T2 closure-review Must · T3 tracker Must · T4 gate-reset Must. |

Status values: `Todo` | `InProgress` | `InReview` | `Done` | `Blocked`

## Milestones

| Milestone | Target date | Status |
|-----------|-------------|--------|
| Phase 1 compass locked | 2026-07-20 | in-progress (PM seat 1 + architect seat 2 + writing-specialist seat 3; PM lock pending) |
| P0 shell selection submenu Done | 2026-07-20 | pending |
| P1 canvas directed axis Done | 2026-07-20 | pending |
| P2 composite endpoint Done | 2026-07-20 | pending |
| P3 status compaction + residual cleanup Done | 2026-07-20 | pending |
| Iteration close + PR | 2026-07-21 | pending |

## Acceptance Criteria

Observable product criteria (each AC maps to exactly one plan or process gate; no orphans):

- **AC-V1126-1** (P0 → S1): In the Creator tab sidebar, activating a World row or a Work row via keyboard (`Enter` / `⌘.` / `Ctrl+.`) or the row-end `•••` button opens a contextual **popover submenu** (280px anchored to row's right edge; auto-flips near viewport edge — shape locked in P0 spec ND-2) with at minimum: Timeline entry link, KB entry link (World only) / Outline entry link (Work only), agent assignment affordance, and per-entity quick actions (rename, delete). A plain mouse **click on the row body navigates** (existing behavior preserved — least surprise). Submenu dismisses on outside-click, Esc, route change, or `Tab` out. Keyboard reachable (arrow keys navigate within submenu; focus returns to row on Esc). Submenu state is **not** URL-persisted (transient).
- **AC-V1126-2** (P1 → S2): On World Timeline (Brief + Narrative layers) and Work Timeline (Narrative + Moment layers), each layer projects onto a **directed center axis** with a layer-differentiated visual spine: Brief = era-spanning horizontal arrow with gradient ticks; Narrative = discrete event pins on a fine-grained axis; Moment = chapter/scene-scoped micro-axis. The `canvas-layer-{brief,narrative,moment}-accent` tokens (already in `tokens.css`, V1.124 P1 gallery) drive the spine color. Studio Timeline fixtures (V1.124 P0) gain the directed-axis treatment per layer in light + dark. No semantic zoom regression vs V1.123 P4.
- **AC-V1126-3** (P2 → S3): `GET /v1/daemon/timeline/overview` returns `{ worlds: [{ world_id, title, era_count, event_count, last_event_at }], cursor, total_worlds }` (cursor-paginated, default cap 20 worlds — **smallest author-valuable slice**; no `recent_events` rows in the response, deep-link per-World Timeline still calls `kb/graph` for detail). Web `global-timeline-view.tsx` consumes **one** composite call instead of N=5–10 `kb/graph` fan-out (per V1.123 P3 N+1 residual); `worlds-page.tsx` activity surface gains era/event counts from the same call. `DF-V1122-DEEPER-WB` row updated: route promotion slice **closed** (full World-scoped `GET /v1/daemon/worlds/{world_id}/timeline` row stays open if P2 ships only the overview shape). `DF-V1123-COMPOSITE-ENDPOINT` row **closed** (note: the DF ID was referenced in V1.123 compass + `status.json::residual_findings` tracking_links but was never registered as a §2.3 open row in the tracker — see P2 spec ND-9 tracker-discipline).
- **AC-V1126-4** (P3 → S4): After compaction, `wc -c .mstar/status.json` < 20_000 bytes. Profile B compaction (`archived/plans-done.json` + `archived/plans/<id>.json`) extended to residuals: ≥ 50 low/nit residuals from V1.121 → V1.125 plans archived to `archived/residuals/<plan-id>.json` (existing pattern). `tech_debt_summary.total_open` ≤ 30 (from 77). No open `medium`/`high`/`critical` residual is archived unless explicitly closed in P3 (closed-by label required).
- **AC-V1126-5** (process): No new `{KNOWLEDGE_DIR}/` documents from Phase 1 Review chain. Knowledge crystallization deferred to Phase 3 `mstar-compound`.
- **AC-V1126-6** (process): Compass `status: locked` after PM lock; all four plans registered in `status.json` with `spec_integration_branch: iteration/v1.126`.

**AC → plan map (no orphans):** AC-1 → P0 · AC-2 → P1 · AC-3 → P2 · AC-4 → P3 · AC-5/6 → process.

## Non-Goals

Concrete exclusions (if a PR does any of these, it is out of V1.126 scope):

- **NG-1**: No Fork creation/merge UI (`DF-V1122-FORK-UI`). Stays roadmap; pick when it becomes the top dogfood ask.
- **NG-2**: No Computable pillar UI / compute-on-timeline (`DF-V1122-COMPUTABLE-UI`, `DF-V1122-COMPUTE-ON-TIMELINE`). Not a V1.125 follow-up; pillar surfacing stays roadmap.
- **NG-3**: No Harness UI rename (`DF-V1122-HARNESS-RENAME`). Polish-only; stays roadmap.
- **NG-4**: No Schedule→cron role creation UX (V1.125 Non-Goal; stays deferred).
- **NG-5**: No `POST /v1/daemon/narrative/worlds` (Create World wire contract) or new Create World dialog wiring. P0 selection submenu reuses existing `worlds-page.tsx` Create World fallback when the client method is absent.
- **NG-6**: No full World-scoped `GET /v1/daemon/worlds/{world_id}/timeline` route promotion (only the overview composite endpoint in P2). The full per-World `TimelineEvent` route stays open under `DF-V1122-DEEPER-WB` remainder.
- **NG-7**: No multi-timeline (`DF-V1123-MULTI-TIMELINE`) or cross-World merge (`DF-V1123-GLOBAL-TIMELINE-MERGE`). Stays roadmap.
- **NG-8**: No WorkOutline scene/beat wire migration (`DF-V1123-MOMENT-WIRE`). P1 directed-axis uses fixture data; real Moment-on-wire data stays roadmap.
- **NG-9**: No new schemas for Moment-on-wire / Brief-on-World DTO. P2 ships an **additive** overview DTO; existing `WorldKbGraphResponse` + `TimelineEvent` domain table are the source.
- **NG-10**: No V1.121 design-elevation token sweep or arbitrary-value Tailwind cleanup (V1.121 residual cluster stays scoped to its own roadmap, `DF-V1122-V1121-RES`). P3 archives rows that are observably stale or fixed-by-other-plans; it does not re-do the V1.121 work.
- **NG-11**: No desktop menu-bar daemon control (`DF-71`). Stays opportunistic roadmap.
- **NG-12**: No Studio-first visual **rework** of V1.124 P0 fixtures (P1 only **adds** the directed-axis treatment; existing chrome stays).
- **NG-13**: No Work-scoped timeline-event route (`GET /v1/daemon/works/{work_id}/timeline`) and no `event_type=moment` enum extension on `TimelineEvent` — the rejected V1.123 alternative (`three-layer-architecture.md` §6.2 Moment-on-TimelineEvent vs Moment-on-Outline) stays rejected. Moment continues to read from `WorkOutline` until `DF-V1123-MOMENT-WIRE` lands.
- **NG-14**: No `AgentPicker` dialog component refactor. P0 reuses the existing `AgentPicker` (V1.110 + V1.119 catalog) **as-is** — only its invocation mode changes (called from a submenu instead of Settings). A PR that redesigns agent picker chrome is out of scope.
- **NG-15**: No mobile / sidebar-below-`lg` selection-menu UX. The Creator sidebar is hidden below the `lg` breakpoint today; the submenu is desktop-only. A future mobile-aware selection surface is a separate roadmap item (not currently DF-tracked).
- **NG-16**: No bulk multi-select batch actions in the sidebar (P0 submenu is single-row contextual). Multi-select belongs to a future list-detail-page refactor, not the sidebar surface.

## Roadmap Position

- **Current iteration (V1.126)**: Shell + Canvas deepening + Tech-debt gate — V1.125 Non-Goal follow-ups land; composite Timeline endpoint closes the rolled-forward `DF-V1122-DEEPER-WB` route slice; `status.json` returns under 20 KB.
- **Next iteration (V1.127+) candidates** (pick after dogfood; owner: product-manager): (a) Fork UI (`DF-V1122-FORK-UI`); (b) Computable pillar UI (`DF-V1122-COMPUTABLE-UI`); (c) compute-on-timeline (`DF-V1122-COMPUTE-ON-TIMELINE`); (d) World Moment layer (`DF-V1123-WORLD-MOMENT`); (e) Work Brief layer (`DF-V1123-WORK-BRIEF`); (f) Moment-on-wire migration (`DF-V1123-MOMENT-WIRE`); (g) Schedule→cron role UX (V1.125 Non-Goal); (h) Harness UI rename (`DF-V1122-HARNESS-RENAME`); (i) desktop menu-bar daemon control (`DF-71`). **Trigger:** V1.126 shipped + dogfood feedback on selection submenu + directed axis.
- **Promotion triggers recorded this iteration:**
  - **Selection submenu chrome** (`@web-shell/selection-submenu` or similar): promote to `@42ch/nexus-ui` when a **second non-shell consumer** needs the same submenu shape. Currently shell-only.
  - **Directed-axis layer spine** (`@web-canvas/directed-axis-spine`): promote to `@42ch/nexus-ui` when a **non-canvas consumer** needs the spine primitive. Currently canvas-only.
  - **Composite Timeline client** (`useTimelineOverview` query): stays app-local; the wire DTO ships in `@42ch/nexus-contracts` per codegen.
- **最终目标**: Every Nexus surface expresses one coherent literary-computational design language, with the Timeline/Canvas bet deepening iteratively and the harness hygiene gate staying under threshold so future iterations stop paying tech-debt interest. V1.126 closes the largest rolled-forward cluster (composite endpoint + selection shell) and resets the residual count.

## Delivery Branch Policy

> Mirror of frontmatter; keep in sync with `.mstar/status.json` `metadata`.

| Field | Value |
|-------|-------|
| `iteration_base_branch` | `main` |
| `spec_integration_branch` | `iteration/v1.126` |
| `target_branch` | `main` |

## Risk Register

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| Selection submenu conflicts with existing Work Outline row active state in `sidebar.tsx::isActiveItem` | Medium | Medium | P0 spec locks submenu trigger contract (click vs alt-click vs hover) before implement; existing active-row logic stays; submenu is **additive** chrome on top |
| Directed-axis visual treatment regresses V1.123 P4 semantic zoom thresholds | Medium | High | P1 preserves `useViewport()` and layer-swap factory rebuild pattern (V1.123 P4 T2); directed-axis is **visual decoration** on the same `when-axis`, not a re-projection |
| Composite endpoint response shape drifts from existing `WorldKbGraphResponse` consumers | Low | Medium | P2 ships **additive** overview DTO (`TimelineOverviewResponse`); existing routes unchanged; web consumer migrates in P2 only |
| `wire_contracts_changed: true` (P2 only) introduces codegen coordination overhead | High (realized) | Low | P2 ships schema + codegen + Rust handler + web consumer in **one commit** per repo policy; CI `Rust fmt & clippy` + `web-build` run sequentially; P0/P1 stay `wire_contracts_changed: false` |
| status.json compaction archives a residual that P0/P1/P2 needed | Low | Medium | P3 runs **last** (after P0/P1/P2 QC) so any new residuals those plans open are kept; only V1.121 → V1.125 residuals are eligible for archival |
| Selection submenu introduces a new `@web-shell/*` alias root not in V1.124 P3 audit | Medium | Medium | P0 reuses existing `shell-sidebar-chrome.tsx` presentational pattern; new alias only if extraction is justified (≥ 2 consumers); P3 P3 audit updated if alias lands |
| Phase 1 Review chain diverges on scope | Low | Medium | Direction locked autonomous; specialists edit within § Scope + Plans. Out-of-scope ideas → roadmap only. |
| Subagent dispatch reliability (V1.124 retrospective flagged empty responses) | Medium | Medium | PM commits on behalf of implementers only when subagent returns empty; SDD per-task loop preserves reviewer isolation |
| Submenu trigger convention conflict — `⌘.` (macOS) also opens the IDE keyboard-shortcuts sheet in VS Code and friends | Low | Low | Architect seat 2 ratifies final trigger contract; product recommendation is **`Enter` (when row focused) opens submenu** with `Open Timeline` as item #1, so a keyboard user who meant to navigate just presses `Enter → Enter`. `⌘.`/`Ctrl+.` and `•••` button are additive power-user / mouse paths. |
| Author confusion if submenu content differs across row types (World shows "Open KB"; Work shows "Open Outline") | Medium | Low | P0 spec ND-3 makes the conditional item explicit with a single position slot; copy uses the DESIGN.md §Voice "name the changed object in dialog title" rule (`Delete <World Name>` / `Delete <Work Name>`) so the entity kind is always clear. |
| P2 response-shape over-engineering (e.g. embedding `recent_events[]` in the overview) bloats payload + breaks the "smallest author-valuable slice" rule | High (caught at seat 1) | Low | **Locked at seat 1:** response shape drops `recent_events` (compass AC-V1126-3 + P2 spec ND-2). Deep-link per-World Timeline still calls `kb/graph` for event rows. |
| P3 archival hides real product debt (e.g. closing a `low` residual that later turns into a `medium` finding) | Low | Medium | P3 spec ND-2 eligibility rule requires `Done` + all nit/low + V1.125-or-earlier; medium/high/critical NEVER archived without explicit closure label (ND-4); 20 KB gate stays as standing pre-merge checklist (ND-7); real tech-debt roadmap items get notes.json entries. |
| **P1 spine renderer introduces React Flow custom-node lifecycle edge cases** (AQ-5) | Low | Medium | React Flow custom node re-renders on layer swap (same factory-rebuild lifecycle as V1.123 P4). The spine node carries no interactive handles; it is a read-only visual decoration. Edge case: if the spine custom node's `useReactFlow()` hook leaks a stale viewport ref across layer swaps, the spine will offset. Implementer verifies via unit test asserting spine position after layer-swap. |
| **P2 handler file split introduces `timeline.rs` module routing** | Low | Low | New `handlers/timeline.rs` + `mod timeline;` declaration follows the existing module pattern (`narrative.rs`, `world_kb.rs`, `outline.rs`). Route registration in `mod.rs` follows the existing `get(..., get(handlers::timeline::...))` pattern. Low risk — additive; no existing handler is moved. |
| **P3 SDD single-task 8-step JSON sequence exceeds typical SDD session duration** (AQ-7) | Medium | Low | T1 is the longest task in the iteration; 8 sequential `jq` + edit steps with a single verification gate. Fallback to inline execution per P3 plan preamble if SDD single-task scope is challenged. No intermediate malformed states — the 8 steps are atomic per plan. |
| **Profile B drift during P3 compaction** | Low | Medium | P3 extends Profile B to residuals. The archived file shape (`archived/residuals/<plan-id>.json`) mirrors the existing `archived/plans/<plan-id>.json` pattern. If the shape drifts (e.g., new keys not in the existing pattern), future compaction tooling may break. Implementer verifies by loading one archived file with `jq` and confirming key parity with the existing pattern. |
| **`⌘.` chord conflict surfaces in dogfood** (AQ-1) | Medium | Low | Already mitigated: `Enter` is the primary keyboard trigger; `⌘.`/`Ctrl+.` is additive only. If dogfood reports persistent chord conflict with VS Code / Electron editors, `Shift+F10` is the documented fallback — a spec update, not a code change. |

## Architecture locks (architect seat 2)

Per-plan architecture decisions ratified at seat 2. Each verdict is a **locked handoff** — implementers treat these as non-negotiable architecture contracts. Where seat 1 flagged open questions (AQ-1–AQ-7), the architect verdict is final.

### P0 — Shell selection submenu

| Decision | Verdict | Rationale |
|----------|---------|-----------|
| **Trigger contract** (AQ-1) | `Enter` primary; `⌘.`/`Ctrl+.` additive power-user chord | VS Code / Electron bind `⌘.` to shortcuts sheet; avoiding `⌘.` as sole keyboard trigger prevents platform conflict. `Enter → Enter` path (open submenu, then first-item "Open Timeline") serves keyboard-only users without a chord. `⌘.`/`Ctrl+.` is registered as secondary; `Shift+F10` is the explicit fallback if dogfood reports persistent chord conflict. |
| **`Enter` override + test matrix** (AQ-2) | Both in same task (T1) | Splitting the Enter override from the test update creates a gap where shipping behavior and assertions diverge. T1's `sidebar.test.tsx` assertions must be retargeted in the same commit as the `sidebar.tsx` Enter handler change. |
| **`@web-shell/selection-submenu` alias** (AQ-3) | **Extract** to new alias root | The Studio fixture is the second consumer (app → Studio), satisfying the V1.106 ≥ 2 consumers rule. The presentational shape (menu items, keyboard nav, focus trap) is pure presentational extractable to `@web-shell/selection-submenu`; the App wrapper injects routing callbacks (`useNavigate`) and i18n (`t()`) via props. The extraction covers: `SelectionSubmenu` component, `SelectionMenuItem` types, focus-trap lifecycle. App-owned: row data mapping, `useNavigate` handlers, i18n strings. |
| **`AgentPicker` reuse contract** | Reuse as-is; dialog mode only | NG-14 locked — no `AgentPicker` component refactor. The submenu invokes the existing `AgentPicker` in dialog mode via a callback prop; the `AgentPicker` component file is **not** edited. |
| **Sidebar integration contract** | `ShellSidebarChrome` gains `renderSubmenu?: (item: ShellNavItem) => ReactNode` render-prop | The submenu popover is anchored to the row's right edge via the chrome's row layout. Focus-trap lifecycle lives in `selection-submenu.tsx` (presentational); the chrome only owns the popover portal and `isOpen` state per row. |
| **`wire_contracts_changed`** | `false` — **CONFIRMED** | No IPC additions; no new PATCH routes; `AgentPicker` is reused unchanged. Frontend chrome only. |

### P1 — Canvas directed axis

| Decision | Verdict | Rationale |
|----------|---------|-----------|
| **Moment spine layout** (AQ-4) | **Density-encoding** (length ∝ scene count) — the rhythm break | The three spines MUST be perceptually distinct per ND-7. Brief+Narrative both encode time-span by segment length (chronological-axis convention). Moment encoding **density** (scene count per chapter) deliberately breaks that convention, signaling "you are at scene precision, not event/era precision." Uniform-length segments would risk conflating with the Narrative discrete-pin rhythm. Documented as intentional divergence — not a defect, a feature. |
| **Spine renderer** (AQ-5) | **React Flow custom node** | The spine shares the React Flow canvas lifecycle (zoom, pan, layer-swap rebuild). A custom node has access to the viewport transform, participates in the same re-render cycle as existing layer nodes, and benefits from React Flow's render batching. Background renderer (SVG overlay) would require separate zoom-sync wiring and is a novel rendering path that complicates the factory-rebuild pattern. |
| **Decoration-only invariant** | **LOCKED** — directed-axis is overlay on existing `when-axis` | The V1.123 `projectBrief`/`projectNarrative`/`projectMoment` projection logic is **not** altered. P1 **adds** a `directedAxisSpine` field to each projection result; the spine is rendered on top of the same geometry. No re-projection, no zoom-band morph. |
| **V1.123 P4 semantic-zoom preservation** | **LOCKED** — constant decoration, no morph | The directed-axis spine is a constant visual element — it does **not** change appearance between zoom bands. The V1.123 P4 zoom thresholds (0.55–0.70) and layer-swap factory-rebuild pattern are preserved. The spine re-renders on layer swap (same lifecycle as today). Verified against `canvas-strategy-surface.md` §3.3.3 layer-swap contract. |
| **V1.123 P4 factory-rebuild preservation** | **LOCKED** | The `createTimelineCanvasAdapter(ctxRef)` + `createWorkTimelineCanvasAdapter(ctxRef)` stable-factory pattern is preserved. The spine component is owned by the adapter; destruction + recreation on layer swap follows the existing V1.123 lifecycle. |
| **Extraction rule** | ≥ 3 consumers = extract to `@web-canvas/directed-axis-spine`; else app-local | World Timeline Brief + World Timeline Narrative + Work Timeline Narrative + Work Timeline Moment = 4 consumer slots. If T1+T2+T3 produce a single reusable `DirectedAxisSpine` component used by ≥ 3 of them, extract. Extraction covers the visual spine renderer only; layer-specific projection data stays in the adapter. |
| **Studio fixture augmentation** | Extend V1.124 P0 fixtures; light + dark; all three layer spines visible side-by-side per ND-7 | The Studio review is the **visual differentiation gate**: if Brief + Narrative + Moment read as the same rhythm with different color, that is a defect — re-cut before P1 ship. |
| **`wire_contracts_changed`** | `false` — **CONFIRMED** | Visual decoration on existing `when-axis` geometry; existing `canvas-layer-{brief,narrative,moment}-accent` tokens consumed. No new DTOs, no daemon changes, no codegen. |

### P2 — Composite Timeline endpoint

| Decision | Verdict | Rationale |
|----------|---------|-----------|
| **Handler file location** (AQ-6) | **New `crates/nexus-daemon-runtime/src/api/handlers/timeline.rs`** | The composite endpoint is a new timeline-overview surface, not a narrative state read. `narrative.rs` handles World list/get (`list_worlds`, `get_world`). Separating the handler avoids bloating an existing focused file; `timeline.rs` may accumulate future timeline-specific handlers (e.g., per-World timeline route promotion under `DF-V1122-DEEPER-WB` remainder). Route registration follows the existing pattern in `mod.rs`. |
| **Response shape** | **CONFIRMED** as seat 1 locked: `{ worlds: [{ world_id, title, era_count, event_count, last_event_at }], cursor, total_worlds }` | No `recent_events[]` — smallest author-valuable slice. Deep-link per-World Timeline still calls `kb/graph` for event rows. |
| **Route registration pattern** | Follow existing handler registration in `mod.rs` | New `get("/v1/daemon/timeline/overview", get(handlers::timeline::get_timeline_overview))` in the daemon routes section. Mirror the existing `narrative.rs` handler pattern: `State(WorkspaceState)` extract, `NarrativeGateway` access, `NexusApiError` error envelope. |
| **Codegen + single-commit policy** | Schema + codegen + Rust handler + web consumer in **one commit** | Per root `AGENTS.md`; CI `Rust fmt & clippy` + `pnpm run codegen` + `web-build` run sequentially. |
| **`NexusClient` interface addition** | `getTimelineOverview(cursor?: string): Promise<TimelineOverviewResponse>` | Add to `NexusClient` interface in `apps/web/src/lib/nexus/types.ts`; browser impl in `desktop-capabilities.ts`. |
| **`wire_contracts_changed`** | `true` — **CONFIRMED** | Additive schema: new `schemas/daemon/responses/timeline-overview-response.schema.json` + codegen + Rust handler. No existing route or DTO mutated. |

### P3 — Status compaction + residual cleanup

| Decision | Verdict | Rationale |
|----------|---------|-----------|
| **T1 sizing** (AQ-7) | **Single SDD task**; fallback inline per plan preamble | The 8 steps are structurally sequential (archival → compaction → verify). Splitting would create intermediate malformed `status.json` states. Each step has its own checkbox for progress tracking. If the single-task scope exceeds one SDD session, fall back to inline execution (PM dispatches `fullstack-dev` directly) per the P3 plan preamble. |
| **`closure_note` schema** | String enum: `"Fixed by <plan-id>"`, `"Stale — <reason>"`, `"Superseded — <reason>"`, `"Archived as low/nit bulk (V1.126 P3 gate)"` — exact four values from seat 1 | Each archived residual entry in `.mstar/archived/residuals/<plan-id>.json` carries a `closure_note` field with exactly one of these four string patterns. |
| **Archived residual file shape** | Mirror `.mstar/archived/residuals/<plan-id>.json` pattern | Each file is a JSON array of archived residual entries from one plan. Each entry carries the original `residual_findings[]` fields plus `closure_note` (see above) and `archived_at` (ISO 8601 timestamp). |
| **`tech_debt_summary` refresh** | Run `./mstar/plan-artifacts/scripts/tech-debt-rollup.sh` (or equivalent) | Count semantics: `total_open` ≤ 30 post-compaction; `by_severity`, `by_target`, `by_plan`, `updated_at` refreshed. No prose fields. |
| **`wire_contracts_changed`** | `false` — **CONFIRMED** | Harness hygiene only; no schemas, no daemon, no web consumer. |

## Iteration package

> Sibling paths under `.mstar/iterations/v1.126/` — not in `specs/` or `knowledge/`. Promoted to knowledge at iteration-close via `mstar-compound`.

| Path | Kind | Status |
|------|------|--------|
| `README.md` | index | active |
| `specs/shell-selection-submenu.md` | spec (P0) | product-reviewed, architect-locked, writing-hygiene done |
| `specs/canvas-directed-axis.md` | spec (P1) | product-reviewed, architect-locked, writing-hygiene done |
| `specs/composite-timeline-endpoint.md` | spec (P2) | product-reviewed, architect-locked, writing-hygiene done |
| `specs/status-compaction-residual-cleanup.md` | spec (P3) | product-reviewed, architect-locked, writing-hygiene done |

Plans: `.mstar/plans/2026-07-20-v1.126-p0-shell-selection-submenu.md` · `.mstar/plans/2026-07-20-v1.126-p1-canvas-directed-axis.md` · `.mstar/plans/2026-07-20-v1.126-p2-composite-timeline-endpoint.md` · `.mstar/plans/2026-07-20-v1.126-p3-status-compaction-residual-cleanup.md`.

## Quality Gate Summary

> Filled at iteration-close. Human summary only; per-plan gate details stay in each main plan, and open residual SSOT stays in `.mstar/status.json`.

| plan_id | QC decision | QA gate | Residuals | Durable summary |
|---------|-------------|---------|-----------|-----------------|
| `2026-07-20-v1.126-p0-shell-selection-submenu` | TBD | TBD | TBD | TBD |
| `2026-07-20-v1.126-p1-canvas-directed-axis` | TBD | TBD | TBD | TBD |
| `2026-07-20-v1.126-p2-composite-timeline-endpoint` | TBD | TBD | TBD | TBD |
| `2026-07-20-v1.126-p3-status-compaction-residual-cleanup` | TBD | TBD | TBD | TBD |

Notes:

- Raw review bundle: `{SDD_DIR}/review/` (ephemeral; do not rely on it after Done).
- Open residual SSOT: `.mstar/status.json` root `residual_findings[<plan-id>]`.

## Compound Round Summary

> Filled at iteration-close.

- 结晶文档数：TBD
- 新增 CONCEPTS.md 条目：TBD
- 触发 compound-refresh：TBD

## Iteration Retrospective (minimal)

> Filled at iteration-close.

- 做得好的：TBD
- 可改进的：TBD
- 下迭代建议：TBD
