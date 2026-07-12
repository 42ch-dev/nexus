# P2 — Design Studio Canvas Surface Gallery Spec

> Iteration: V1.111. Status: **PM draft — architect refines at §5.2**. Primary
> consumer: plan `2026-07-12-v1.111-design-studio-canvas-gallery.md`.

## Problem

> **Architect correction (2026-07-12):** the gallery **already exists**.
> `SurfacesCanvasPage` at `/surfaces/canvas` (`surfaces.tsx:611`) renders
> `CanvasSurfacesFixtures` (`canvas-surfaces-fixtures.tsx`, 673 lines) — a
> comprehensive V1.108 P1 fixture mirroring the **Outline** surface chrome
> (shell, dot-grid, controls, minimap, Volume/Chapter/Timeline/Scene/Beat node
> matrix, context menus). It is daemon-independent, RF-free, token-driven, and
> already routed/nav'd. The real gap: **Strategy** and **World KB** surface
> chrome are NOT mirrored. P2 is re-scoped to **expand the existing fixture**.

## User value

- **FB-DS-000** — Design Studio Canvas page reachable from studio nav — **already satisfied** (existing `/surfaces/canvas`); P2 verifies.
- **FB-DS-001** — Canvas page renders fixture-driven Strategy / Outline / World KB surface chrome — Outline **already present**; P2 ADDS Strategy + World KB (read-only, daemon-independent).
- **FB-DS-002** — Light + dark preview; DESIGN tokens consumed (no hardcoded values).
- **FB-DS-003** — `apps/design-studio/AGENTS.md` documents the canvas page (boundary note already present; P2 adds a one-line all-three-surfaces note).

## Open questions for architect — RESOLVED (2026-07-12)

> See plan `## Architecture locks` for full evidence. Verdicts below.

1. **Chrome sharing — confirmed (proven pattern).** The existing fixture is the
   template: **static node/edge chrome previews** via hand-authored JSX consuming
   `var(--color-canvas-*)` tokens. **NO RF dep into Studio** — boundary already
   respected and enforced by `tooling/check-ui-guardrails.sh`. P2 follows the
   same hand-mirror for Strategy + World KB.
2. **Fixture source — Studio-local.** No App projection-test import (boundary
   forbids `apps/web/src/components/canvas/**`). P2 hand-mirrors Strategy
   node/edge chrome (from `strategy-nodes.tsx` / `state-machine.ts` visuals) and
   World KB chrome (entity/relationship/anchor visuals) as Studio-local static
   markup, matching the existing Outline mirror discipline.
3. **Page structure — extend existing.** One Canvas page; P2 ADDS a "Strategy
   surface chrome" `FixtureFrame` section and a "World KB surface chrome"
   `FixtureFrame` section to `CanvasSurfacesFixtures`, alongside the existing
   Outline + context-menu sections.
4. **Nav wiring — NO new work.** `/surfaces/canvas` route (`App.tsx:60`) and the
   "Canvas" `SURFACES_SECTIONS` entry already exist. P2 does not touch routes
   or `nav.tsx`.

## Non-goals

- Live React Flow canvas in Studio (RF-coupled import boundary forbids it).
- Interactive canvas editing in Studio (read-only preview only).
- Daemon connectivity (Studio stays daemon-independent).
- Backend/wire changes — `wire_contracts_changed: false`.

## DoD

All FB-DS-000..003 accepted in Studio; no RF dep introduced into Studio (import
boundary preserved); DESIGN tokens only (no hardcoded color/spacing); AGENTS.md
updated; QC tri Approve; QA gate passed.
