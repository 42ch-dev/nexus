# Spec: Titlebar window-drag restore

**plan_id:** `2026-07-22-v1.132-p1-titlebar-window-drag`  
**Status:** plan locked (architect, 2026-07-22)  
**Wave:** 1∥

**Related documents**

- **Compass:** [delivery-compass.md](../delivery-compass.md) (AC-1, AC-2)
- **Plan:** [2026-07-22-v1.132-p1-titlebar-window-drag.md](../../../plans/2026-07-22-v1.132-p1-titlebar-window-drag.md)
- **Supersedes:** [chronos-titlebar-chrome.md](../../v1.131/specs/chronos-titlebar-chrome.md) (V1.131 empty-paint-only drag AC)

## Problem

Chronos titlebar exists but drag feels broken: grabbing logo/title moves the image or selects text instead of moving the window. V1.131 locked drag to empty paint only.

## Goals

- Expand drag to logo/title chrome; `draggable={false}` on logo img; `select-none` on title
- Interactive controls (gear, theme, health) remain clickable (`no-drag`)
- Supersede V1.131 chronos-titlebar empty-paint-only AC

## User Value

Authors can grab anywhere on the Chronos titlebar (including logo and title paint) to move the desktop window — restoring standard OS window-chrome behavior and removing the "drag only works on empty paint" friction locked in by V1.131.

## Non-Goals

- Replacing Overlay with decorated system titlebar
- VI color retune (P2)

## Architecture decision (locked 2026-07-22)

### Boundaries and ownership

- The desktop Overlay remains the window-chrome owner and the web titlebar remains the visual/attribute owner. The implementation uses `data-tauri-drag-region` on the non-interactive logo/title paint and does not replace the Overlay or introduce `-webkit-app-region`.
- The logo mark and title slots are drag-enabled chrome, with `draggable={false}` on the image and `select-none` on title text. Gear, theme, and health controls are explicit `no-drag` interactive islands owned by their existing control components.
- Studio mirrors the presentational chrome state only; desktop Overlay smoke is required because a browser-only fixture cannot prove native window movement.

### Failure modes and rollback

- If logo/title dragging only moves an image or selects text, the web attribute boundary is incomplete; restore the prior empty-paint region while fixing the missing chrome selector.
- If controls stop receiving clicks, narrow the drag region and preserve explicit `no-drag`; do not make the whole header an undifferentiated draggable surface.
- If browser/Studio behavior passes but the desktop window does not move, treat it as an Overlay/desktop integration failure and retain the existing Overlay until smoke evidence is fixed.

## Wire

- Locked verdict: `wire_contracts_changed: false`; this is local shell interaction and has no wire DTO or endpoint impact.

## Acceptance

Maps to compass AC-1, AC-2. Related residual `R-V1131P0-QC2-W-001` close or re-target with Overlay smoke.

### Success criteria (dogfood)

- Dragging the logo or title paint moves the window.
- Logo image is not native-dragged out of the titlebar; title text is not selectable.
- Gear / theme / health controls remain clickable (no-drag).
- Full-width Chronos ink titlebar preserved.
- Overlay smoke confirms drag on desktop shell (not just web).
