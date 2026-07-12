---
iteration_id: V1.111
start_date: 2026-07-12
end_date: 2026-07-12
status: completed
iteration_base_branch: main
target_branch: main
spec_integration_branch: iteration/v1.111
plans:
  - 2026-07-12-v1.111-canvas-command-palette
  - 2026-07-12-v1.111-sidebar-canvas-ia
  - 2026-07-12-v1.111-design-studio-canvas-gallery
  - 2026-07-12-v1.111-frontend-consolidation-sweep
---

# V1.111 Delivery Compass — Frontend Canvas Navigation + Design Studio + Consolidation

## Scope

**Direction lock mode: autonomous** — caller (`/iteration-loop`) supplied
`direction = "UI/UX aesthetics, UI bugs, canvas features, design studio, leftover
issues — frontend"` and `scale = L`.

Four **Must** plans resuming the canvas trajectory and paying down the frontend
leftover debt. The roadmap signal is triple-confirmed: the V1.108, V1.109, and
V1.110 compasses all name **shared command palette**, **sidebar canvas IA**, and
**Design Studio canvas preview** as the next-iteration canvas direction; V1.110's
retrospective explicitly recommends "恢复 canvas 轨迹（command palette / sidebar
IA）". The user direction adds **UI/UX polish** and **遗留问题 (leftover frontend
residuals)**, which a consolidation plan addresses.

1. **P0 — Shared Canvas Command Palette (⌘K):** A greenfield command palette
   overlaying the Control Room, giving authors keyboard-driven access to canvas
   navigation, node creation, and surface switching. The single highest-value
   discoverability gap — no command palette exists today (grep confirms zero
   `CommandPalette`/`cmdk` usage). Named "next" in V1.108/V1.109/V1.110 compasses.
2. **P1 — Sidebar Canvas IA:** Restructure the sidebar to nest canvas surfaces
   (Strategy / Outline+Timeline / World KB) under Work context, so authors reach
   the right surface without top-level flat nav. Named "next" in V1.108/V1.109
   compass Roadmap Position.
3. **P2 — Design Studio Canvas Surface Gallery:** Expand the existing
   `CanvasSurfacesFixtures` (V1.108 P1) with Strategy + World KB surface chrome
   so all three canvas surfaces are previewable under studio-first — closing the
   gap that the fixture currently mirrors **Outline chrome only**.
4. **P3 — Frontend Consolidation & UI Polish Sweep:** Close the cluster of
   post-V1.106–V1.110 frontend residuals (toast duplication `R-V1106P0-001`,
   Studio chrome static-mirror `R-V1109-P0-QC1-W001`, stale comment drift
   `R-V1106P2-001`, ConnectDaemonFormChrome prop drift `R-V1107QC1-W002`, and UX
   polish suggestions) plus a UI/UX aesthetics pass. Architect-scoped residual
   selection at plan-lock.

### Locked direction rationale (autonomous)

| Candidate cluster | Evidence | Rank |
|-------------------|----------|------|
| **Canvas command palette** | Zero `CommandPalette`/`cmdk` usage in `apps/web/src` or `apps/design-studio/src` (grep); named "full shared command palette" in V1.108 compass `## Roadmap Position` next; "Shared command palette" in V1.109 compass Non-Goals (deferred); V1.110 retrospective next recommendation. Highest discoverability ROI; greenfield so file-disjoint and parallel-safe. | **1** (locked) |
| **Sidebar canvas IA** | `apps/web/src/components/layout/sidebar.tsx` ships flat two-tab (Creator + Orchestrator) IA from V1.94; V1.108 + V1.109 compasses name "Sidebar canvas IA" as next; nesting canvas surfaces under Work context is the documented trajectory. File-local to `sidebar.tsx` + tests. | **2** (locked) |
| **Design Studio canvas gallery** | `apps/design-studio/src/pages/surfaces.tsx` already has a Canvas page (`/surfaces/canvas`) with `CanvasSurfacesFixtures` (V1.108 P1) mirroring **Outline** chrome. The real gap: Strategy + World KB chrome are NOT mirrored. V1.108 compass long-term Done: "previewable in Design Studio under studio-first". P2 re-scoped to **expand the existing fixture**. Daemon-independent, so parallel-safe with P0/P1. | **3** (locked) |
| **Frontend consolidation sweep** | 91 open residuals; frontend-owned cluster deferred post-V1.106/107/108/109/110 (toast dup `R-V1106P0-001`, chrome mirror `R-V1109-P0-QC1-W001`, prop drift `R-V1107QC1-W002`, stale comments `R-V1106P2-001`/`R-V1100P1QC2-*`, UX polish suggestions). User direction explicitly names 遗留问题 + UI bug + UI/UX美观. | **4** (locked) |
| Deferred: graph layout engine (dagre/elk) | Named in V1.109 next; lower priority (deterministic grid still functional); defer to next iteration. | deferred |
| Deferred: Strategy onConnect for inner-graph groups | Named in V1.109 next; narrower capability; defer. | deferred |

