# Spec: Creator / Orchestrator 功能区 IA

**plan_id:** `2026-07-22-v1.132-p3-creator-orch-gongnengqu-ia`  
**Status:** plan locked (architect, 2026-07-22)  
**Wave:** 3

**Related documents**

- **Compass:** [delivery-compass.md](../delivery-compass.md) (AC-6..AC-9)
- **Plan:** [2026-07-22-v1.132-p3-creator-orch-gongnengqu-ia.md](../../../plans/2026-07-22-v1.132-p3-creator-orch-gongnengqu-ia.md)
- **Supersedes:** [shell-ia-finish.md](../../v1.131/specs/shell-ia-finish.md) (AC-4 / `DF-V1130-WORKSPACE-UNDER-ORCH`)

## Problem

1. 创作 still lands on left **Menu** (世界 / 作品) instead of **创建** hub — V1.130 P2 false-Done.
2. 工作区 footer only under 编排 — conflicts with product model (workspace parent; modes under it). Supersede V1.131 AC-4 / `DF-V1130-WORKSPACE-UNDER-ORCH`.

## Goals

- **Grill A (locked):** 创作 hub left = Create-only (Create World + 延续 Work); **no Menu mode** on left; Worlds/Works = right content lists; selecting a row → entity mode
- 工作区 always bottom-left; 创作|编排 = in-workspace mode switch
- Studio fixtures light+dark for Create hub (not Menu-only)

## User Value

Authors land on a Create-first hub (创建 World / 延续 Work) instead of a Menu nav — matching the product model where 创作 is about making, not browsing. 工作区 persists across 创作 and 编排, so the author's workspace identity stays stable when switching modes (no more "工作区 disappears when I leave 编排").

## Supersessions (explicit)

| Prior AC / DF | Superseded by | Reason |
|----------------|---------------|--------|
| V1.131 shell-ia-finish AC-4 / `DF-V1130-WORKSPACE-UNDER-ORCH` (工作区 footer only under 编排) | This spec → AC-6, AC-7 | 工作区 is the parent shell; 创作/编排 = modes under it; footer always visible |
| V1.130 P2 创作-left-Menu false-Done | This spec → AC-8 | 创作 hub left = Create-only; no Menu mode on left |

## Non-Goals

- Full Agent Chat product depth
- Orchestrator create+list redesign beyond 创作 default
- Templates gallery
- Multi-root workspace rewrite

## Architecture decision (locked 2026-07-22)

### Shell component ownership

- `Sidebar` owns left navigation and mode intent. In 创作 hub it exposes Create-only actions (创建 World / 延续 Work) and must not render a creator Menu containing Worlds/Works.
- `ShellSidebarChrome` owns persistent shell framing, left-slot composition, and the workspace-parent relationship. It keeps the active workspace identity stable while the mode changes between 创作 and 编排.
- `CreatorShellContent` owns the creator hub content area: Worlds and Works are right-side lists, and selecting a row enters the corresponding entity mode. It does not move those lists into the left shell.
- `FooterProfiles` owns the always-visible workspace footer/profile anchor. Its footer is passed and rendered in both 创作 and 编排; mode switching cannot gate or recreate the workspace identity.
- `apps/design-studio` owns light/dark fixtures proving this composition. `apps/web` owns product routing/state wiring; both reuse existing Create World/Create Work paths.

### Failure modes and rollback

- If 创作 shows Worlds/Works as left Menu navigation, remove that branch from `Sidebar`/`ShellSidebarChrome`; do not reinterpret it as a new mode.
- If the footer disappears in 创作 or switching modes changes workspace identity, fix the `FooterProfiles` wiring and parent-shell state boundary before changing content layout.
- If a right-side list selection loses entity mode, preserve the existing route/state contract and repair only the `CreatorShellContent` handoff.
- V1.131 AC-4 / `DF-V1130-WORKSPACE-UNDER-ORCH` and the V1.130 creator-left-Menu false-Done remain explicitly superseded; no compatibility branch may reinstate them.

## Wire

- Locked verdict: `wire_contracts_changed: false` (reuse existing Create World / Create Work paths; shell ownership and layout do not change wire DTOs or routes).

## Acceptance

Maps to compass AC-6, AC-7, AC-8, AC-9.

### Success criteria (dogfood)

- 创作 hub left shows Create (创建 World / 延续 Work), not Menu nav (世界/作品).
- Worlds/Works appear as right content lists; selecting a row enters entity mode.
- 工作区 footer visible on both 创作 and 编排.
- Switching 创作/编排 does not change the active workspace identity.
- Studio fixtures show 创作 hub Create in light + dark (not Menu-only).
- V1.131 AC-4 / `DF-V1130-WORKSPACE-UNDER-ORCH` supersession documented in this spec and compass.
