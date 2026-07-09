# Settings Workspace Path — Stretch W2 (V1.103 P4)

**Status:** architect-locked (§5.2); writing-polished (§5.3)  
**Plan:** `2026-07-09-v1.103-settings-workspace`  
**Shell:** [`settings-shell-ia.md`](settings-shell-ia.md)  
**Compass:** [`v1.103-delivery-compass.md`](../../v1.103-delivery-compass.md)  
**Tier:** Stretch (P4) — whole-plan defer allowed  
**Wire:** `wire_contracts_changed: false`

## Goal

Authors can view and **change** the current workspace path from Settings (W2), with honest guidance that the daemon/app may need reload/restart after change.

## Author-facing outcome

Settings → **Workspace** (Stretch only) → see current path → Browse/change via native picker (desktop) → persist via `set_workspace_path` → see clear reload/restart copy.

**Must plans must not ship this section.** If P4 defers, authors continue using wizard-time workspace selection only — iteration close is unaffected.

## Architecture locks (implementer SSOT)

### Route & module (P4 only)

| Lock | Value |
|------|-------|
| Route | `/settings/workspace` — register in `App.tsx` **and** shell nav **only** when P4 runs |
| Module | `apps/web/src/pages/settings/settings-workspace-section.tsx` |
| Shell nav | Add Workspace `NavLink` in same P4 change set — **forbidden** in P0–P3 |

### IPC / picker reuse (wizard parity)

Mirror `apps/web/src/pages/setup-step-welcome.tsx`:

| Step | Capability | Tauri invoke |
|------|------------|--------------|
| Load current path | `desktop.getWorkspaceRoot()` | `get_workspace_root` |
| Browse | `desktop.pickDirectory(currentPath)` | `pick_directory` with `{ defaultPath }` |
| Persist | `desktop.setWorkspacePath(selectedPath)` | `set_workspace_path` with `{ path }` |

**Do not** add new workspace IPC commands or `schemas/` changes for W2.

### Honesty contract (post-persist)

After successful `setWorkspacePath`:

| Lock | Value |
|------|-------|
| Required UI | Inline success/info copy stating the **running app and daemon may still use the previous workspace root until you restart or reload the app** |
| Rationale | `desktop-shell.md` / desktop AGENTS: path guard + sidecar workspace root are captured at startup — live refresh is **not** in V1.103 scope |
| Forbidden claim | "Instant everywhere" / silent global consistency without restart |
| Optional CTA | Manual restart guidance (copy only) — **no** automatic `stopDaemon`/`startDaemon` sequence unless PM explicitly expands scope |
| `ensureSetupBootstrap` | **Out** for Settings workspace change — wizard-only; do not re-bootstrap on path edit |

### Multi-workspace

**Out** — single active workspace path in `~/.nexus42/config.toml` only.

### Browser

Honest desktop-only disabled state; no HTTP workspace path API.

## Author-facing copy (DESIGN Voice)

Honesty contract — never imply instant global consistency after path change.

| Surface | Copy (locked) |
|---------|---------------|
| Section title (in-body, if shown beyond shell) | **Workspace** |
| Section helper | View or change where Nexus stores your creative files on this machine. |
| Current path label | **Workspace folder** |
| Change action | **Change Folder…** (desktop) / **Browse…** if matching wizard parity |
| Post-persist success (inline info, required) | Workspace path saved. Restart or reload the app so the running daemon uses the new location. |
| Optional restart guidance (copy-only CTA) | **Quit and reopen Nexus** — label only; no automatic restart orchestration in V1.103 |
| Browser-only helper | Workspace path changes are available on the desktop app only. |

Forbidden claims: "Changes apply everywhere immediately", "Daemon updated automatically", or any copy that hides the startup-root limitation documented in [`desktop-shell.md`](../../../specs/desktop-shell.md).

## In scope

1. Workspace section UI (path display + change) when P4 runs.
2. Persist via existing capabilities; honesty copy after success.
3. Studio fixtures if plan runs.

## Out of scope

- Multi-root / workspace switcher.
- Migrating existing creative files between roots.
- Treating this plan as Must or blocking iteration close when deferred.
- Automatic daemon/sidecar restart orchestration.

## Acceptance (if plan runs; author-visible)

- Author can view current workspace path and change it on desktop.
- Post-change copy honestly states app/daemon may need restart/reload (no false "instant everywhere" claim).
- No `schemas/` change.

## Deferral

If deferred: compass + DF-70 tracker retarget V1.104+ with reason; **Must completeness unaffected**; no Workspace route or nav item in shipped shell.