**Scale budget:** L → 4 business plans (within 3–4 cap). Harness process
(Review chain / QC / QA / compound / close / PR) excluded from count per
autonomous-direction-lock § Scale budget. The user explicitly chose L and a
broad 5-concern direction; 4 plans honors the budget while covering every named
concern with a clean file-disjoint boundary.

### Must / Stretch integrity

| Tier | Plans | May defer? | Iteration incomplete if missing? |
|------|-------|------------|----------------------------------|
| **Must** | **P0** command palette; **P1** sidebar IA; **P2** design-studio canvas gallery | No | **Yes** (any missing) |
| **Stretch** | **P3** consolidation sweep | Yes — if architect scopes P3's residual list and it overflows the shippable cap, P3 may defer to roadmap-next without blocking iteration completion | No |

> **P3 Stretch rationale (product-manager seat — see Review & Edit chain note
> below):** The iteration's headline is canvas navigation + discoverability
> (P0/P1/P2), which is the triple-confirmed roadmap direction. Consolidation
> (P3) is high-value drift-paydown but must not block the canvas-navigation
> delivery: if the architect scopes P3's residual list and it overflows the
> shippable cap, P3 defers to roadmap-next and the iteration still completes at
> the 3 Must plans (within the L budget). This de-risks the iteration while
> still honoring the user's "遗留问题" direction — P3 ships if it fits.

> P3's exact residual list is architect-scoped at plan-lock.

## Product Story

**Who:** Authors who live in the Control Room canvas daily, and contributors who
maintain the design system in Design Studio.

**Problem:** After V1.108–V1.109 deepened the canvas graphs (Scene/Beat, Strategy
edges, viewport reliability) and V1.110 polished the launch flow, the canvas is
capable but **not discoverable**: there is no ⌘K palette to jump to a surface or
fire a canvas action, the sidebar still uses the flat V1.94 two-tab IA while the
canvas has grown to three rich surfaces, Design Studio's canvas fixture only mirrors the Outline surface
(studio-first stops at Outline-only chrome), and a long tail of frontend
residuals (duplicate toast impl, static chrome mirrors, stale comments, prop
drift) keeps accumulating drift risk.

**Narrative:** Land a keyboard-first command palette so the canvas is one
keystroke away, restructure the sidebar to nest canvas surfaces under Work
context, bring canvas surfaces into Design Studio so the design system covers the
real product, and sweep the frontend residual tail so the codebase the next
iteration inherits is clean.

**Iteration complete when:** All three Must plans (P0/P1/P2) Done (or non-blocking residuals
documented); P0 FB-CP-* accepted; P1 FB-SB-* accepted; P2 FB-DS-* accepted; P3
FB-CS-* accepted (Stretch — may defer to roadmap-next if architect-scoped list overflows).

### User-visible outcomes by feedback ID

**P0 — Command Palette (FB-CP-*)** — primary spec: `v1.111/specs/canvas-command-palette.md`

| ID | What the author sees |
|----|----------------------|
| FB-CP-000 | ⌘K / Ctrl+K opens a command palette overlay from anywhere in the Control Room |
| FB-CP-001 | Palette lists canvas navigation actions (go to Strategy / Outline / World KB), node-creation actions, and surface switches; fuzzy-filtered by typing |
| FB-CP-002 | Arrow + Enter keyboard navigation; Escape closes; focus returns to caller |
| FB-CP-003 | Palette is registered via an action registry so P1/P2/future surfaces add commands without touching the palette component |
| FB-CP-004 | Light + dark token consumption verified; a11y (role=dialog, aria-combobox) verified |

**P1 — Sidebar Canvas IA (FB-SB-*)** — primary spec: `v1.111/specs/sidebar-canvas-ia.md`

