# Creator Hub Dual-Pane IA — Architecture Contract

**plan_id:** `2026-07-23-v1.134-p3-creator-hub-dual-pane-ia`  
**iteration:** V1.134  
**Status:** architect-locked (Seat 2)  
**Authority:** normative technical contract — supersedes product brief for implement tasks  
**Product brief:** [`p3-creator-hub-product-brief.md`](p3-creator-hub-product-brief.md) (Seat 1)

## 1. Target-State Architecture

### 1.1 Layout model

The Creator Hub is a **stable dual-pane** surface (no conditional mode-switch). Every visit to the hub route renders both panes simultaneously:

```
┌─────────────────────────────────────────────────────────┐
│  [世界  Tab ]  [作品  Tab ]    ← shared tab bar        │
├────────────────────────┬────────────────────────────────┤
│                        │                                │
│  LEFT PANE             │  RIGHT PANE                    │
│  Workspace 功能区       │  Card list + empty state       │
│                        │                                │
│  ┌──────────────────┐  │  ┌─────┐ ┌─────┐ ┌─────┐      │
│  │ Create affordance│  │  │Card │ │Card │ │Card │ ...  │
│  │ (inline, no modal│  │  └─────┘ └─────┘ └─────┘      │
│  │  on hub)         │  │                                │
│  └──────────────────┘  │  (or empty-state placeholder)  │
│                        │                                │
│  (entity kind =       │  (entity kind =                │
│   active tab)         │   active tab)                   │
│                        │                                │
└────────────────────────┴────────────────────────────────┘
```

- **Shared tab bar** (World / Work — 世界 / 作品) sits *above* both panes, visually spanning full width. One shared tab SSOT; switching is bidirectional and **both panes react immediately** — the author never sees left on Worlds while right shows Works.
- **Left pane** adapts to the active tab: when World is active, the inline create affordance creates a World; when Work is active, it creates a Work. No create-via-dialog path on the hub.
- **Right pane** adapts to the active tab: shows World cards when World tab is active, Work cards when Work tab is active (never mixed). Empty state when the active kind has zero items.
- **No controller stub.** The `selectedEntity → full-page-replace` path in `creator-hub-page.tsx` (the `mode: 'controller'` branch) is **removed**. The dual-pane chrome is immutable within the hub route.

### 1.2 Tab control — placement + link semantics

**Placement:** One shared tab bar above both panes. Not mirrored controls (two tab bars that stay in sync) — a single control that both panes read from.

**Rationale for shared over mirrored:**
- Single source of truth eliminates sync bugs (e.g. one pane out of sync after a race).
- Simpler a11y: one tab-stop region, not two redundant ones.
- Matches the author's mental model of "I'm in the World workspace" or "I'm in the Work workspace" — the whole dual-pane is that workspace.

**SSOT:** `useHubTab` — a new context or a `useState` in `creator-hub-page.tsx` passed as prop to both pane components. The type is:

```ts
type HubTab = 'world' | 'work';
```

**Link semantics:**
- Clicking either tab label switches both panes.
- The tab bar is the **only** mechanism for switching entity kind on the hub. No hidden shortcuts.
- Tab state is **volatile** — reset to default on mount (default: whichever tab has content, or World if both have content, or Work if only Work has content). Not persisted across route navigations (canvas routes are orthogonal; returning to the hub always starts fresh or remembers the last tab via session memory — TBD in implement, not required for V1.134 Must).

### 1.3 Selection behavior (the "stable chrome" rule)

**Product rule (locked):** Selection must NOT full-page-replace the hub with a controller stub.

**Architect decision:** Selection on a card → **navigate to the entity's canvas route.** The hub is a create + browse surface; deep work happens on canvas. This is the simplest, most aligned model:

| Action | Behavior |
|--------|----------|
| Click a World card | Navigate to `/worlds/:worldId/timeline` |
| Click a Work card | Navigate to `/works/:workId/outline` |
| Submenu "Open Timeline" | Navigate to the appropriate timeline route |
| Submenu "Open Outline" / "Open KB" | Navigate to the appropriate canvas route |
| "Back to Hub" | Standard browser back / shell nav returns to hub dual-pane |

**What this means:**
- The `useCreatorEntitySelection` `selectedEntity` → `CreatorShellContent mode="controller"` path in `creator-hub-page.tsx` is **removed**.
- `useCreatorEntitySelection` may be **simplified or deprecated** on the hub. The right pane does not need a persistent `selectedEntity` state — it just renders cards. Click = navigate.
- The presentational `CreatorEntityLists` component (which renders Worlds + Works in a single vertical stack) is **no longer used on the hub**. It remains available for non-hub surfaces that need dual-kind lists.
- The `CreatorShellContent` `mode: 'controller'` path may be **deprecated** if no other surface uses it. The `mode: 'create'` path is superseded on the hub by inline create in the left pane.

