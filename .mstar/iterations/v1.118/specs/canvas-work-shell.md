# P2 Spec — Canvas-first work shell

**Status:** Draft (V1.118) — product specify + clarify **done**; architect **plan done** (2026-07-15)  
**Document class:** Draft overlay  
**Iteration compass:** [delivery-compass.md](../delivery-compass.md)  
**Depends on:** [creation-peer-groups.md](creation-peer-groups.md) (P1)  
**Promote target:** fold into [web-ui.md](../../../specs/web-ui.md) at iteration P5

## Problem statement

V1.117 enter-work UX (`isDrillIn` when `workId` is in the route):

- Hides Creator | Orchestrator tabs
- Replaces sidebar with Back to all / Outline / Body only
- Makes Canvas feel like a drill-down detail, not the core writing surface

Product direction: **entering a work is entering the Canvas room** — main stage for Outline, Works switching on the right, Body read-only still available.

## Target users

| Persona | Job to be done |
| --- | --- |
| Author writing in a work | See Outline canvas immediately; steer structure visually |
| Author switching works | Pick another work from right rail without "back out" drill-in |
| Author reviewing body | Open read-only chapters from within the shell |

## Product rule (normative)

**Enter work → Canvas-first shell.**

```
┌─────────────┬──────────────────────────┬─────────────────┐
│ Left sidebar│ Main (default: Outline   │ Right rail      │
│ (tabs stay) │ canvas for selected work)│ Works list +    │
│ Creator /   │                          │ preview of      │
│ Orchestrator│                          │ selected work   │
└─────────────┴──────────────────────────┴─────────────────┘
```

| Region | Behavior |
| --- | --- |
| **Left sidebar** | Creator \| Orchestrator tabs **remain visible**; P1 peer groups apply when on Creator tab in list context; inside work, no V1.117 drill-in skeleton |
| **Main** | Default route `/works/:workId/outline` — Outline canvas for that work |
| **Right rail** | Scrollable Works list (from same data as P1 Works group) + preview panel for selected work |
| **Body** | Read-only chapters via existing `/works/:workId/chapters…` — reachable from shell (nav link, command palette, or canvas chrome — implementer choice within author-observable AC) |

**Retired as primary pattern:** whole-left `isDrillIn` hiding tabs and showing only Back/Outline/Body.

## Architecture contract (normative — architect locked)

### Shell layout model

`RootLayout` (sidebar + header + banner + scrollable `<main>`) is unchanged. Work routes nest **`WorkShellLayout`** inside `<main>`:

```
RootLayout
├── Sidebar (tabs visible; P1 peer groups; NO drill-in)
└── main (scroll)
    └── WorkShellLayout  ← only on /works/:workId/*
        ├── WorkMain (flex-1, min-w-0) → <Outlet /> (Outline, Chapters, …)
        └── WorkRail (280px, lg+) | drawer (<lg)
```

- **Default route:** `/works/:workId` → redirect `/works/:workId/outline`.
- **Width:** Work shell **opts out** of RootLayout inner `max-w-[1200px]` wrapper for canvas routes (implement via route-level layout wrapper or conditional class on main child).
- **Body:** `/works/:workId/chapters` renders inside same shell; rail persists.

### WorkRail (right rail)

| Region | Content (MVP) |
| --- | --- |
| **List** | Same `useWorks({ limit: 12 })` as P1; current work highlighted |
| **Preview** | Metadata only: title, status badge, work profile / preset label, `updated_at` relative — **no** outline snippet or findings |
| **Switch** | Click row → `navigate(/works/:id/outline)` without full reload |

### Responsive

| Breakpoint | Behavior |
| --- | --- |
| `lg`+ (≥961px) | Fixed 280px right rail, always visible |
| `<lg` | Rail hidden; **end-sheet drawer** opened from work-shell header control; focus trap + dismiss |

No drill-in fallback as primary on narrow viewports.

### `isDrillIn` / `drillInItems`

