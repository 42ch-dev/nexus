---
module: apps/web shell + apps/design-studio
date: 2026-07-22
problem_type: architecture-pattern
category: architecture-patterns
severity: medium
plan_id: 2026-07-22-v1.132-p3-creator-orch-gongnengqu-ia
tags: [shell, sidebar, workspace, footer, creator-hub, ia]
applies_when: changing Control Room left shell, creator hub, mode switch 创作/编排, or 工作区 footer gating
last_updated: 2026-07-23
---

# Workspace-parent shell IA

**Track:** Knowledge. Distilled from V1.132 P3; **V1.134 P3** evolves hub **content** to stable dual-pane (inline create + linked World/Work tabs). Still supersedes V1.131 orchestrator-only 工作区 and V1.130 creator-left-Menu.

## Context

Authors treat **工作区** as the stable identity parent. 创作 and 编排 are **modes under** that parent — not peers that may hide the workspace footer. The 创作 hub is a **stable dual-pane**: left workspace 功能区 (inline create/select), right card list, shared World/Work tab SSOT — not modal create and not a full-page controller stub on selection.

## Ownership lock

| Component | Owns |
|-----------|------|
| `Sidebar` | Left navigation / mode intent; hide duplicate create panel on hub routes (`/works`, `/worlds`) |
| `ShellSidebarChrome` | Persistent framing + left-slot composition; workspace-parent relationship |
| `CreatorHubPage` / `CreatorHubDualPane` + `hub-*` presentational panes | Dual-pane chrome, linked tabs (`useHubTabState`), inline create, empty-state i18n; card click → canvas routes |
| `FooterProfiles` | Always-visible 工作区 footer/profile anchor — **not** mode-gated |

## Invariants

1. **Footer always mounted** for both 创作 and 编排 (`footer={<FooterProfiles />}` — no orchestrator-only branch).
2. **Mode switch must not change** active workspace / creator identity.
3. **No creator Menu** on the left listing 世界/作品; lists live in the **right** card pane; selection navigates to canvas (does **not** replace dual-pane with a controller stub).
4. **Linked tabs:** one shared World/Work tab SSOT above both panes; re-resolve initial tab after list queries hydrate (works-only → Work); do not treat pending queries as empty.
5. **Hub create is inline** on hub routes; dialogs may remain on canvas/sidebar non-hub call sites only.
6. Studio fixtures prove dual-pane (tabs × empty/populated × themes) before treating App wiring as visually accepted.
7. Do not reinstate V1.131 AC-4, V1.130 left-Menu, or V1.132 Create-only-left + controller-stub as compatibility branches.

## Failure modes

- Footer disappears in 创作 → fix `FooterProfiles` wiring / gating, not content layout first.
- Worlds/Works appear as left nav → remove Menu branch from `Sidebar` / chrome; keep lists on the right.
- Mode switch remounts footer and drops `aria-pressed` / active profile → keep footer always mounted under parent shell state.