**Alternative considered and rejected:** In-pane detail view (select a card → right pane shows entity overview with "Open in Canvas" button). Rejected for V1.134 because:
- It adds a navigation layer the author didn't ask for.
- Canvas routes already provide rich detail/editing surfaces.
- Keeping the hub as a launchpad (create + browse → launch to canvas) is the simpler, more durable model.
- If in-pane detail is desired later, it can be added without breaking the dual-pane shell — the right pane can grow a detail mode while keeping the left workspace visible. That is a separate iteration.

### 1.4 Inline create scope

**On the hub (left pane):**
- Create World — inline form in the left pane (World tab active)
- Create Work — inline form in the left pane (Work tab active)
- These replace the current hub create-via-dialog paths. No `CreateWorldDialog` / `CreateWorkDialog` is mounted on the hub route.

**Off the hub (still dialogs):**
- `WorldsPage` (`/worlds`) — `CreateWorldDialog` remains as modal
- `WorksPage` (`/works`) — `CreateWorkDialog` remains as modal
- Any future call site outside the hub uses existing dialog components

**Inline create UX (target state — sliced for V1.134):**
- When the active tab has **zero items** (empty state), the left pane shows an expanded create card/form prominently.
- When the active tab has **items**, the left pane shows a compact "Create new…" affordance (button / expandable section).
- Create form fields: minimum = title (required). The full form (description, profile, goal) is a later enhancement — V1.134 only requires functional inline create that replaces the existing dialog behavior (same fields, same validation, inline instead of modal).
- After successful create, the right pane list automatically refreshes to show the new entity.

**Which create dialogs become inline:** Both `CreateWorkDialog` and `CreateWorldDialog` have inline equivalents on the hub left pane. The dialog components themselves are **preserved** for non-hub call sites (`WorldsPage`, `WorksPage`).

### 1.5 Empty-state trigger

| Condition | Right pane behavior |
|-----------|-------------------|
| Active tab = World, World list = empty | Empty state: i18n copy + visual cue pointing left |
| Active tab = Work, Work list = empty | Empty state: i18n copy + visual cue pointing left |
| Active tab = World, World list has items | Cards render normally |
| Active tab = Work, Work list has items | Cards render normally |

**i18n copy (en + zh-CN from day one):**

| Key | en | zh-CN |
|-----|----|-------|
| `hub.empty.worlds` | No Worlds yet — create one from the left | 暂无世界，从左边创建 |
| `hub.empty.works` | No Works yet — create one from the left | 暂无作品，从左边创建 |

### 1.6 Canvas-route orthogonality (confirmation)

**Locked:** The hub route (currently `/`, served by `CreatorHubPage`) and canvas routes (`/works/:workId/*`, `/worlds/:worldId/*`) are **independent product surfaces**. The hub does not absorb, rewrite, or redirect canvas routes. Navigation from hub → canvas is a standard React Router navigation, not a conditional render inside the hub component.

**Confirmation points:**
- `apps/web/src/pages/worlds-page.tsx` — unchanged (standalone Worlds list; consumes dialogs; navigates to `/worlds/:id/*` canvas)
- `apps/web/src/pages/works-page.tsx` — unchanged (standalone Works list; consumes dialogs; navigates to `/works/:id/*` canvas)
- Canvas components under `/works/:id/*` and `/worlds/:id/*` — unchanged
- Hub → canvas navigation uses `navigate()` from React Router, not in-component mode switching
- Shell sidebar / footer / 工作区 parent — unchanged (P3 is hub content IA only)

### 1.7 Relationship to prior IA (V1.132)

| V1.132 | V1.134 P3 (this contract) |
|--------|---------------------------|
| Left = Create-only sidebar panel (Create World + Create Work buttons) | Left = **full workspace 功能区** (shared tab bar + inline create affordance) |
| Right = `CreatorEntityListsPanel` (Worlds section + Works section, both visible) | Right = **card list for active tab only** (World cards OR Work cards, not both) |
| `CreatorEntityLists` presentational (dual-section) used on hub | `CreatorEntityLists` **deprecated on hub**; new single-kind card list |
| `selectedEntity` → controller stub (`CreatorShellContent mode='controller'`) | No controller stub; selection → navigate to canvas |
| Create via dialogs on hub | Create inline in left pane; dialogs preserved for non-hub call sites |
| `useCreatorEntitySelection` as selection SSOT | `selectedEntity` state **removed from hub**; shared `HubTab` replaces `selectedEntity` on hub; `CreatorEntitySelectionProvider` remains available for non-hub consumers |

