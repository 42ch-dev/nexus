# Shell layout + menu IA + status bar (V1.117 P2)

> Iteration-scoped product brief for V1.117 P2. Architect locked (§5.2);
> spec frozen after writing (§5.3).

| Attribute | Value |
| --- | --- |
| **plan_id** | `2026-07-14-v1.117-shell-layout-ia` |
| **Tier** | Must (P2) |
| **Status** | Spec frozen (§5.3) |
| **Audience** | Authors (daily navigation + daemon visibility) |
| **primary plan** | `.mstar/plans/2026-07-14-v1.117-shell-layout-ia.md` |
| **Depends on** | P0 T4 (footer Profile switch activates workspace) for coherent Profile UX |

## Problem framing

The desktop shell should read as a **calm control rail**, not a scrolling page.
Today:

- Sidebar menus scroll away; **Settings** and **Profiles** are not pinned as one
  bottom footer section.
- **Canvas** is incorrectly a top-level nav **group** — canvas is a
  presentation mode inside work context, not a sibling of Creator/Orchestration.
- **Strategy** sits under Creation (Canvas group) but belongs under
  **Orchestration** (编排).
- **World KB** belongs under **Creation** (创作).
- Status bar copy is noisy; authors cannot see **which agent** is active at a
  glance or jump to agent Settings.

## User value

| Who | Why they care |
| --- | --- |
| **Authors (navigation)** | Menu groups match mental model: Orchestration vs Creation; no fake Canvas section. |
| **Authors (focus)** | Sidebar + status bar stay fixed; only content scrolls. |
| **Authors (work context)** | Drill into a work → simple back + 大纲/正文 skeleton instead of full tree. |
| **Authors (desktop)** | Status bar: Daemon Running + agent badge → quick jump to agent Settings. |

## Goals

### Layout

