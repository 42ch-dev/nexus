---
module: apps/web shell + apps/design-studio
date: 2026-07-22
problem_type: architecture-pattern
category: architecture-patterns
severity: medium
tags: [shell, sidebar, workspace, footer, creator-hub, ia, inline-create]
applies_when: changing Control Room left shell, creator hub, mode switch 创作/编排, or 工作区 footer gating
last_updated: 2026-07-23
---

# Workspace-parent shell IA

**Track:** Knowledge. Distilled from V1.132 P3; **V1.135 P0** locks hub create in the **sidebar** (`panelContent`) and hub **content** as browse-only; **V1.136 P1** replaces dashed create cards with **inline-tab create** in the same slot. Still supersedes V1.131 orchestrator-only 工作区 and V1.130 creator-left-Menu.

## V1.134 P3 correction (superseded)

**V1.134 P3** placed hub create in the **content dual-pane left column** (`HubWorkspacePane` / `CreatorHubDualPane` inline create) and hid sidebar `panelContent` on `/works` and `/worlds`. That placement is **incorrect for author intent** — authors expect create in the persistent **sidebar / 功能区** shell column, not inside the main content pane. **V1.135 P0** inverts it; do not cite V1.134 P3 dual-pane-left-create as the normative pattern or keep it as a compatibility branch.

## V1.136 P1 — inline-tab create (normative)

**V1.135 P0** restored sidebar placement but still used two dashed **`CreateCardButton`** cards that opened dialogs. **V1.136 P1** replaces that with an **inline create zone** in the same `panelContent` slot:

```
Sidebar panelContent (creator hub)
├── [ World | Work ]   ← local create-zone tabs (independent state)
├── Inline form fields (World: title; Work: title + goal + idea + profile)
└── Submit → direct API (no dialog on happy path)

Content (CreatorHubPage)
├── Hub tab bar (World | Work — browse SSOT via useHubTabState)
└── Card list OR empty (“从侧边栏创建” / en equivalent)
```

| Layer | Contract |
|-------|----------|
| `CreatorShellContent` | `mode="create-inline"` — tab bar + forms + `sidebar-create-*` testids |
| `CreatorCreatePanel` | Host: mutations, navigation, i18n labels; **no** dialog `open` state |
| Content hub tabs | **Independent** from create-zone tabs — optional post-success sync only |
| i18n | `shell.hub.empty.*` points at sidebar; `shell.hub.create.tabs.ariaLabel`; reuse `worldCreate` / `workCreate` field labels |

**Forbidden:** `mode="create"` card buttons on hub path; `creator-create-world` / `creator-create-work` testids in hub wiring.

## Context

Authors treat **工作区** as the stable identity parent. 创作 and 编排 are **modes under** that parent — not peers that may hide the workspace footer. On hub routes (`/works`, `/worlds`), **create lives in the sidebar** (`Sidebar` → `ShellSidebarChrome.panelContent` via `CreatorCreatePanel`); the **content area** is browse-only (linked World/Work tabs + card list / empty). Selection navigates to canvas — not a full-page controller stub.

## Ownership lock

| Component | Owns |
|-----------|------|
| `Sidebar` | Left navigation / mode intent; **`CreatorCreatePanel` as `panelContent` when `activeTab === 'creator'`** (including hub routes — no `isCreatorHubSurface` hide) |
| `ShellSidebarChrome` | Persistent framing + `panelContent` scroll region (`data-testid="shell-sidebar-panel"`) |
| `CreatorCreatePanel` / `CreatorShellContent` (`create-inline`) | Sidebar create-zone tabs, inline forms, submit testids — **independent** from content hub tab state |
| `CreatorHubPage` / `CreatorHubDualPane` + `hub-*` presentational panes | **Browse only** on hub: linked tabs (`useHubTabState`), card list / empty, empty-state i18n pointing at **sidebar** create; card click → canvas routes |
| `HubWorkspacePane` | Presentational reuse in Studio/fixtures only — **not mounted** from hub App wiring |
| `FooterProfiles` | Always-visible 工作区 footer/profile anchor — **not** mode-gated |

## Terminology lock

| Term | Means | Does **not** mean |
|------|-------|-------------------|
| **Sidebar / 功能区 / sidebar menu slot** | `Sidebar` → `panelContent` shell slot (`ShellSidebarChrome`) | Content dual-pane left column (`CreatorHubDualPane` / `*-workspace-pane*`) |
| **Inline create zone** | World\|Work tabs + form + submit inside `panelContent` (`create-inline`) | Dashed `CreateCardButton` ×2; dialog as primary affordance |
| **Content / 内容区** | `CreatorHubPage` main column — browse only | Create form column inside hub dual-pane |

## Invariants

1. **Footer always mounted** for both 创作 and 编排 (`footer={<FooterProfiles />}` — no orchestrator-only branch).
2. **Mode switch must not change** active workspace / creator identity.
3. **No creator Menu** on the left listing 世界/作品; lists live in the **content** card pane; selection navigates to canvas (does **not** replace browse surface with a controller stub).
4. **Linked tabs:** one shared World/Work tab SSOT above the browse surface; re-resolve initial tab after list queries hydrate (works-only → Work); do not treat pending queries as empty.
5. **Hub create is inline in sidebar `panelContent`** on `/works` and `/worlds` (`data-testid="sidebar-create-panel"`, `data-mode="create-inline"`); reuse `CreatorCreatePanel` + `CreatorShellContent` (`mode="create-inline"`). Create-zone tabs use `sidebar-create-tab-*` testids and **local state** — not coupled to `creator-hub-dual-pane-tab-bar-*`. **Forbidden on hub:** content-left create (`HubWorkspacePane` mount, `onCreateSubmit`, `forceExpandedCreate`, `*-workspace-pane-inline-form` testids); dashed card create (`mode="create"` on hub path).
6. Studio fixtures prove **sidebar inline create + content browse** before treating App wiring as visually accepted (`creator-orch-gongnengqu-ia-fixtures` primary; `creator-hub-dual-pane-ia-fixtures` matrix aligned to `create-inline`).
7. Do not reinstate V1.131 AC-4, V1.130 left-Menu, V1.132 Create-only-left + controller-stub, **V1.134 P3 content dual-pane left-create**, or **V1.135 card-button hub create** as compatibility branches.

## Failure modes

- Footer disappears in 创作 → fix `FooterProfiles` wiring / gating, not content layout first.
- Worlds/Works appear as left nav → remove Menu branch from `Sidebar` / chrome; keep lists in content.
- Mode switch remounts footer and drops `aria-pressed` / active profile → keep footer always mounted under parent shell state.
- **Empty sidebar on hub while create sits in content-left** → restore `panelContent` on hub; collapse hub to browse-only (regression of V1.134 P3 placement).
- **Card-button create returns in sidebar** → restore `create-inline` mode; remove dialog open state from `CreatorCreatePanel`.
- **Create-zone tabs sync with content hub tabs** → decouple state; only optional post-success navigation may align tabs.
- Empty-state copy points at sidebar create but `panelContent` is undefined → fix sidebar gating before copy QA.
