# P0 — Sidebar menu-area create IA (normative contract)

> **Status:** Normative (Architect Review & Edit §5.2, 2026-07-23)  
> **Document class:** Iteration Draft overlay — V1.135 P0  
> **Coordinates with:** [`delivery-compass.md`](../delivery-compass.md); plan [`2026-07-23-v1.135-p0-sidebar-menu-create-ia`](../../../plans/2026-07-23-v1.135-p0-sidebar-menu-create-ia.md)  
> **Supersedes for author intent:** V1.134 P3 “hub create in content dual-pane left” — **incorrect placement** (residual `R-V1134P3-001`).

## Author problem (plain language)

When I open **创作** on `/works` or `/worlds`, I expect to **create from the left shell** — the same tall menu/sidebar zone that sits above **创作 | 编排** and **工作区**. V1.134 moved create into the **main content pane’s left column** and left the sidebar almost empty. That is the opposite of what I asked.

**【图1】 evidence:** Empty white sidebar + create cards sitting in content-left (`image-c0a47a7b-6e8f-48e6-9b3c-e0d5b0120b2d.png`).

---

## Architecture decision record (locked)

| Decision | Choice | Rationale |
|----------|--------|-----------|
| **Hub create owner** | `Sidebar` → `ShellSidebarChrome.panelContent` | Author “功能区” = persistent shell column, not content pane |
| **Hub content owner** | `CreatorHubPage` browse surface only | Tabs + card grid / empty — no create column |
| **Reuse vs invent** | Reuse `CreatorCreatePanel` + `CreatorShellContent` (`mode="create"`) | Already wired for off-hub routes; dialogs (`CreateWorldDialog`, `CreateWorkDialog`) stay |
| **Dual implementation** | **Forbidden** | Remove V1.134 content-left create; do not keep compatibility branch |
| **Canvas routes** | Orthogonal | `/works/:id/*`, `/worlds/:id/*` unchanged; sidebar create may remain as today |
| **Wire contracts** | `wire_contracts_changed: false` | UI-only IA inversion |

---

## Component ownership matrix (normative)

| Component | Route scope | Owns | Must NOT own after P0 |
|-----------|-------------|------|------------------------|
| `Sidebar` (`apps/web/src/components/layout/sidebar.tsx`) | App shell | `CreatorCreatePanel` as `panelContent` when `activeTab === 'creator'` | Hub hide via `isCreatorHubSurface` |
| `CreatorCreatePanel` (local to `sidebar.tsx`) | Shell | `CreatorShellContent` create cards + dialog orchestration | — |
| `ShellSidebarChrome` (`presentational/shell-sidebar-chrome.tsx`) | Presentational | Renders `panelContent` in `data-testid="shell-sidebar-panel"` scroll region | Routing, create logic |
| `CreatorHubPage` (`pages/creator-hub-page.tsx`) | `/works`, `/worlds` | Mounts browse-only hub content | Create forms, `HubWorkspacePane` |
| `CreatorHubDualPane` (`creator-hub-dual-pane.tsx`) | Hub wired layer | Tab SSOT (`useHubTabState`), queries, card navigation | Inline create, `onCreateSubmit`, `forceExpandedCreate` |
| `HubDualPaneChrome` (`presentational/hub-dual-pane-chrome.tsx`) | Presentational | **Browse-only usage on hub** — tab bar + card list | Left `HubWorkspacePane` column on hub routes |
| `HubWorkspacePane` | Presentational | Retained for Studio/fixture reuse if needed | **Not mounted** from hub App wiring |

**Terminology lock**

| Term | Means | Does **not** mean |
|------|-------|-------------------|
| **Sidebar / 功能区 / sidebar menu slot** | `Sidebar` → `panelContent` shell slot (`ShellSidebarChrome`) | Content dual-pane left column (`CreatorHubDualPane` / `*-workspace-pane*`) |
| **Content / 内容区** | `CreatorHubPage` main column — browse only | Create form column inside hub dual-pane |

---