| ID | What the author sees |
|----|----------------------|
| FB-SB-000 | Sidebar nests canvas surfaces (Strategy / Outline+Timeline / World KB) under the active Work context |
| FB-SB-001 | Selecting a canvas surface navigates to it with the Work context preserved |
| FB-SB-002 | Active surface is highlighted; collapsed/expanded state is consistent across navigation |
| FB-SB-003 | Mobile/narrow viewport behavior preserved (no regression to V1.94 responsive rules) |

**P2 — Design Studio Canvas Gallery (FB-DS-*)** — primary spec: `v1.111/specs/design-studio-canvas-gallery.md`

| ID | What the author/contributor sees |
|----|----------------------------------|
| FB-DS-000 | Design Studio Canvas page already exists at `/surfaces/canvas` (V1.108 P1); P2 verifies — no new route or nav entry |
| FB-DS-001 | Existing `CanvasSurfacesFixtures` expanded with Strategy + World KB surface chrome sections (Outline chrome already present); read-only, daemon-independent |
| FB-DS-002 | Light + dark preview; DESIGN tokens consumed (no hardcoded values) |
| FB-DS-003 | `apps/design-studio/AGENTS.md` documents the canvas page (boundary note already present; P2 adds a one-line all-three-surfaces note) |

**P3 — Frontend Consolidation & UI Polish Sweep (FB-CS-*)** — primary spec: `v1.111/specs/frontend-consolidation-sweep.md`

| ID | What the author/contributor sees |
|----|----------------------------------|
| FB-CS-000 | Toast duplication dismissed with reason (already consolidated — `use-toast.tsx` is a 7-line re-export barrel from `@42ch/nexus-ui`; sole impl is `packages/nexus-ui/src/components/toast.tsx`); `R-V1106P0-001` closed with disposition |
| FB-CS-001 | Stale comment/copy drift swept (closes `R-V1106P2-001` and siblings) |
| FB-CS-002 | ConnectDaemonFormChrome prop contract reconciled or documented (closes/dispositions `R-V1107QC1-W002`) |
| FB-CS-003 | Architect-scoped UX polish + UI/UX aesthetics pass applied |
| FB-CS-004 | All closed residuals archived to `archived/residuals/<plan-id>.json`; `status.json` `residual_findings` updated |

## Plans

| plan_id | Name | Status | Notes |
|---------|------|--------|-------|
| 2026-07-12-v1.111-canvas-command-palette | P0 — Shared Canvas Command Palette (⌘K) | Done | SDD T1-T5; QC tri Approve w/ residuals (2 low); QA Pass; merged to iteration/v1.111 |
| 2026-07-12-v1.111-sidebar-canvas-ia | P1 — Sidebar Canvas IA | Done | SDD T1-T4; QC tri Approve w/ residual (1 low, /worlds picker); QA Pass; merged to iteration/v1.111 |
| 2026-07-12-v1.111-design-studio-canvas-gallery | P2 — Design Studio Canvas Surface Gallery | Done | SDD T1-T5; QC tri Approve (qc2 degraded PM-authored); QA Pass; expanded fixture with Strategy+World KB chrome; merged to iteration/v1.111 |
| 2026-07-12-v1.111-frontend-consolidation-sweep | P3 — Frontend Consolidation & UI Polish Sweep (Stretch) | Done | SDD T1-T5; QC tri Approve w/ residuals (2 defer); QA Pass; 8 historical residuals archived; merged to iteration/v1.111 |

Status values: `Todo` | `InProgress` | `InReview` | `Done` | `Blocked`

## Milestones

| Milestone | Target date | Status |
|-----------|-------------|--------|
| Spec freeze (Review & Edit chain) | 2026-07-12 | pending |
| P0 dev complete | 2026-07-12 | pending |
| P1 dev complete | 2026-07-12 | pending |
| P2 dev complete | 2026-07-12 | pending |
| P3 dev complete | 2026-07-12 | done |
| QC complete | 2026-07-12 | pending |
| Iteration close | 2026-07-12 | pending |

## Acceptance Criteria

### P0 — Command Palette (FB-CP-000..004)

- [ ] FB-CP-000: ⌘K/Ctrl+K opens palette overlay globally in Control Room
- [ ] FB-CP-001: Action registry feeds palette (canvas nav + node create + surface switch); fuzzy filter
- [ ] FB-CP-002: Full keyboard navigation (Arrow/Enter/Escape); focus restoration
- [ ] FB-CP-003: Action registry is extensible (P1/P2 add commands without editing palette component)
- [ ] FB-CP-004: Light + dark tokens; a11y (role=dialog, aria-combobox) verified
- [ ] `wire_contracts_changed: false` (frontend-only)

