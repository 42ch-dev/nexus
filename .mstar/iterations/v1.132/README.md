# V1.132

Control Room dogfood + orch load blocker: 404 fix, titlebar drag, VI retune, 创作 Create-only hub + persistent 工作区.

**Compass status:** `completed` (end_date 2026-07-22). Integration branch `iteration/v1.132` ready for PR → `main`.

| Artifact | Path |
|----------|------|
| Compass | [delivery-compass.md](delivery-compass.md) |
| Specs | [specs/](specs/) |
| Plans | [`.mstar/plans/`](../../plans/) (`2026-07-22-v1.132-*`) |

## Plans

| Wave | plan_id | Spec |
|------|---------|------|
| 1 | [p0-orch-load-404](../../plans/2026-07-22-v1.132-p0-orch-load-404.md) | [orch-load-404.md](specs/orch-load-404.md) |
| 1∥ | [p1-titlebar-window-drag](../../plans/2026-07-22-v1.132-p1-titlebar-window-drag.md) | [titlebar-window-drag.md](specs/titlebar-window-drag.md) |
| 2 | [p2-vi-aesthetic-retune](../../plans/2026-07-22-v1.132-p2-vi-aesthetic-retune.md) | [vi-aesthetic-retune.md](specs/vi-aesthetic-retune.md) |
| 3 | [p3-creator-orch-gongnengqu-ia](../../plans/2026-07-22-v1.132-p3-creator-orch-gongnengqu-ia.md) | [creator-orch-gongnengqu-ia.md](specs/creator-orch-gongnengqu-ia.md) |

## Wave order (HARD)

| Wave | Plans | Gate |
|------|-------|------|
| 1 | P0 ∥ P1 | P0 Must-first for load; P1 may parallel with worktree isolation |
| 2 | P2 | VI aesthetic (Studio-first) |
| 3 | P3 | After P1 shell chrome stable (soft) |

## Terminology (author-facing)

SSOT: [compass § Terminology](delivery-compass.md#terminology-author-facing) (创作/编排/工作区/功能区, healthy daemon, Chronos).

## Supersessions (explicit)

| Prior | Superseded by | Reason |
|-------|---------------|--------|
| V1.131 chronos-titlebar empty-paint-only drag AC | P1 AC-1, AC-2 | Drag expands to logo/title chrome |
| V1.131 AC-4 / `DF-V1130-WORKSPACE-UNDER-ORCH` (orchestrator-only 工作区) | P3 AC-6, AC-7 | 工作区 is parent; 创作/编排 = modes |
| V1.130 P2 创作-left-Menu false-Done | P3 AC-8 | 创作 hub left = Create-only |

## Dogfood acceptance

Author-facing acceptance criteria live in the [compass](delivery-compass.md) (AC-0..AC-9). Each AC is dogfood-testable; P0 requires Network/curl evidence for Done (no classification-only).