## 2. Component Ownership

### 2.1 New components

| Component | Path | Responsibility |
|-----------|------|---------------|
| `HubWorkspacePane` | `apps/web/src/components/layout/hub-workspace-pane.tsx` | Left pane: tab-aware inline create affordance + create forms. Reads `HubTab` from context or props. |
| `HubCardListPane` | `apps/web/src/components/layout/hub-card-list-pane.tsx` | Right pane: single-kind card list + empty state. Reads `HubTab` from context or props. |
| `HubTabBar` | (inline in `creator-hub-page.tsx` or shared component) | Shared tab bar above both panes. Two tabs (World / Work), linked. |

All new components are **app-local** under `apps/web/src/components/layout/`. No `@42ch/nexus-ui` promotion in V1.134 (non-goal per product brief). Studio fixture consumes them via `@web-layout/*` aliases.

### 2.2 Modified components

| Component | Change |
|-----------|--------|
| `creator-hub-page.tsx` | Rewrite: compose `HubTabBar` + `HubWorkspacePane` + `HubCardListPane`. Remove `selectedEntity` controller stub branch. Remove `CreatorEntityListsPanel` import. |
| `creator-entity-selection-context.tsx` | **No functional change.** Still provides `useCreatorEntitySelection` for consumers outside the hub. The hub no longer calls `setSelectedEntity` to trigger a controller stub; it sets `HubTab` instead. |
| `creator-entity-lists-panel.tsx` | **Unchanged** — it is the wired consumer of `CreatorEntityLists` and `useCreatorEntitySelection` used by non-hub surfaces. The hub no longer imports it. |
| `creator-shell-content.tsx` | `mode: 'controller'` path is **deprecated** (no hub consumer). Check if any non-hub surface uses it; if none, deprecate with `@deprecated` JSDoc. `mode: 'create'` path is superseded on hub by inline create; remains available for other consumers. |
| `creator-entity-lists.tsx` | **Unchanged** — presentational; consumed by `CreatorEntityListsPanel` (non-hub) and Studio. |

### 2.3 Deprecated (hub context)

| Artifact | Fate |
|----------|------|
| `selectedEntity` → controller stub in `creator-hub-page.tsx` | **Removed.** The `if (selectedEntity)` branch is deleted. |
| `CreatorShellContent mode='controller'` | Deprecated if no other consumer exists. |
| `CreatorEntityListsPanel` from hub | Hub no longer imports it. Component preserved for other routes. |

## 3. Data Flow

### 3.1 Tab state

```
HubTabBar (UI)
  │ onClick → setHubTab('world' | 'work')
  ▼
creator-hub-page.tsx (state owner)
  │ props: activeTab
  ├──▶ HubWorkspacePane (reads activeTab → decides create target)
  └──▶ HubCardListPane  (reads activeTab → fetches + renders cards)
```

### 3.2 Create flow

```
HubWorkspacePane
  │ user fills inline form → onSubmit
  ▼
useCreateWork / useCreateWorld (react-query mutation)
  │ onSuccess → invalidateQueries / refetch
  ▼
HubCardListPane (re-renders with new entity in list)
```

No `selectedEntity` state is involved in the create flow.

### 3.3 Navigate flow

```
HubCardListPane
  │ user clicks card
  ▼
navigate('/works/:workId/outline') or navigate('/worlds/:worldId/timeline')
  │ standard React Router navigation
  ▼
Canvas route component (WorksPage sub-route / WorldsPage sub-route)
```

### 3.4 Query layer

- World list: `useNarrativeWorlds({ limit: N })` — existing query
- Work list: `useWorks({ limit: N })` — existing query
- Both already exist. `HubCardListPane` calls the appropriate query based on `activeTab`.
- No new daemon endpoints. `wire_contracts_changed: false`.

## 4. Wire Contracts

**`wire_contracts_changed: false`.** No new daemon routes, no DTO changes, no schema bumps. This is an apps/web IA change only — it reuses existing `NexusClient` methods (`getWorks`, `getNarrativeWorlds`, `createWork`, `createWorld`) with no contract modifications.

## 5. Design Tokens

