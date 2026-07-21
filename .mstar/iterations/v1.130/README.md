# V1.130

Control Room shell rewrite (Open Design dual-pane) + daemon Restart + VI Chronos Must.

**Compass status:** `locked` (Phase 1 Review & Edit complete 2026-07-22). Next: Phase 2 on `iteration/v1.130`.

| Artifact | Path |
|----------|------|
| Compass | [delivery-compass.md](delivery-compass.md) |
| Specs | [specs/](specs/) |
| Guides | [guides/](guides/) |

## Wave order (HARD)

| Wave | Plans | Gate |
|------|-------|------|
| 1 | P0 ∥ P4 | Independent; P4 token lock before P1 App paint |
| 2 | P1 | After P4 merge; P0 soft integration |
| 3 | P2 ∥ P3a | After P1 shell/modal boundaries |
| 4 | P3b | After P3a load AC green |

## Terminology (author-facing)

| Concept | Label |
|---------|-------|
| Shell layout | 左功能区 + 右内容区 |
| Mode switch | 创作 \| 编排 (功能区 footer) |
| Profile/workspace | 工作区 (under 编排; P3b) |
| Global settings | Settings modal (≥80% viewport) |
| VI default | T1 Chronos (compile-time) |