### P1 — Sidebar Canvas IA (FB-SB-000..003)

- [ ] FB-SB-000: Sidebar nests canvas surfaces under active Work context
- [ ] FB-SB-001: Surface selection navigates with Work context preserved
- [ ] FB-SB-002: Active-surface highlight + collapse state consistent
- [ ] FB-SB-003: Narrow viewport behavior preserved (no V1.94 responsive regression)
- [ ] `wire_contracts_changed: false` (frontend-only)

### P2 — Design Studio Canvas Gallery (FB-DS-000..003)

- [ ] FB-DS-000: Design Studio Canvas page already exists at `/surfaces/canvas` (V1.108 P1) — verified, no new route/nav work
- [ ] FB-DS-001: Existing `CanvasSurfacesFixtures` expanded with Strategy + World KB surface chrome sections (Outline already present); daemon-independent
- [ ] FB-DS-002: Light + dark preview; DESIGN tokens only (no hardcoded values)
- [ ] FB-DS-003: `apps/design-studio/AGENTS.md` documents the canvas page (one-line all-three-surfaces note added)
- [ ] `wire_contracts_changed: false` (frontend-only)

### P3 — Frontend Consolidation & UI Polish Sweep (FB-CS-000..004)

- [ ] FB-CS-000: Toast duplication dismissed with reason (already consolidated — `use-toast.tsx` is a re-export barrel; `R-V1106P0-001` closed with disposition)
- [ ] FB-CS-001: Stale comment/copy drift swept (closes `R-V1106P2-001` + siblings — architect-scoped)
- [ ] FB-CS-002: ConnectDaemonFormChrome prop contract reconciled/documented (dispositions `R-V1107QC1-W002`)
- [ ] FB-CS-003: UX polish + UI/UX aesthetics pass (architect-scoped)
- [ ] FB-CS-004: Closed residuals archived; `status.json` updated
- [ ] `wire_contracts_changed: false` (frontend-only)

## Non-Goals

- Graph layout engine (dagre/elk) integration — deferred to next iteration (deterministic grid remains)
- Strategy `onConnect` for inner-graph groups — deferred (narrower capability)
- Fifth canvas domain surface (Manuscript/Findings/Memory as graphs)
- Scene/Beat **write** operations (still read-projection; deferred until daemon models scenes/beats)
- Full keyboard **reconnect** for Strategy edges (V1.109 deferred; not in scope)
- Backend/wire contract changes — all four plans are `wire_contracts_changed: false` (frontend-only)
- Platform / cloud sync
- A full design-system redesign — P3 is a polish/consolidation pass, not a token overhaul
- Mass archival of V1.91–V1.105 tech-debt residuals (a separate spec-hygiene pass; P3 closes only the frontend-owned cluster)

## Roadmap Position

- **Current iteration (V1.111):** **Delivered.** Canvas discoverability (⌘K command palette + sidebar Canvas IA), Design Studio three-surface chrome gallery (Outline + Strategy + World KB), and frontend consolidation sweep (8 historical residuals archived + 1 token fix). Three Must + one Stretch; all `wire: false`.
- **Next iteration:** Graph layout engine (dagre/elk); Strategy inner-graph group `onConnect`; `/worlds` picker route (`R-V1111P1-WORLDS-PICKER`); mass tech-debt archival / canonical-severity hygiene (`R-V1111P3QC1-W001`). Trigger: V1.111 ships green. Owner: `@project-manager` + `@architect`.
- **Long-term Done:** Three canvas surfaces are spatially complete, fully editable on-graph, **discoverable via ⌘K palette and nested sidebar IA**, **performant via auto-layout**, and **previewable in Design Studio under studio-first** — with a clean frontend codebase.

## Delivery Branch Policy

> Mirror of frontmatter; kept in sync with `{HARNESS_DIR}/status.json` `metadata`.

| Field | Value |
|-------|-------|
| `iteration_base_branch` | `main` |
| `spec_integration_branch` | `iteration/v1.111` |
| `target_branch` | `main` |