- P2 T3 **removes** `isDrillIn` derivation and `drillInItems` pass-through in `sidebar.tsx`.
- `ShellSidebarChrome.drillInItems` prop: mark `@deprecated` in P2 if not deleted; **must not** be used as enter-work UX.

**Depends on P1 merge** before P2 sidebar edits.

`wire_contracts_changed: false`.

## Scope boundary

| In scope (P2) | Out of scope |
| --- | --- |
| Layout chrome for `/works/:workId/*` | New canvas surface types beyond existing Outline default |
| Outline as default main surface | World KB as default main (Outline wins) |
| Right-rail Works list + preview | Rich manuscript snippet preview (deferred) |
| Retire primary `isDrillIn` path | Orchestration tab IA redesign |
| Locales + tests for new chrome | Strategy canvas in main slot from rail |

## Acceptance criteria

| ID | Criterion | Verification (author-observable) | Priority |
| --- | --- | --- | --- |
| AC-P2-1 | Selecting a work (from Works group or list) opens **Canvas-first shell** on `/works/:workId/outline` | Click work → URL has `outline`; main pane shows Outline canvas | Must |
| AC-P2-2 | Default main surface is **Outline** for the selected work | Land on work without subpath → redirect or default to outline | Must |
| AC-P2-3 | **Right rail** shows Works list with current work indicated + preview for selected work | Visual: list + preview panel populated | Must |
| AC-P2-4 | **Body read-only** reachable at `/works/:workId/chapters` (and sub-routes) from within shell | Navigate to chapters; read-only body renders | Must |
| AC-P2-5 | Creator \| Orchestrator tabs **visible** inside work — not replaced by drill-in-only nav | Inside work: both tabs present; no sole Back/Outline/Body left column | Must |
| AC-P2-6 | V1.117 `isDrillIn` drill-in skeleton is **not** the primary enter-work UX | Inside work: `drillInItems` not passed as default nav; Creator \| Orchestrator tabs remain | Must |
| AC-P2-7 | Outside work context (`/works`, `/worlds`, `/memory`), P1 peer groups still work | Regression from P1 ACs | Must |
| AC-P2-8 | Switching works from right rail updates main Outline context without full app reset | Select different work in rail → outline updates | Must |
| AC-P2-9 | Studio-first tokens for any new shell chrome per DESIGN.md | New components use design tokens / studio path | Should |

## Edge cases

| Case | Expected behavior |
| --- | --- |
| Direct URL `/works/:id/chapters` | Shell wraps body; right rail still present |
| Single work in library | Right rail list length 1; still shown |
| Mobile / narrow viewport | Architect defines collapse — product minimum: shell degrades gracefully, no drill-in fallback as primary |
| World-scoped routes | Unchanged — P2 scoped to **work** routes only |

## Non-goals

- Rich right-rail preview (findings density, manuscript excerpts) → next iteration
- Alternate main surfaces (Strategy, World KB) selectable from rail → later
- Changing Outline editor capabilities
- Replacing command palette work navigation

## Deferred scope (tracked)

| Item | Trigger | Owner |
| --- | --- | --- |
| Richer right-rail preview | Post-V1.118 UX iteration | PM |
| Strategy / World KB as alternate main from rail | Canvas trajectory backlog | PM |

## Code anchors

- `apps/web/src/components/layout/work-shell-layout.tsx` — **new** (T1)
- `apps/web/src/components/layout/work-rail.tsx` — **new** (T1)
- `apps/web/src/components/layout/sidebar.tsx` — remove `isDrillIn` / `drillInItems` (T3, post-P1)
- `apps/web/src/App.tsx` — nested `works/:workId` routes
- `apps/web/src/components/layout/presentational/shell-sidebar-chrome.tsx` — deprecate `drillInItems` if kept

## Open questions for architect

~~All resolved in § Architecture contract.~~

1. ~~Right-rail preview MVP~~ → metadata card only (`WorkSummary` fields).
2. ~~Responsive collapse~~ → `<lg` end-sheet drawer.
3. ~~`drillInItems` removal vs deprecation~~ → remove usage in P2; prop deprecated one release if needed.
