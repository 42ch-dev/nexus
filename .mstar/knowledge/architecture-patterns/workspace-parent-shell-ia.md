---
module: apps/web shell + apps/design-studio
date: 2026-07-22
problem_type: architecture-pattern
category: architecture-patterns
severity: medium
plan_id: 2026-07-22-v1.132-p3-creator-orch-gongnengqu-ia
tags: [shell, sidebar, workspace, footer, creator-hub, ia]
applies_when: changing Control Room left shell, creator hub, mode switch 创作/编排, or 工作区 footer gating
last_updated: 2026-07-22
---

# Workspace-parent shell IA

**Track:** Knowledge. Distilled from V1.132 P3; supersedes V1.131 orchestrator-only 工作区 (`DF-V1130-WORKSPACE-UNDER-ORCH`) and the V1.130 creator-left-Menu false-Done.

## Context

Authors treat **工作区** as the stable identity parent. 创作 and 编排 are **modes under** that parent — not peers that may hide the workspace footer. Separately, 创作 hub must land on **Create** (创建 World / 延续 Work), not a left Menu of Worlds/Works.

## Ownership lock

| Component | Owns |
|-----------|------|
| `Sidebar` | Left navigation / mode intent; 创作 hub left = Create-only |
| `ShellSidebarChrome` | Persistent framing + left-slot composition; workspace-parent relationship |
| `CreatorShellContent` / hub page | Create-only left actions + **right-side** Worlds/Works lists + entity handoff |
| `FooterProfiles` | Always-visible 工作区 footer/profile anchor — **not** mode-gated |

## Invariants

1. **Footer always mounted** for both 创作 and 编排 (`footer={<FooterProfiles />}` — no orchestrator-only branch).
2. **Mode switch must not change** active workspace / creator identity.
3. **No creator Menu** on the left listing 世界/作品; lists live in the right content region; row select enters entity mode.
4. Studio fixtures prove Create hub + footer both modes (light+dark) before treating App wiring as visually accepted.
5. Do not reinstate V1.131 AC-4 or V1.130 left-Menu as compatibility branches.

## Failure modes

- Footer disappears in 创作 → fix `FooterProfiles` wiring / gating, not content layout first.
- Worlds/Works appear as left nav → remove Menu branch from `Sidebar` / chrome; keep lists on the right.
- Mode switch remounts footer and drops `aria-pressed` / active profile → keep footer always mounted under parent shell state.