Branch resolve evidence (autonomous): `status.json` root `metadata.iteration_base_branch = main`,
`metadata.target_branch = main`, `metadata.latest_ship.iteration = V1.110` (PR #140 merged to main).
V1.106–V1.110 all shipped to `main` via PR — this is documented delivery policy, not a silent
`main`/`master` default. `spec_integration_branch = iteration/v1.111` (does not yet exist; created at
Phase 1 §6).

## Architecture Locks (architect — cross-plan contracts, 2026-07-12)

> Code-evidenced. Per-plan locks live in each plan's `## Architecture locks`;
> this section captures the cross-plan contracts that govern parallel execution.

### Parallel-safety verdict — file-disjoint, P0/P1/P2 safe parallel; P3 trailing

| Plan | Owns (writes) | Shared/hot files | Parallel-safe? |
|------|---------------|------------------|----------------|
| **P0** | new `lib/canvas/command-registry.ts`, new `lib/use-hotkey.ts`, new `components/command-palette.tsx`, edits `root-layout.tsx` (mount palette + ⌘K) | `root-layout.tsx` — **P0-exclusive** (P1/P2/P3 do not touch it) | ✅ |
| **P1** | `components/layout/sidebar.tsx` + `sidebar.test.tsx`; consumes P0 registry contract | none shared with P0 files; **contract dependency** on P0 `command-registry.ts` (P1 T5 imports it → P0 T1 lands first) | ✅ (serial on T1→T5 only) |
| **P2** | `apps/design-studio/src/fixtures/canvas-surfaces-fixtures.tsx` + `pages/surfaces.tsx` (copy) + `apps/design-studio/AGENTS.md` (one line) | fully disjoint from `apps/web` | ✅ fully disjoint |
| **P3** | scattered: `connect-daemon-form.tsx`, `check-ui-guardrails.sh`, `@42ch/nexus-ui/AGENTS.md`, token audit on P0/P1 output | **P3 T4 (UX audit) depends on P0/P1 output** → lands last | ⚠️ trailing |

**Verdict:** P0 + P1 + P2 are **file-disjoint and parallel-safe** on one
integration branch (`iteration/v1.111`). P1's only serial seam is importing P0's
registry contract (P0 T1 before P1 T5). **P3 lands last** (T4 UX audit consumes
P0/P1 surfaces). One hot file to coordinate: **`root-layout.tsx`** — P0-exclusive
this iteration (no other plan edits it), so no contention.

### Key cross-plan contracts

- **P0 action registry** (`lib/canvas/command-registry.ts`) is the one cross-plan
  contract: P1 consumes `useRegisterCommand` (sidebar surface commands). Shape
  locked in P0 plan. P2 does NOT consume it (Studio is daemon-independent, no
  runtime command wiring).
- **P2 import boundary** — NO RF dep into Studio; the existing
  `CanvasSurfacesFixtures` static-mirror pattern is the locked approach. P2
  extends, does not replace. Enforced by `tooling/check-ui-guardrails.sh`.
- **Toast canonical direction** — sole impl is
  `packages/nexus-ui/src/components/toast.tsx`; App-local `use-toast.tsx` is a
  re-export barrel. P3 dismisses `R-V1106P0-001` with reason (already
  consolidated); no 40+ call-site migration this iteration.
- **P2 scope correction** — the Studio canvas gallery **already exists**
  (`/surfaces/canvas` + `CanvasSurfacesFixtures`, V1.108 P1). P2 is re-scoped
  from "add a page" to "expand the existing fixture with Strategy + World KB
  chrome" (Outline chrome already present). Nav/route already exist.

## Risk Register

> Architect-updated 2026-07-12 (P2 + P3 risks revised against code evidence).

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| P0 command palette grows beyond one plan (fuzzy search lib, action registry surface) | Low | Medium | Action-registry contract locked (module store + `useRegisterCommand`); filter = substring + cheap rank, no fuzzy lib; palette at top-level `components/` not `canvas/` |
| P1 sidebar IA regresses V1.94 responsive rules | Low | Low | Responsive rules live in `root-layout.tsx` (P1 does NOT touch it); `sidebar.test.tsx` IA assertions (7) preserved; active-surface via route-pattern matching |
| P2 Design Studio canvas fixture drifts from App surfaces | Low | Low | P2 EXTENDS the existing V1.108 fixture (proven pattern); Studio-local hand-mirror; no RF dep (boundary enforced by guardrails) |
| P3 residual scope balloons | Low | Low | Architect capped P3 at 9 residuals; toast `R-V1106P0-001` already consolidated (dismiss with reason); UX audit scoped to P0/P1 surfaces only |
| Cross-plan shared-file contention on sidebar/layout | Low | Medium | File-disjoint by design: P0 owns `root-layout.tsx` (exclusive); P1 owns `sidebar.tsx`; P2 owns design-studio; P3 owns scattered residual files. P1↔P0 serial only on registry contract (P0 T1 → P1 T5) |

## Iteration workspace

| Path | Purpose |
|------|---------|
| `v1.111/specs/canvas-command-palette.md` | P0 primary spec (architect refines) |
| `v1.111/specs/sidebar-canvas-ia.md` | P1 primary spec (architect refines) |
| `v1.111/specs/design-studio-canvas-gallery.md` | P2 primary spec (architect refines) |
| `v1.111/specs/frontend-consolidation-sweep.md` | P3 primary spec (architect scopes residual list) |
| `v1.111/guides/` | Exploration / process notes |

## Quality Gate Summary

> Filled at iteration-close. Human summary only; per-plan gate details stay in each main plan, and open residual SSOT stays in `{HARNESS_DIR}/status.json`.

| plan_id | QC decision | QA gate | Residuals | Durable summary |
|---------|-------------|---------|-----------|-----------------|
| 2026-07-12-v1.111-canvas-command-palette | Approve with residuals (tri) | mandatory — Pass | R-V1111P0QC2-W001, R-V1111P0QC2-W002 (low, defer) | `{PLAN_DIR}/2026-07-12-v1.111-canvas-command-palette.md#review-gate-summary` |
| 2026-07-12-v1.111-sidebar-canvas-ia | Approve with residuals (tri) | mandatory — Pass | R-V1111P1-WORLDS-PICKER (low, defer) | `{PLAN_DIR}/2026-07-12-v1.111-sidebar-canvas-ia.md#review-gate-summary` |
| 2026-07-12-v1.111-design-studio-canvas-gallery | Approve (qc2 degraded PM-authored) | mandatory — Pass | none blocking | `{PLAN_DIR}/2026-07-12-v1.111-design-studio-canvas-gallery.md#review-gate-summary` |
| 2026-07-12-v1.111-frontend-consolidation-sweep | Approve with residuals (qc1 W pre-existing → residual) | mandatory — Pass | R-V1111P3QC1-W001, R-V1111P3QC1-S001 (defer) | `{PLAN_DIR}/2026-07-12-v1.111-frontend-consolidation-sweep.md#review-gate-summary` |

Notes:

- Raw review bundle: `{SDD_DIR}/review/` (ephemeral; do not rely on it after Done).
- Open residual SSOT: `{HARNESS_DIR}/status.json` root `residual_findings[<plan-id>]`.
- All four plans: SDD + QC tri + QA Pass; `wire_contracts_changed: false`.

## Compound Round Summary

- 结晶文档数：1 新增 (`architecture-patterns/action-registry-command-palette.md`) + 1 更新 (`architecture-patterns/canvas-surface-implementation-pattern.md` layer 11 discoverability)
- 新增 CONCEPTS.md 条目：0（command palette / action registry 为通用术语）
- 触发 compound-refresh：否
- **Workspace 盘点** (`v1.111/specs/`): 4 篇 iteration specs → **Keep snapshot**（已被 plans 消费；迭代级 bugfix/feature specs，不提升为长期 `{SPECS_DIR}/` 冻结契约）。`guides/` 空。

## Iteration Retrospective (minimal)

- 做得好的：autonomous direction lock 与 V1.108–V1.110 roadmap 对齐；P0 action registry 契约清晰（module store 拒绝 context）；architect 在 Phase 1 纠正 P2「无 canvas 页」事实错误；P3 Stretch 不阻塞 Must 交付；8 条历史 residual 归档 + tech_debt_summary 刷新。
- 可改进的：`qc-specialist-2` 空返回仍出现（P2 degraded PM-authored）；`status.json` 体积仍 >20KB（spec-hygiene 仍欠）；P1 与 P0 的 surface-target 逻辑有轻微重复（consolidate 候选）。
- 下迭代建议：`/worlds` picker（解锁 World KB 无 context 导航）；dagre/elk 布局；canonical residual severity 规范化（`suggestion` → enum）；可选：palette 与 sidebar 共享 `resolveCanvasNavTarget`。
