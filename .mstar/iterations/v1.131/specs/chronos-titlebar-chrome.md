# Spec: Chronos titlebar chrome

**plan_id:** `2026-07-22-v1.131-p0-chronos-titlebar`  
**Tracker:** `DF-V1131-CHRONOS-TITLEBAR`  
**Status:** specify+clarify+plan locked (architect Seat 2)

## Problem

After VI logo upgrade, product shell still places the **primary plate** lockup on a **light** sidebar header row. Chronos T1 / MiniShell intent is a **full-width deep-blue titlebar** (`#0D2B3E`) on both light and dark shells, with logo/mark on ink-matching surface. Light shell without that top ink bar reads as "office light chrome" and makes the deep plate logo look like a floating postage stamp.

## Goals

1. Introduce a **full-width Chronos titlebar** strip (`bg-brand-deep-blue` / `#0D2B3E`) across the app shell (web), spanning edge-to-edge with no light-gray gap above or beside it.
2. Place brand mark/logo **on the titlebar** (or on another ink-matching surface), not on light gray alone. On the ink bar, use a **white or bright mark** (not the deep `primary` plate, which would double ink on ink).
3. Titlebar labels: **white** on light shell, **cyan** on dark shell (existing Chronos rule).
4. Align **desktop window chrome** with the Tauri v2 macOS Overlay titlebar so macOS leaves no system-light bar above the ink strip. Native traffic lights remain present, clickable, and correctly positioned; the web bar supplies the deep-blue paint and explicit drag region.
5. Wire **Settings** entry on the new **Chronos titlebar** chrome (gear opens `SettingsModalHost`), closing residual `R-V1130P1-QC1-W-003`. This is **Must** for P0 (the titlebar is being built this iteration; the gear belongs on it). P2 only archives the residual after P0 closes it — P2 does **not** attempt the wire.

## Non-goals

- Redesign sidebar IA groups
- Change primary Button / token dual-role (already shipped)
- Native macOS vibrancy/material effects
- Decorationless (`decorations: false`) windows or web-reimplemented traffic-light controls

## Acceptance

All AC are dogfood-testable (human pass/fail by looking at the running app or Studio fixture):

- **AC-1.** Light shell: viewport top shows a continuous deep-blue titlebar (`#0D2B3E`) edge-to-edge, **no light-gray strip above the sidebar header**. Brand mark/logo sits on the ink bar (white/bright mark on ink).
- **AC-2.** Dark shell: same continuous deep-blue titlebar; labels are **cyan** (legible on ink).
- **AC-3.** Sidebar header is **no longer** the sole brand-mark host on light surfaces (logo placement rule below holds).
- **AC-4.** Studio fixtures render the titlebar in **both** light + dark themes and close `R-V1130P1-QC1-S-001`.
- **AC-5 (desktop).** On macOS, Tauri v2 `titleBarStyle: "Overlay"` extends the web ink bar beneath native chrome. Native close/minimize/zoom controls are clickable and correctly positioned; dragging works from empty titlebar space; double-click behavior and interactive controls are not captured by the drag region. `decorations: false` and a native-titlebar deferral do not satisfy this AC.
- **AC-6 (Settings).** Gear on the **Chronos titlebar** opens the single app-level `SettingsModalHost` (Settings modal) and closes `R-V1130P1-QC1-W-003`. This is required delivery, not a deferral option.

## Surfaces

- `apps/web` root layout / shell chrome
- `apps/design-studio` shell fixtures (dual-pane / titlebar)
- `apps/desktop` Tauri window config + web titlebar drag/safe-area integration

## Architecture decision (locked)

### Window chrome

- Keep Tauri/native decorations enabled. In `apps/desktop/src-tauri/tauri.conf.json`, set the main macOS window to `titleBarStyle: "Overlay"` and `hiddenTitle: true`. Use `trafficLightPosition` to align the retained native controls with the Chronos strip after the first macOS smoke; do not replace them with JavaScript controls.
- Do not set `decorations: false`: that removes native traffic lights and would require a second window-control implementation plus new Tauri window API/capability work, contrary to the existing desktop boundary.
- `macOSPrivateApi` stays `false`; no vibrancy/private API dependency is required.
- The titlebar background carries `data-tauri-drag-region` only on non-interactive paint. Buttons, links, logo, labels, and controls are outside the drag marker. Desktop mode reserves the native traffic-light safe inset; browser mode does not.

### Web shell boundary

- `RootLayout` becomes a viewport-height column: full-width `ChronosTitlebar` first, then the existing sidebar/content row as `min-h-0 flex-1`. This is required for edge-to-edge paint across both panes.
- `ChronosTitlebarChrome` is a props-driven presentational extract under `components/layout/presentational/**`; it owns markup/classes/slots only. The app wrapper owns route title, theme, daemon health, desktop detection, and Settings invocation.
- Remove the reserved sidebar-logo row when the full-width titlebar is present; do not leave a second plate logo on light sidebar chrome. The titlebar uses the transparent bright/white mark on ink, not the square `primary` plate.
- Studio imports only the presentational extract through `@web-layout/*` and renders light + dark variants before App integration.

### Settings entry

- The gear calls the single app-level Settings controller/host with the default section; it does not render a second `Dialog`, own open state, or navigate to a full-page Settings shell.
- P0 mounts/stabilizes the thin `openSettings(defaultSection, invoker)` controller around the existing `SettingsModalHost` and wires the titlebar gear. P2 extends that same host/controller with the route-aware section registry and dirty guard; it does not replace P0’s controller or create another modal. This ordering avoids a P0↔P2 plan dependency cycle.

## Validation

- Browser + Studio: continuous bar geometry, light white labels, dark cyan labels, one logo host, and gear accessibility.
- macOS Tauri smoke: launch both HMR and dist-load modes; verify native close/minimize/zoom, drag from empty bar, double-click, focus, gear/theme clicks, and no light system strip.
- Automated tests assert RootLayout row/column scroll ownership, titlebar slots/colors, drag attribute isolation, and gear-to-host invocation. Native traffic-light geometry remains a mandatory manual QA check.

## Logo placement rule (normative for this plan)

| Asset | Allowed surfaces |
|-------|------------------|
| `primary` (deep plate) | Deep-blue titlebar / ink plates only |
| `whiteBg` | White/light plates only |
| `white` / `NexusMark` bright | Dark heroes / ink titlebar |
| `mono` / tintable mark | Light content chrome when plate is wrong |