## Interface contract

### 1. Sidebar `panelContent` (restore)

**Current (wrong):** `sidebar.tsx` L134–135:

```ts
const creatorPanel =
  activeTab === 'creator' && !isCreatorHubSurface(pathname) ? <CreatorCreatePanel /> : undefined;
```

**Required (normative):**

```ts
const creatorPanel = activeTab === 'creator' ? <CreatorCreatePanel /> : undefined;
```

**Delete:** `isCreatorHubSurface` and `CREATOR_HUB_PATH` usage for panel gating (keep `CREATOR_HUB_PATH` for tab navigation if still referenced).

**Invariant:** On `/works` and `/worlds`, `panelContent` is defined → `shell-sidebar-panel` is populated with `sidebar-create-panel`.

### 2. `CreatorCreatePanel` props / behavior (unchanged)

| Surface | Props / wiring |
|---------|----------------|
| `CreatorShellContent` | `mode="create"`, `canCreateWorld`, `labels` from `worlds` namespace, `onCreateWorld` / `onCreateWork` → dialogs |
| `data-testid` | `"sidebar-create-panel"` (required on hub) |
| Dialogs | `CreateWorldDialog`, `CreateWorkDialog` — same as off-hub today |

Do **not** introduce a third create surface. Inline title-only submit in content (V1.134 `HubWorkspacePane`) is **removed** from hub.

### 3. Hub browse surface (collapse dual-pane create)

**Required composition on `/works`, `/worlds`:**

```
CreatorHubPage
└── CreatorHubBrowse* (wired; may refactor CreatorHubDualPane in place)
    ├── HubTabBar          (World | Work — useHubTabState SSOT)
    └── HubCardListPane    (cards / empty / loading)
```

**Forbidden on hub:**

- `HubWorkspacePane` mount
- `createExpanded`, `onCreateSubmit`, `onExpandCreate`, `forceExpandedCreate`
- `data-testid` matching `*-workspace-pane-inline-form` or `*-workspace-pane-compact-create`

**Acceptable implementation paths (implementer picks one):**

1. Refactor `CreatorHubDualPane` to browse-only (drop left column; call `HubTabBar` + `HubCardListPane` directly or via a new `HubBrowseChrome` presentational extract).
2. Replace hub mount with `CreatorEntityListsPanel` + tab bar if it satisfies PAC-3 with less churn.

**Canvas navigation:** `onSelectCard` → `/worlds/:id/timeline` or `/works/:id/outline` — unchanged.

### 4. Routes (orthogonal — no change)

| Route | Sidebar create | Hub content |
|-------|----------------|-------------|
| `/works`, `/worlds` | **Yes** (`panelContent`) | Browse only |
| `/works/:workId/*`, `/worlds/:worldId/*` | Yes (as today) | Canvas — out of P0 scope |
| Orchestrator routes | No (`navGroups` only) | — |

### 5. i18n contract

| Key | Namespace | Requirement |
|-----|-----------|-------------|
| `hub.empty.worlds` | `shell` | Must refer to **sidebar** create (en + zh-CN). Current zh “从左边创建” is valid **only after** sidebar hosts create; prefer “从侧边栏创建” if copy QA wants disambiguation from content-left |
| `hub.empty.works` | `shell` | Same |
| `emptyCreateWorldTitle`, etc. | `worlds` | Reused by `CreatorCreatePanel` — no duplicate hub-only create strings |

**Fail:** Empty copy points at sidebar while `panelContent` is undefined.

### 6. Test ID contract (CI)

| Assertion | Hub `/works` `/worlds` |
|-----------|------------------------|
| `sidebar-create-panel` | **present** |
| `shell-sidebar-panel` | **present**, non-empty |
| `creator-hub-dual-pane-workspace-pane-inline-form` | **absent** |
| `creator-hub-dual-pane` or successor browse root | **present** |
| Card list / empty testids | **present** per active tab |

Add/update `sidebar.test.tsx` or layout integration test: hub routes assert sidebar create **not** suppressed.

