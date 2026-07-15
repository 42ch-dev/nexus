# Settings Workspace Path — W2 (V1.104 P0)

**Status:** architect-locked (§5.2); writing-polished (§5.3)  
**Plan:** `2026-07-09-v1.104-settings-workspace`  
**Shell delta:** [`settings-shell-workspace-delta.md`](settings-shell-workspace-delta.md)  
**Studio-first:** [`../guides/studio-first-visual-then-app.md`](../guides/studio-first-visual-then-app.md)  
**Compass:** [`v1.104/delivery-compass.md`](../../v1.104/delivery-compass.md)  
**Tier:** Must (P0)  
**Wire:** `wire_contracts_changed: false`  
**Supersedes (scope):** V1.103 Stretch deferral — product intent carried from [`v1.103/specs/settings-workspace-section.md`](../../v1.103/specs/settings-workspace-section.md)

## Carry-forward from V1.103

V1.103 Must shipped the Settings shell without Workspace nav/route (P4 Stretch deferred). V1.104 **Must** delivers the same W2 author outcome described in [`v1.103/specs/settings-workspace-section.md`](../../v1.103/specs/settings-workspace-section.md) — now iteration-blocking, with route + nav registered in P0.

Authors who completed setup under V1.103 cannot change workspace from Settings until V1.104 ships.

## Goal

Authors can view and **change** the current workspace path from Settings (W2), with honest guidance that the daemon/app may need reload/restart after change.

## Author-facing outcome

Settings → **Workspace** → see current path → **Change Folder…** via native picker (desktop) → persist via `set_workspace_path` → see clear reload/restart copy.

**Pain addressed:** post-setup folder change without re-running the wizard or editing `~/.nexus42/config.toml` by hand.

## Architecture locks (implementer SSOT)

### Route & module

| Lock | Value |
|------|-------|
| Route | `/settings/workspace` — child of existing `SettingsShellLayout` nested tree in `apps/web/src/App.tsx` |
| Module | `apps/web/src/pages/settings/settings-workspace-section.tsx` — section body only (no shell chrome) |
| Shell nav | Add Workspace entry in `settings-shell-layout.tsx` `SETTINGS_SECTIONS` — **same P0 change set** as route registration |
| Shell delta SSOT | [`settings-shell-workspace-delta.md`](settings-shell-workspace-delta.md) |

Placement (additive under V1.103 tree):

```tsx
// apps/web/src/App.tsx — import SettingsWorkspaceSection; add child route:
<Route path="workspace" element={<SettingsWorkspaceSection />} />
```

### Capability boundary

| Lock | Value |
|------|-------|
| Hook | `useDesktopCapabilities()` from `apps/web/src/lib/client-context.tsx` |
| Interface | `DesktopCapabilities` in `apps/web/src/lib/nexus/desktop-capabilities.ts` |
| Implementation | `TauriDesktopCapabilities` → Tauri `core.invoke` in desktop build; `null` in browser build |
| Forbidden | Direct `window.__TAURI__` in section code; Daemon API HTTP for workspace path |

### IPC / picker reuse (wizard parity)

Mirror `apps/web/src/pages/setup-step-welcome.tsx` flow (load → pick → persist). Transport is **Tauri IPC**, not Daemon API HTTP — `wire_contracts_changed: false`.

| Step | `DesktopCapabilities` method | Tauri command | Args (JS → Rust) |
|------|------------------------------|---------------|------------------|
| Load current path | `getWorkspaceRoot()` | `get_workspace_root` | — |
| Browse | `pickDirectory(defaultPath)` | `pick_directory` | `{ defaultPath }` |
| Persist | `setWorkspacePath(path)` | `set_workspace_path` | `{ path }` |

Rust handlers: `apps/desktop/src-tauri/src/lib.rs` (`get_workspace_root`, `pick_directory`, `set_workspace_path`). `set_workspace_path` writes `workspace_path` to `~/.nexus42/config.toml` preserving other keys.

**Do not** add new workspace IPC commands, Tauri commands, or `schemas/` changes for W2.