1. Left sidebar **full height**; primary nav menus stretch in the middle.
2. **Settings + Profiles** share one **bottom-aligned** footer section (single
   visual block — grill-me #9).
3. Window **status bar fixed**; only main **Content** scrolls.

### Menu IA (locked depth: regroup + skeleton only)

4. **Remove Canvas** as a nav group.
5. **Strategy** under **Orchestration**.
6. **World KB** under **Creation**.
7. **Work drill-in skeleton** — when `workId` is present in the route
   (`/works/:workId/*`), replace top-level nav groups with (AD-P2-1):
   - `Back to all` / `返回所有` (`nav.drillIn.backToAll`) → `/works`
   - `Outline` / `大纲` (`nav.drillIn.outline`) → `/works/:workId/outline`
   - `Body` / `正文` (`nav.drillIn.body`) → `/works/:workId/chapters` (skeleton entry to chapter/body surface)
   Not a full multi-level tree or parallel work stacks.

### Status bar (desktop, daemon running)

8. Label: **Daemon Running** (en) / product zh equivalent; health tag text
   **`running`** (lowercase; replace current `healthy` soft-badge copy); left **state dot**
   encodes health (existing semantics).
9. **Clickable agent badge** — shows agent **name + version** when known (AD-P2-3);
   empty state placeholder `未选择智能体` / `No agent` (still clickable).
10. Badge navigates to **`/settings/agent`** (Settings → Agent).
11. Keep **Restart** control on the right.

## Non-goals

- Full hierarchical multi-level menu tree
- Parallel work stacks / breadcrumb depth beyond drill-in skeleton
- Status bar on browser build (returns `null` today — preserve)
- Changing canvas route param model (V1.111 architect locks still apply)

## Carry-forward (locked)

| Prior | What V1.117 changes |
| --- | --- |
| V1.111 Canvas nav group | **Remove** Canvas group; regroup Strategy / World KB |
| V1.102 status bar | Redesign copy + add agent badge |
| Grill-me #1 | Static regroup + work drill-in **skeleton only** |
| Grill-me #7–8 | Daemon Running + running tag + agent badge |

## Target state

- Sidebar: nav in the middle, Settings+Profiles pinned bottom, no scroll bleed.
- IA: Orchestration contains Strategy; Creation contains World KB; no Canvas group.
- Work drill-in: three-item skeleton replaces top nav while inside a work.
- Status bar: calm Running line + agent badge shortcut.

## Acceptance criteria (author-observable)

| ID | Criterion | How to verify |
| --- | --- | --- |
| **AC-P2-1** | Settings + Profiles bottom-aligned as one section | Resize window → footer block stays at sidebar bottom; does not scroll with nav |
| **AC-P2-2** | Only main content scrolls | Long page → sidebar + status bar fixed |
| **AC-P2-3** | No Canvas nav group | Sidebar IA has no Canvas group label |
| **AC-P2-4** | Strategy under Orchestration | Strategy entry nested under Orchestration group |
| **AC-P2-5** | World KB under Creation | World KB entry nested under Creation group |
| **AC-P2-6** | Work drill-in skeleton visible in work context | Open a work route → see Back to all + Outline + Body (en + zh-CN labels) |
| **AC-P2-7** | Status: Daemon Running + `running` tag + state dot | Desktop daemon up → footer shows Running + running badge + dot |
| **AC-P2-8** | Agent badge shows name+version or placeholder | Select agent → badge updates; none → placeholder, still clickable |
| **AC-P2-9** | Agent badge → `/settings/agent` | Click badge → Agent settings section |
| **AC-P2-10** | Restart control preserved | Footer still has Restart |

## Architect decisions (§5.2 — locked)

### AD-P2-1: Drill-in trigger

| Condition | Sidebar nav |
| --- | --- |
| `workId` **absent** | Normal Creation / Orchestration groups (post-regroup IA) |
| `workId` **present** (`/works/:workId`, `/works/:workId/outline`, `/works/:workId/chapters`, …) | Drill-in skeleton **only** (three links) |

**Out of drill-in:** `/works` (list), `/worlds/*`, orchestration routes, settings.
Strategy at `/strategies` stays global — not work-scoped drill-in (no `workId`).

Implementation: derive mode from `useParams().workId` in `sidebar.tsx`; do not
add new route params (V1.111 locks preserved).

### AD-P2-2: Layout / scroll SSOT

```
root-layout.tsx
├── aside (sidebar) — h-screen, flex flex-col, overflow hidden
│   └── ShellSidebarChrome — flex-1 nav scrolls; bottom block fixed
│       ├── Settings link
│       └── footer slot (FooterProfiles)
└── main column — flex-1 flex flex-col min-h-0
    ├── header + banner (fixed)
    ├── content — flex-1 overflow-y-auto  ← only scroll region
    └── DaemonStatusBar (fixed footer)
```

`shell-sidebar-chrome.tsx` owns the bottom-aligned **Settings + Profiles** visual
group (single `border-t` block). `root-layout.tsx` owns page-level scroll split.

### AD-P2-3: Menu regroup (implementation map)

| Item | New parent | Route unchanged |
| --- | --- | --- |
| Strategy | Orchestration tab → new or existing group | `/strategies` |
| World KB | Creation tab (remove Canvas group wrapper) | `/worlds` + canvas targets |
| Canvas group label | **Removed** | Outline/World KB links move under Creation |

Reuse `canvas-nav.ts` resolvers for Outline/World KB targets; delete
`CANVAS_NAV_GROUP` as a labeled group — fold items into Creation group or
drill-in only.

### AD-P2-4: Agent badge data wiring

| Field | Source (priority order) |
| --- | --- |
| **Name** | `get_agent_profile().name` → map to overrides `displayName` when key is `*-native` / `*-acp` |
| **Version** | Last successful `POST /v1/daemon/agent-host/scan` entry matching saved profile id/command; else omit version segment |
| **Placeholder** | No profile / unreadable config |

Badge format: `{displayName}` or `{displayName} v{version}` when version non-empty.
Do **not** block status bar on live scan — use React Query cache from Settings/Setup
scan hook or one lightweight refetch on mount (10s stale OK).

Click → `react-router` navigate `/settings/agent`.

### AD-P2-5: i18n keys (new)

Add under `shell.json`:

| Key | en | zh-CN |
| --- | --- | --- |
| `nav.drillIn.backToAll` | Back to all | 返回所有 |
| `nav.drillIn.outline` | Outline | 大纲 |
| `nav.drillIn.body` | Body | 正文 |
| `daemon.runningTag` | running | running |
| `daemon.agentBadge.empty` | No agent | 未选择智能体 |

Deprecate use of `daemon.healthy` for the status tag (replace with `runningTag`).

## Key files (expected)

- `apps/web/src/components/layout/root-layout.tsx`
- `apps/web/src/components/layout/sidebar.tsx`, `canvas-nav.ts`
- `apps/web/src/components/layout/presentational/shell-sidebar-chrome.tsx`
- `apps/web/src/components/layout/daemon-status-bar.tsx`
- Locales: `shell.json`
