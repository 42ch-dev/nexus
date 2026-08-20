---
module: apps/desktop + apps/web
date: 2026-07-22
problem_type: architecture_pattern
category: architecture-patterns
severity: medium
tags: [chronos, titlebar, tauri-v2, overlay, drag-region]
applies_when: shipping a full-width Chronos ink titlebar on web + Tauri v2 macOS with native traffic lights
---

# Chronos Titlebar Overlay Pattern

**Track**: Knowledge (durable guidance, distilled from V1.131 P0 Chronos titlebar chrome).

## Context

Dogfood after the VI logo upgrade required a **titlebar-first** Chronos chrome: full-width deep-blue ink (`#0D2B3E`), brand mark on ink, Settings gear into the Settings modal. On desktop, authors still need **native** traffic lights — `decorations: false` is not an accepted delivery path.

## Guidance

### 1. Tauri v2 macOS Overlay — keep decorations

| Setting | Value |
| --- | --- |
| `titleBarStyle` | `"Overlay"` |
| `hiddenTitle` | `true` |
| Decorations | **on** (native traffic lights) |
| `trafficLightPosition` | Align native controls with the Chronos bar height/inset |

The **web** titlebar paints the ink plane under the native chrome. Logo/title paint and empty spacer use `data-tauri-drag-region`; gear, theme, and health controls are explicit **no-drag** interactive islands.

### 2. Layout ownership

- `RootLayout` owns a **full-width** titlebar **above** the sidebar/content row.
- Props-driven `ChronosTitlebarChrome` lives under `apps/web/src/components/layout/presentational/**` and is mirrored in Studio via `@web-layout/*`.
- Routing, Settings open, theme, and desktop detection stay in app wrappers.

### 3. Maximize / window APIs

Double-click maximize needs explicit Tauri commands (`is_maximized` / `maximize` / `unmaximize`). Missing IPC surfaces fail QC even when Overlay paint is correct.

### 4. Smoke gates (human vs agent)

| Check | Owner |
| --- | --- |
| Compile, H1 ink paint, gear → Settings modal, automated drag-contract (A1–A4) | Agent / CI |
| Traffic lights (H2), logo/title/spacer drag (H3–H5), control click islands (H6), dist/HMR parity (D1), double-click maximize (D2), logo/title maximize guard (D3) | **Human** when Screen Recording / Accessibility TCC block agent capture |

Do **not** mark Overlay live-smoke residuals resolved without human evidence for **H2–H6 + D1–D3**. Product code can still Profile-B Done with an open residual and the durable V1.132 protocol (`titlebar-window-drag-overlay-smoke.md`).

## What did not work

- Treating `decorations: false` as a shortcut for full-bleed ink (loses native controls).
- Claiming Dock/titlebar live visuals from compose/preview alone (same class of error as premature `R-VI-003` archive).

## See also

- Spec (V1.132): `titlebar-window-drag.md`
- Smoke protocol (V1.132): `titlebar-window-drag-overlay-smoke.md`
- V1.131 baseline: `chronos-titlebar-chrome.md`
- Brand tokens: [nexus-brand-token-hierarchy.md](nexus-brand-token-hierarchy.md)
- Settings gear target: [settings-modal-primary-host.md](settings-modal-primary-host.md)