### Persist vs runtime root (honesty rationale)

| Layer | Behavior after `setWorkspacePath` |
|-------|-------------------------------------|
| Config file | `~/.nexus42/config.toml` updated immediately |
| Running sidecar/daemon | May still use workspace root from **app startup** until restart/reload |
| Path guard (`open_with` / `reveal_in_finder`) | Continues using startup-captured `WorkspaceRoot` until app restart ([`apps/desktop/AGENTS.md`](../../../../apps/desktop/AGENTS.md) V1.66 limitation; [`.mstar/specs/desktop-shell.md`](../../../specs/desktop-shell.md) §9) |

### Honesty contract (post-persist)

After successful `setWorkspacePath`:

| Lock | Value |
|------|-------|
| Required UI | Inline success/info copy (not toast-only) stating the **running app and daemon may still use the previous workspace root until you restart or reload the app** |
| Forbidden claim | "Instant everywhere" / "Daemon updated automatically" / silent global consistency |
| Optional CTA | **Quit and reopen Nexus** — copy-only label; **no** wired `stopDaemon`/`startDaemon` sequence |
| `ensureSetupBootstrap` | **Out** — wizard step 1→2 only; do not re-bootstrap on Settings path edit |
| Error surfacing | `pickDirectory` / `setWorkspacePath` failures → toast (mirror wizard); stay on section |

### Multi-workspace

**Out** — single active workspace path in `~/.nexus42/config.toml` only.

### Browser (`useDesktopCapabilities()` returns `null`)

Mirror `settings-setup-section.tsx` pattern:

| Lock | Value |
|------|-------|
| `data-desktop` | `"false"` on section root |
| Helper | Locked browser-only copy (see Author-facing copy table) |
| Change action | **Change Folder…** rendered **disabled** with `title` tooltip — no click handler |
| Forbidden | Fake HTTP workspace path API; optimistic path display implying browser can persist |

## Author-facing copy (DESIGN Voice)

| Surface | Copy (locked) |
|---------|---------------|
| Section title | **Workspace** |
| Section helper | View or change where Nexus stores your creative files on this machine. |
| Current path label | **Workspace folder** |
| Change action | **Change Folder…** (desktop; Settings label — wizard step 1 uses **Browse…**) |
| Post-persist success (inline info, required) | Workspace path saved. Restart or reload the app so the running daemon uses the new location. |
| Optional restart guidance (copy-only CTA) | **Quit and reopen Nexus** — label only; no automatic restart orchestration |
| Browser-only helper | Workspace path changes are available on the desktop app only. |
| Browser-only tooltip | Open the Nexus desktop app to change your workspace folder. |

Forbidden claims: "Changes apply everywhere immediately", "Daemon updated automatically", or any copy that hides the startup-root limitation documented in [`desktop-shell.md`](../../../specs/desktop-shell.md) and [`apps/desktop/AGENTS.md`](../../../../apps/desktop/AGENTS.md).

## In scope

1. Workspace section UI (path display + change).
2. Persist via existing capabilities; honesty copy after success.
3. Studio fixtures in `apps/design-studio/src/fixtures/settings-host-fixtures.tsx` before App wiring claim.

## Out of scope

- Multi-root / workspace switcher.
- Migrating existing creative files between roots.
- Automatic daemon/sidecar restart orchestration.
- Execution-mode matrix / BYOK.
- TOFU transport pinning (R-V192SEC-001).

## Acceptance (author-visible)

| # | Scenario | Pass when |
|---|----------|-----------|
| 1 | Desktop: open Settings → Workspace | Current path visible under **Workspace folder** |
| 2 | Desktop: **Change Folder…** → pick directory → confirm | Path updates; inline info copy mentions restart/reload |
| 3 | Desktop: after persist, before restart | UI does not claim daemon already uses new root |
| 4 | Browser build: open `/settings/workspace` | Desktop-only helper; change disabled |
| 5 | Shell nav | **Workspace** link present; Agent remains default |

- No `schemas/` change.
- Studio visual acceptance before App wiring claim.