---

## File touch list (implementer)

| File | Action |
|------|--------|
| `apps/web/src/components/layout/sidebar.tsx` | Remove `isCreatorHubSurface` guard; update module comment |
| `apps/web/src/components/layout/creator-hub-dual-pane.tsx` | Remove create wiring; browse-only |
| `apps/web/src/pages/creator-hub-page.tsx` | Update docstring; mount browse-only hub |
| `apps/web/src/pages/creator-hub-page.test.tsx` | Invert assertions (sidebar create present; workspace-pane absent) |
| `apps/web/src/locales/en/shell.json`, `zh-CN/shell.json` | Empty-state copy if needed |
| `apps/design-studio/src/fixtures/creator-hub-dual-pane-ia-fixtures.tsx` | **Replace** dual-pane-create matrix with sidebar-create + content-browse matrix |
| `apps/design-studio/src/fixtures/creator-orch-gongnengqu-ia-fixtures.tsx` | Align creator-hub fixture: create in `panelContent`, lists in content (may already match author intent) |
| `apps/design-studio/src/pages/surfaces.tsx` | Section titles/descriptions if fixture names change |
| `apps/web/src/components/layout/presentational/hub-dual-pane-chrome.tsx` | Optional: extract `HubBrowseChrome` if dual-pane chrome is no longer used on hub |
| `.mstar/knowledge/architecture-patterns/workspace-parent-shell-ia.md` | **Task 4 only** — invert V1.134 dual-pane-create ownership |

**Do not touch:** `schemas/`, daemon APIs, canvas routes, `wire_contracts_changed`.

---

## Product acceptance (PAC — PM locked)

Author-observable gates for hub create placement. Plan AC-1–AC-5 map 1:1 below; plan AC-6–AC-8 cover CI, knowledge, and residual closure.

| ID | Author can observe… | Pass | Fail |
|----|---------------------|------|------|
| **PAC-1** | Create on hub | World + Work create cards/buttons in **left sidebar** | Create only in content-left column |
| **PAC-2** | Sidebar on hub | Sidebar menu zone is **populated** | Empty sidebar while create sits in content |
| **PAC-3** | Content on hub | Single browsing surface: tabs + cards or empty | Dual-pane with left create column |
| **PAC-4** | Empty state copy | Points author to **sidebar** create (en + zh-CN) | Copy says “从左边创建” but left create is in content, or sidebar is empty |
| **PAC-5** | Studio before app claim | Fixture shows sidebar create + content list/empty × World/Work × light/dark | App-only wiring without Studio proof |

## Anti-patterns (do not ship)

1. **Re-hide sidebar create on hub** — any guard that empties `panelContent` on `/works`/`/worlds` is a regression.
2. **Keep dual-pane left-create “for compatibility”** — delete content-left create.
3. **Re-label content-left as “sidebar”** — create inside `CreatorHubDualPane` left pane fails AC regardless of copy.
4. **Cite V1.134 P3 dual-pane as the fix** — V1.135 P0 inverts it.
5. **Third create surface** — no new hub-only create component beyond `CreatorCreatePanel` reuse.

## V1.134 residual carry-forward

| Residual | Disposition |
|----------|-------------|
| `R-V1134P3-001` | Close when PAC-1–5 pass with **sidebar** create |
| V1.134 P3 AC-12 “stable dual-pane left workspace” | **Rejected** for author intent |

## Non-goals

- Dock icon (P1)
- New create APIs or wire contract changes
- Orchestrator IA redesign
- Left nav Menu of all worlds/works
- Modal-only create as **primary** hub path (inline sidebar create is primary)

## Architect sign-off

| Field | Value |
|-------|-------|
| **Signed** | Architect §5.2 Review & Edit |
| **Date** | 2026-07-23 |
| **Dual-pane create** | Removed from hub content; sidebar `CreatorCreatePanel` restored |
| **Knowledge** | Deferred to P0 Task 4 (`workspace-parent-shell-ia.md`) |