- Tab bar: `text-label-14`, `text-gray-1000` (active), `text-gray-700` (inactive), `border-gray-alpha-400` (separator), DESIGN.md tokens for active indicator (cyan accent in Light).
- Cards: existing `CreatorEntityLists` card tokens or new card tokens per DESIGN.md §Components.
- Empty state: `text-copy-14`, `text-gray-900`, icon treatment consistent with Studio fixture.
- Inline create: reuse `CreateWorkDialog` form tokens (Input, Select, Textarea, Button) rendered inline (no dialog chrome).
- **Cyan discipline (inherited from P2 rule):** Light shell = accent-only (tab active indicator, focus ring). Dark shell = liberal.

## 6. Accessibility (WCAG 2.1 AA)

- Tab bar: `role="tablist"`, `role="tab"` with `aria-selected`, keyboard navigation (arrow keys).
- Card list: `role="list"` / `role="listbox"`, keyboard-navigable, focus-visible ring.
- Inline create: form labels, `aria-required`, error announcements via `role="alert"`.
- Empty state: descriptive text, no keyboard trap.

## 7. Studio Fixture Contract

The Studio fixture (`apps/design-studio`) must render:

- **Two tab states:** World tab active, Work tab active
- **Four content states:** empty World tab, populated World tab, empty Work tab, populated Work tab
- **Two themes:** light, dark
- **Combined:** 2 × 2 × 2 = 8 variants minimum
- **Inline create affordance visible** (create form or create button, depending on populated/empty)
- **Empty-state i18n copy** visible (fixture can use hardcoded keys)
- **Author** must visually accept the fixture before app wiring claims (per compass visual acceptance caveat)

## 8. V1.134 Sliced Deliverable

The **target state** is described in §1–§7 above. The **V1.134 deliverable** slices it as follows:

| Slice | Scope | Task |
|-------|-------|------|
| **Phase A (Must)** | Dual-pane shell + linked tabs + empty state + right card list. Hub no longer shows controller stub. | Plan Task 3 |
| **Phase B (Must)** | Inline create replacing hub dialogs. Left pane shows create affordance. Dialogs preserved for non-hub call sites. | Plan Task 4 |
| **Phase C (Deferred to post-V1.134)** | In-pane detail view (right pane shows entity overview when card is selected without leaving hub). | Future iteration |
| **Phase D (Deferred)** | Session tab memory (returning to hub restores last active tab). | Future iteration |
| **Phase E (Deferred)** | Full inline create form parity (description, profile, goal fields inline — V1.134 minimum = title only, matching current dialog behavior). | Future iteration |

**Cut line (V1.134 Must):**
- Stable dual-pane shell with linked tabs ✓
- Right card list for active tab ✓
- Empty state with i18n ✓
- Selection → navigate to canvas (no controller stub) ✓
- Inline create for World + Work (functional, replacing hub dialogs) ✓
- Canvas route orthogonality preserved ✓

**Cut line (explicitly out):**
- In-pane detail / entity overview in right pane
- Tab session memory
- Expanded inline create fields beyond title + existing dialog fields
- `@42ch/nexus-ui` promotion of hub chrome

## 9. Risk & Mitigation (technical)

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| Tab-link sync breaks on rapid switching | Low | Med | Single `useState` SSOT (`HubTab`) — no distributed sync. Test with rapid clicks in Studio fixture. |
| Canvas route accidentally broken by hub refactor | Low | High | Hub and canvas are different route components — refactor touches `creator-hub-page.tsx` only. Existing canvas route tests should still pass. Pre-merge gate: `cargo` not needed; `pnpm --filter web run typecheck && build && test`. |
| `CreatorShellContent mode='controller'` removal breaks another consumer | Low | Med | Grep for `mode=\"controller\"` and `mode: 'controller'` before removal. If another consumer exists, deprecate with JSDoc instead of delete. |
| `useCreatorEntitySelection` context removal breaks non-hub consumers | Low | Med | The context itself is **not removed** — only the hub stops calling `setSelectedEntity` to trigger a controller stub. Other consumers (`CreatorEntityListsPanel`, sidebar, etc.) see no API change. |
| Inline create form design feels cramped in left pane | Med | Low | The left pane has adequate width (~320–400px). If too tight, the create form can expand into a temporary wider state or the right pane can temporarily reduce. Studio fixture reveals this before app wiring. |

## 10. Verification

- `pnpm --filter web run typecheck` — no new TS errors
- `pnpm --filter web run build` — successful production build
- `pnpm --filter web test` — existing hub/creator tests updated; new tests for tab-link + empty state + dual-pane stability
- Manual: navigate hub → create → card appears → click card → canvas renders → back → hub still shows dual-pane (not controller stub)
- Studio: author accepts fixture (World+Work × empty+populated × light+dark)
