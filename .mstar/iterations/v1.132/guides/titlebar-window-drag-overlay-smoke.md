# Titlebar window-drag — macOS Overlay smoke protocol

**Plan:** `2026-07-22-v1.132-p1-titlebar-window-drag`  
**Spec:** [titlebar-window-drag.md](../specs/titlebar-window-drag.md)  
**Residual:** `R-V1131P0-QC2-W-001` (re-targeted scope — logo/title drag supersedes V1.131 empty-paint-only)  
**Supersedes:** V1.131 P0 `.mstar/sdd/2026-07-22-v1.131-p0-chronos-titlebar/review/macos-overlay-smoke.md` (ephemeral SDD; this guide is the durable V1.132 handoff)

Browser, Studio, and Vitest prove **attribute boundaries only**. Native window movement, traffic-light geometry, and Tauri Overlay integration require **human dogfood** on macOS desktop.

## Preconditions

| Item | Value |
| --- | --- |
| Host | macOS (Apple Silicon or Intel) |
| Build | `pnpm --filter desktop tauri dev` (HMR) **and** a release/dist load after merge |
| Daemon | Local `nexus42` sidecar running (health badge optional in desktop shell) |
| Permissions | Screen Recording / Accessibility if capturing evidence (agents cannot self-grant TCC) |

## Automated pre-checks (agent / CI — not sufficient alone)

| ID | Check | Command / surface | Pass criteria |
| --- | --- | --- | --- |
| A1 | Presentational chrome contract | `pnpm --filter web test -- chronos-titlebar-chrome` | Logo/title/spacer drag attrs; controls `no-drag`; `select-none` + `draggable={false}` |
| A2 | App wrapper wiring | `pnpm --filter web test -- chronos-titlebar` | Desktop mode sets `desktopSafeInset`; gear/theme outside drag region |
| A3 | Studio mirror | `pnpm --filter design-studio test -- chronos-titlebar-fixtures` | Light+dark desktop specimens mirror drag contract |
| A4 | Compile | `pnpm --filter desktop build` | Tauri + web bundle green |

## Human smoke matrix (authoritative for native movement)

Record **Pass / Fail / Blocked** per row. Attach screenshot or short screen recording to PR or QA note when possible.

| ID | Scenario | Steps | Expected |
| --- | --- | --- | --- |
| **H1** | Ink paint geometry | Launch desktop; compare light + dark theme | Full-width `#0D2B3E` bar edge-to-edge; no light system strip above ink; cyan title on dark, white on light |
| **H2** | Native traffic lights | Click close, minimize, zoom | Each control responds; lights align with safe inset (no overlap with logo/title) |
| **H3** | Window drag — logo | Click-drag on Nexus mark (not gear/theme) | **Window moves**; mark does **not** ghost-drag as an image |
| **H4** | Window drag — title | Click-drag on route title text | **Window moves**; text does **not** select |
| **H5** | Window drag — empty paint | Click-drag safe inset + flex spacer | Window moves (regression guard for V1.131 behavior) |
| **H6** | Interactive controls | Click gear, theme toggle | Settings modal opens; theme toggles; clicks are **not** swallowed by drag region |
| **D1** | Dist / HMR parity | Repeat H1–H6 in `tauri dev` and packaged/dist build | No light strip or drag regression between modes |
| **D2** | Double-click maximize | Double-click safe inset and flex spacer only | Window maximizes/restores via Tauri IPC |
| **D3** | Double-click guard | Double-click logo and title | **No** maximize toggle (handlers on empty paint only) |

## Failure triage

| Symptom | Likely layer | Action |
| --- | --- | --- |
| Image ghosts or text selects on drag | Web chrome attrs | Fix `data-tauri-drag-region`, `draggable={false}`, `select-none` in `chronos-titlebar-chrome` |
| Controls stop clicking | Drag region too broad | Narrow drag to logo/title/spacer; keep controls `data-tauri-drag-region="false"` |
| Attributes pass in Studio but window does not move | Tauri Overlay / desktop integration | Keep Overlay; inspect `tauri.conf.json` + webview drag forwarding — **not** a Studio defect |
| Maximize double-click fails | Desktop IPC | Verify `toggleMaximizeWindow` / `is_maximized` commands |

## Evidence path

1. Run matrix above on branch `plan/v1.132-p1-titlebar-window-drag` (or integration `iteration/v1.132` after merge).
2. Record results in PR test plan or QA gate note; reference this file path.
3. On full Pass for H2–H6 + D1–D3, PM may archive `R-V1131P0-QC2-W-001` with evidence link.
4. Until human Pass, residual stays **open** with `tracking_link` → this guide (product code may still ship Profile B Done).

## Related

- Architecture: [chronos-titlebar-overlay.md](../../knowledge/architecture-patterns/chronos-titlebar-overlay.md)
- V1.131 baseline: [chronos-titlebar-chrome.md](../../v1.131/specs/chronos-titlebar-chrome.md) (AC-5 empty-paint drag superseded by V1.132 spec)
