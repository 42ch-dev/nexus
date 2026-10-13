---
module: apps/desktop-electron + apps/web
date: 2026-07-22
last_updated: 2026-10-12
problem_type: architecture_pattern
category: architecture-patterns
severity: medium
tags:
  - chronos
  - titlebar
  - desktop-host
  - overlay
  - drag-region
applies_when:
  - shipping a full-width Chronos ink titlebar on web + the desktop host on macOS with native traffic lights
---

# Chronos Titlebar Overlay Pattern

**Track**: Knowledge (durable guidance).

## Context

Dogfood after the VI logo upgrade required a **titlebar-first** Chronos chrome: full-width deep-blue ink (`#0D2B3E`), brand mark on ink, Settings gear into the Settings modal. On desktop, authors still need **native** traffic lights — `decorations: false` is not an accepted delivery path.

## Guidance

### 1. Desktop-host Overlay — keep decorations

The desktop shell keeps native traffic lights; `decorations: false` is not an accepted delivery path. The **web** titlebar paints the ink plane under the native chrome. Logo/title paint and the empty spacer are plain ink chrome; there is no drag-region attribute (it carries no consumer), and what remains is the macOS safe inset plus the empty-paint double-click callback. Gear, theme, and health controls are explicit interactive islands.

### 2. Layout ownership

- `RootLayout` owns a **full-width** titlebar **above** the sidebar/content row.
- Props-driven `ChronosTitlebarChrome` lives under `apps/web/src/components/layout/presentational/**` and is mirrored in Studio via `@web-layout/*`.
- Routing, Settings open, theme, and desktop detection stay in app wrappers.

### 3. Maximize / window APIs

Double-click maximize needs explicit host commands (`is_maximized` / `maximize` / `unmaximize`). Missing IPC surfaces fail QC even when Overlay paint is correct.

### 4. Smoke gates (human vs agent)

| Check | Owner |
| --- | --- |
| Compile, H1 ink paint, gear → Settings modal, automated drag-contract (A1–A4) | Agent / CI |
| Traffic lights (H2), logo/title/spacer drag (H3–H5), control click islands (H6), dist/HMR parity (D1), double-click maximize (D2), logo/title maximize guard (D3) | **Human** when Screen Recording / Accessibility TCC block agent capture |

Do **not** mark Overlay live-smoke residuals resolved without human evidence for **H2–H6 + D1–D3**. Product code can still Profile-B Done with an open residual and the durable smoke protocol.

## What did not work

- Treating `decorations: false` as a shortcut for full-bleed ink (loses native controls).
- Claiming Dock/titlebar live visuals from compose/preview alone.

## See also

- Brand tokens: [nexus-brand-token-hierarchy.md](nexus-brand-token-hierarchy.md)
- Settings gear target: [settings-modal-primary-host.md](settings-modal-primary-host.md)
