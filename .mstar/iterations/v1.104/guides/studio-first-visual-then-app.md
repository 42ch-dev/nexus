# Studio-First: Visual Then App (V1.104)

**Status:** writing-polished (§5.3)  
**Plan:** `2026-07-09-v1.104-settings-workspace`  
**Primary spec:** [`../specs/settings-workspace-section.md`](../specs/settings-workspace-section.md)  
**Compass:** [`../../v1.104-delivery-compass.md`](../../v1.104-delivery-compass.md)

**Author value:** Workspace path change must look intentional before IPC wiring — authors see path, action, and honesty copy in a polished surface, not a debug form.

Reuse the V1.101–V1.103 studio-first discipline for Settings **Workspace** (W2).

1. **Studio fixtures** — Workspace section visual states in Design Studio (path display, **Change Folder…**, post-persist honesty/info, browser-only disabled; props-driven; no daemon required).
2. **Visual acceptance** — Workspace section reads intentional in light/dark; shell nav shows four sections including Workspace.
3. **App wiring** — only after (1)+(2), connect routing/nav/IPC in `apps/web` / desktop.

| Plan | Tier | Studio-first? |
|------|------|---------------|
| P0 Settings Workspace (W2) | Must | Yes |

## Module layout

```
apps/web/src/pages/settings/
  settings-shell-layout.tsx          # existing — extend SETTINGS_SECTIONS + FolderOpen
  settings-workspace-section.tsx     # P0 Must — create

apps/design-studio/src/fixtures/
  settings-host-fixtures.tsx         # add Workspace nav + SettingsWorkspaceSectionFixture
```

## Fixture states (minimum)

| State | Props / variant | Must show |
|-------|-----------------|-----------|
| Desktop — idle | `desktop: true`, path loaded | Current path, enabled **Change Folder…** |
| Desktop — post-persist | `desktop: true`, `saved: true` | Inline honesty copy (restart/reload required) |
| Browser-only | `desktop: false` | Helper copy; disabled **Change Folder…** with tooltip |

## Hard preferences

- `wire_contracts_changed: false` — fixtures are props-driven; no Tauri invoke in Studio
- lucide only (`FolderOpen` for nav + section affordance)
- Register Workspace **route + nav** in the same P0 App change set (V1.103 omitted both)
- Human desktop smoke is a **separate gate**; passing automated plan Done does not substitute for it
- Plan tasks describe product work only — not harness ceremony
