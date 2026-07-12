# P1 — Sidebar Canvas IA Spec

> Iteration: V1.111. Status: **PM draft — architect refines at §5.2**. Primary
> consumer: plan `2026-07-12-v1.111-sidebar-canvas-ia.md`.

## Problem

The Control Room sidebar (`apps/web/src/components/layout/sidebar.tsx`) ships the
V1.94 flat two-tab IA (Creator + Orchestrator). The canvas has since grown to
three rich surfaces (Strategy, Outline+Timeline, World KB) with V1.108–V1.109
depth (Scene/Beat, Strategy edges, viewport reliability). The flat IA no longer
reflects the canvas surface structure — authors reach surfaces via top-level
flat nav without Work-context nesting. The V1.108 and V1.109 compasses name
"Sidebar canvas IA" as next.

## User value

- **FB-SB-000** — Sidebar nests canvas surfaces (Strategy / Outline+Timeline /
  World KB) under the active Work context.
- **FB-SB-001** — Selecting a canvas surface navigates to it with the Work
  context preserved.
- **FB-SB-002** — Active surface is highlighted; collapsed/expanded state is
  consistent across navigation.
- **FB-SB-003** — Mobile/narrow viewport behavior preserved (no regression to
  V1.94 responsive rules).

## Open questions for architect — RESOLVED (2026-07-12)

> See plan `## Architecture locks` for full evidence. Verdicts below.

1. **Nesting model — reframed.** The three canvas surfaces use **different route
   params** (Outline `/works/:workId/outline`, World KB `/worlds/:worldId/kb`,
   Strategy `/strategies/:presetId`) — they CANNOT all nest under one "active
   Work" ID. **Verdict:** sidebar nests a **"Canvas" nav group** (existing
   `ShellNavGroup` disclosure) exposing the three surface entry points;
   **active-surface highlight derives from route-pattern matching**, not a single
   Work param. Non-canvas items stay put (Memory under Creator; Settings footer
   utility; Findings reachable via routes). Long-term "active Work name inline"
   (requires sidebar data-fetch from route params) deferred to roadmap-next.
2. **Coordinator with P0** — **Resolved: P1 consumes P0's `useRegisterCommand`
   hook** (one-way). Parallel-safe/file-disjoint (P0 = new files + root-layout;
   P1 = sidebar.tsx). Serial dependency: P1 T5 imports P0 registry → P0 T1
   lands first.
3. **State persistence** — **Resolved: in-session only.** Collapse state via
   the chrome's existing internal `useState(group.defaultOpen ?? true)`. No
   persistence (matches V1.109). No new state.
4. **Responsive contract — clarified.** The "V1.94 responsive rules" live in
   `root-layout.tsx` (sidebar hidden below `lg`), which P1 does NOT touch —
   narrow-viewport behavior structurally preserved. `sidebar.test.tsx` tests IA
   structure (7 assertions), not viewport; P1 preserves all 7, updating
   link-label assertions with documented reason if the Canvas group changes
   visible names.

## Non-goals

- Top-level IA redesign beyond canvas surface nesting.
- Customizable sidebar / drag-reorder.
- Cross-session collapse persistence (unless trivial).
- Backend/wire changes — `wire_contracts_changed: false`.

## DoD

All FB-SB-000..003 accepted in App; existing `sidebar.test.tsx` responsive
assertions preserved (or intentionally updated with documented reason); P1
registers/consumes P0's action registry cleanly; QC tri Approve; QA gate passed.
