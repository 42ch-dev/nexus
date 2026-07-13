# Canvas UX Residual Cluster Closure (V1.115 P1)

> Iteration-scoped product/tech brief for V1.115 P1. Not a normative `{SPECS_DIR}`
> Master — closes a bounded residual cluster so foundation completion includes
> author-visible honesty, not only adapter code.

| Attribute | Value |
| --- | --- |
| **plan_id** | `2026-07-13-v1.115-canvas-ux-residual-cluster` |
| **Tier** | Must |
| **Audience** | Authors (nav + alt-view affordances) + maintainers (RF upgrade resilience) |

## Problem framing

After V1.114, five canvas residuals remain that either **block everyday
navigation** or **hide a silent upgrade risk**:

| Residual | Author-visible? | Failure mode if deferred |
| --- | --- | --- |
| `R-V1108P0QC1-M001` Alt-view no sort | Yes | Chapter / timeline alt lists stay unsortable; spatial canvas is the only navigation order |
| `R-V1111P0QC2-W001` Hotkey `.react-flow` class | Latent | RF upgrade renames internal class → Cmd/Ctrl+K conflict-avoidance silently breaks |
| `R-V1111P1-WORLDS-PICKER` World KB nav disabled | Yes | Sidebar item looks broken when no world is active; no path to pick a world |
| `R-V1108P1QC1-S002` Studio chrome re-mirror | Maintainer / Design Studio | Studio drifts from shipped chrome after V1.114 extract |
| `R-V1109-P0-QC3-W002` `beatParentSceneTitle` O(n) | Perf (fixture-scale today) | Cheap O(1) fix now avoids re-work when real wire volume grows |

This is not a feature wave: it is **foundation hygiene** so the canvas feels
finished and survives the next RF bump.

## User value

| Who | Why they care |
| --- | --- |
| **Authors** | Sortable alt-views for chapter / event lists; World KB nav always leads somewhere honest (picker or empty state), never a dead disabled control |
| **Authors (keyboard)** | Command palette hotkeys keep working inside and outside canvas after RF upgrades |
| **Design Studio consumers** | Gallery chrome matches production node chrome (single extract) |
| **Maintainers** | Residual slate trends down; no RF-internal class dependency in hotkey path |

## Goals

1. Client-side sort on Outline+Timeline alt-view lists where data supports it.
2. Hotkey conflict-avoidance without `.react-flow` CSS-class dependency.
3. World KB sidebar reachable with no active `worldId`.
4. Design Studio consumes shared node-chrome extract (no static re-mirror).
5. Index `beatParentSceneTitle` lookup (O(1) / memoized).

## Non-goals

- Server-side `sort_by` / `sort_order` (F-F1) — client-side only for small lists
- Full world-management CRUD surface — minimal picker or empty state only
- Auto-select first world (surprising; see locked decisions)
- Domain graph mutations from sort controls
- Closing unrelated residual slate items outside this cluster
- New DESIGN.md tokens

## Target state

- Alt-views are navigable lists (sortable where columns exist).
- World KB nav never permanently disabled.
- Hotkey detection uses a stable, public mechanism.
- Design Studio and production share node chrome.
- Named residuals above closed (or accepted with durable tracking + rationale).

## Acceptance criteria (author/maintainer-observable)

| ID | Criterion | How to verify |
| --- | --- | --- |
| **AC-P1-1** | Authors can sort Outline alt-view chapter list by **chapter number** and **volume** (toggle asc/desc) | UI + tests: order changes; sort state ephemeral (not persisted); list still reflects underlying graph |
| **AC-P1-2** | Authors can sort Timeline alt-view event list by **event time** (toggle asc/desc) | Same as AC-P1-1 for events |
| **AC-P1-3** | Cmd/Ctrl+K conflict-avoidance does not depend on `.react-flow` CSS class | Code uses attribute selector (e.g. `[data-command-palette-ignore]`) and/or RF public API; test survives simulated class rename; behavior outside canvas unchanged |
| **AC-P1-4** | World KB sidebar item is focusable and navigates when no world is active | Lands on minimal world list **or** honest empty state (“No world selected — pick one”); never permanently `disabled` |
| **AC-P1-5** | Design Studio canvas fixtures import shared node-chrome extract | No static re-mirror of chrome markup; RF import boundary respected (no `@xyflow/react` pull into Studio presentational extract) |
| **AC-P1-6** | `beatParentSceneTitle` uses memoized Map / equivalent O(1) lookup | Code + test: correct title; stable across renders with same scenes |

## Product decisions (locked this seat)

| Decision | Choice | Rationale |
| --- | --- | --- |
| World KB no-`worldId` path | **Minimal world-list picker** reusing `GET /v1/local/narrative/worlds` when any worlds exist; **honest empty state** when the list is empty | Prefer discoverability over auto-routing to “first world” (surprising context switch) |
| Auto-select first world | **Out of scope** | Violates author control; would look like a feature, not residual close |
| Sort columns | Chapters: **number + volume**; Timeline events: **event time** | Fields already on data; no schema work |
| Sort persistence | **Ephemeral session UI state only** | Non-goal: server sort / saved preferences |
| Cluster Must framing | Entire cluster is Must; if capacity forces depth cut, drop **Studio chrome (S002)** then **O(n) index (W002)** before author-visible items | Author-visible + RF risk outrank Design Studio polish |

## Plan independence note

P1 is **product-independent of P0**: UX residuals do not require Outline to be
on the adapter first. File-level conflict risk exists if both edit
`outline-canvas*` — coordinate at integration merge; do not serialize the plans
for product reasons.

## Architect decisions (Seat 2 — resolved)

| # | Question | Decision | Rationale |
| --- | --- | --- | --- |
| 1 | World list: new minimal route vs reuse existing settings/worlds entry? | **Reuse existing.** Change `resolveCanvasNavTarget('world-kb', {})` from `null` → `/worlds`. Add a minimal `/worlds` route reusing `GET /v1/daemon/narrative/worlds` (already consumed by `soul-section.tsx`). | Smallest IA change: no new endpoint, no new CRUD. The worlds query + `useWorlds` hook already exist. One resolver line + one route. |
| 2 | Hotkey detection: attribute on canvas root vs RF public focus API? | **Attribute on canvas root (`data-command-palette-ignore`).** Drop `.react-flow` from the selector; set the attribute on `CanvasShell` root. | Self-owned attribute is stable across RF upgrades. RF public focus API is heavier, depends on RF internals, and has no stable "canvas focused" method. Attribute is Ladder rung 4 (native feature) over library API. |
| 3 | Studio extract import path: confirm V1.114 presentational boundary? | **V1.114 did NOT ship a shared extract.** T4 must CREATE a `NodeChromeShell` presentational component (no `@xyflow/react`) first, then both App + Design Studio consume it. | The claim that the extract was "shipped in V1.114" is factually wrong — chrome is inline in each RF node component today. The presentational boundary is: props in, JSX out, zero RF types. |

## Spec refs

- `.mstar/specs/canvas-strategy-surface.md`
- `.mstar/specs/web-ui.md`
- `.mstar/iterations/v1.109/specs/` (scene-beat)
- `.mstar/iterations/v1.111/specs/` (command palette, sidebar)
- Residuals: `R-V1108P0QC1-M001`, `R-V1111P0QC2-W001`, `R-V1111P1-WORLDS-PICKER`,
  `R-V1108P1QC1-S002`, `R-V1109-P0-QC3-W002`
