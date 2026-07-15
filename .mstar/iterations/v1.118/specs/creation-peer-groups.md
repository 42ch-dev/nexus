# P1 Spec — Creation peer groups

**Status:** Draft (V1.118) — product specify + clarify **done**; architect **plan done** (2026-07-15)  
**Document class:** Draft overlay  
**Iteration compass:** [delivery-compass.md](../delivery-compass.md)  
**Promote target:** fold into [web-ui.md](../../../specs/web-ui.md) sidebar IA section at iteration P5

## Problem statement

V1.117 Creator tab structure:

1. **Works** group — "All Works" link
2. **Creator** meta-group — mixes Outline (canvas), World KB (canvas), and Memory (plain link)

Authors think in **peer domains**: Works, Worlds, Memories. Outline is a **work-scoped canvas surface**, not a top-level Creation peer. Strategy correctly lives under Orchestrator (V1.117 AC-P2-4) and stays there.

## Target users

| Persona | Job to be done |
| --- | --- |
| Author browsing library | Find a work, world, or memory entry quickly |
| Author switching domains | Jump between works list, worlds picker, memory/SOUL without canvas meta-group confusion |

## Product rule (normative)

**List mode = three parallel peer groups. Outline is not a Creation peer.**

| Group label (en) | Group label (zh-CN) | Contents | Route target |
| --- | --- | --- | --- |
| **Works** | (locale) | **All Works** link + flat work rows (≤12) from Works query | `/works`, `/works/:id` (P1); P2 defaults to `/works/:id/outline` |
| **Worlds** | (locale) | Link to worlds surface | `/worlds` (existing page) |
| **Memories** | (locale) | Link to memory / SOUL surface | `/memory` (existing page) |

**Terminology:** **Creator tab** (shell IA) is unchanged — only the **Creator meta-group** (Outline + World KB + Memory mix) is removed. **Profile** is the author-facing label for the active creator identity (`active_creator_id` in config/API).

## Scope boundary

| In scope (P1) | Deferred to P2 |
| --- | --- |
| Creator tab groups in list mode (`/works`, `/worlds`, `/memory` without work-shell chrome) | Canvas-first layout on `/works/:workId/*` |
| Remove Creator meta-group + canvas items from list-mode sidebar | Retire `isDrillIn` drill-in skeleton |
| Works entries from existing Works data source | Right-rail Works list + preview |
| en + zh-CN `shell.json` keys + sidebar tests | Body entry chrome inside work shell |

**Overlap guard:** P1 edits `sidebar.tsx` group definitions only for **non-work-shell** routes. P1 **merges before P2**; P2 rebases and owns `isDrillIn` removal.

## Architecture contract (normative — architect locked)

### Works group shape

```
Works                    ← group label (nav.works)
  All Works              → /works
  <Work title 1>        → /works/:workId
  <Work title 2>        → /works/:workId
  … (up to 12 rows from useWorks({ limit: 12 }))
Worlds                   ← group label
  Worlds                 → /worlds  (single link; label may equal group)
Memories                 ← group label
  Memories               → /memory
```

- Data source: existing `useWorks` / `listWorks` — no new API.
- Zero works: show **All Works** only; no error state.
- **No** `CANVAS_ITEMS` (Outline, World KB) in Creation list-mode groups.

### Active highlight (pre-P2 interim)

On `/works/:workId/*` before P2 ships, V1.117 `isDrillIn` highlight may remain. P1 **must not** remove drill-in. After P2, peer groups show in sidebar with standard prefix highlight.

### Locale keys

| Key | en (example) |
| --- | --- |
| `nav.works` | Works |
| `nav.worlds` | Worlds |
| `nav.memories` | Memories |
| `nav.allWorks` | All Works (retain) |

`wire_contracts_changed: false`.

## Acceptance criteria

| ID | Criterion | Verification (author-observable) | Priority |
| --- | --- | --- | --- |
| AC-P1-1 | Creator tab shows **three** peer group labels: Works, Worlds, Memories | Open app → Creator tab → visually count three group headings | Must |
| AC-P1-2 | No group labeled "Creator" (or equivalent) mixing Outline + World KB + Memory | Absence of V1.117 meta-group; grep `nav.creator` as **group** label removed or repurposed | Must |
| AC-P1-3 | **Worlds** group navigates to existing `/worlds` surface | Click Worlds entry → worlds page loads | Must |
| AC-P1-4 | **Memories** group navigates to existing `/memory` / SOUL surface | Click Memories entry → memory page loads | Must |
| AC-P1-5 | **Works** group lists works from existing Works query (not only "All Works" stub if data exists) | With ≥1 work, entries visible under Works group | Must |
| AC-P1-6 | Orchestrator tab unchanged: Strategy remains under Orchestrator, not Creation | Switch to Orchestrator → `/strategies` link present | Must |
| AC-P1-7 | en + zh-CN locale keys for new group labels; sidebar unit tests updated | `pnpm --filter web test` sidebar tests; both locale files have keys | Must |
| AC-P1-8 | List-mode behavior unchanged on Orchestrator tab and footer Profiles | Regression smoke | Must |

## Edge cases

| Case | Expected behavior |
| --- | --- |
| Zero works | Works group shows All Works or empty state — no crash |
| Active route `/works/:workId/*` before P2 | V1.117 drill-in may still apply until P2 ships (acceptable interim) |
| Canvas resolver items (Outline, World KB) | **Not** in list-mode Creation groups after P1 |
| Memory SOUL sub-routes | Still reachable via `/memory` tree — P1 does not restructure memory IA |

## Non-goals

- New Memories editor or world authoring
- Orchestration tab redesign
- Changing canvas surface types or Outline editor behavior
- i18n for canvas content strings (R-P1-001 class stays deferred unless touched)

## Code anchors

- `apps/web/src/components/layout/sidebar.tsx` — `creatorGroups`
- `apps/web/src/components/layout/presentational/shell-sidebar-chrome.tsx`
- `apps/web/src/locales/en/shell.json`, `zh-CN/shell.json` — `nav.works`, `nav.worlds`, `nav.memories` (or aligned keys)
- `apps/web/src/components/layout/canvas-nav.ts` — list-mode removal of canvas items from Creation groups

## Open questions for architect

~~All resolved in § Architecture contract.~~

1. ~~Works group shape~~ → All Works + flat rows (≤12).
2. ~~Highlight on `/works/:workId` before P2~~ → keep drill-in until P2; P1 does not touch `isDrillIn`.
