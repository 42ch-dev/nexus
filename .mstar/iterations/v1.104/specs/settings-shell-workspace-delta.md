# Settings Shell — Workspace Nav/Route Delta (V1.104)

**Status:** architect-locked (§5.2); writing-polished (§5.3)  
**Plan:** `2026-07-09-v1.104-settings-workspace`  
**Prior shell SSOT:** [`v1.103/specs/settings-shell-ia.md`](../../v1.103/specs/settings-shell-ia.md) (V1.103 omit)  
**Section SSOT:** [`settings-workspace-section.md`](settings-workspace-section.md)  
**Studio-first:** [`../guides/studio-first-visual-then-app.md`](../guides/studio-first-visual-then-app.md)  
**Compass:** [`v1.104-delivery-compass.md`](../../v1.104-delivery-compass.md)

## Goal

V1.103 shipped the Settings shell **without** Workspace nav/route ([`settings-shell-ia.md`](../../v1.103/specs/settings-shell-ia.md); Stretch deferred). V1.104 **adds** Workspace to the shell allowlist so authors can discover post-setup path change alongside Agent, Connection, and Setup.

## Delta locks

| Lock | Value |
|------|-------|
| Prior omit | V1.103: no `/settings/workspace` route in `App.tsx`; no Workspace entry in `SETTINGS_SECTIONS` |
| V1.104 Must | Register **both** route and nav in the **same P0 change set** |
| Placement | Child of existing `SettingsShellLayout` nested tree (same parent as agent/connection/setup) |
| Default landing | Unchanged — `/settings` index → `agent`; `/settings/agent` unchanged |
| Nav order | Agent · Connection · Setup · **Workspace** (append after Setup) |
| Nav icon | `FolderOpen` (lucide) — distinct from Agent `Bot`, Connection `Wifi`, Setup `RotateCcw` |
| Shell helper | **Unchanged** — V1.103 locked copy; Workspace section owns its own helper |

### `apps/web/src/App.tsx`

Import `SettingsWorkspaceSection` and add child route (V1.103 tree comment removed):

```tsx
<Route path="settings" element={<SettingsShellLayout />}>
  <Route index element={<Navigate to="agent" replace />} />
  <Route path="agent" element={<SettingsAgentSection />} />
  <Route path="connection" element={<SettingsConnectionSection />} />
  <Route path="setup" element={<SettingsSetupSection />} />
  <Route path="workspace" element={<SettingsWorkspaceSection />} />
</Route>
```

### `apps/web/src/pages/settings/settings-shell-layout.tsx`

Extend `SETTINGS_SECTIONS` id union and append Workspace entry:

```tsx
import { Bot, FolderOpen, RotateCcw, Wifi, type LucideIcon } from 'lucide-react';

const SETTINGS_SECTIONS: {
  id: 'agent' | 'connection' | 'setup' | 'workspace';
  label: string;
  to: string;
  icon: LucideIcon;
}[] = [
  { id: 'agent', label: 'Agent', to: '/settings/agent', icon: Bot },
  { id: 'connection', label: 'Connection', to: '/settings/connection', icon: Wifi },
  { id: 'setup', label: 'Setup', to: '/settings/setup', icon: RotateCcw },
  { id: 'workspace', label: 'Workspace', to: '/settings/workspace', icon: FolderOpen },
];
```

Remove V1.103 "Workspace omitted until P4" comments when implementing.

### Studio delta

Extend `apps/design-studio/src/fixtures/settings-host-fixtures.tsx`:

- Add **Workspace** to fixture section nav allowlist (four items).
- Add `SettingsWorkspaceSectionFixture` with desktop + browser-only visual states per primary spec.

## Author-visible acceptance

- Settings sidebar shows **Workspace** after Agent, Connection, Setup.
- Clicking **Workspace** loads the W2 section at `/settings/workspace`.
- Visiting `/settings` or `/settings/agent` still lands on Agent (default unchanged).
- Vitest: shell nav includes `settings-section-nav-workspace`; route outlet mounts `SettingsWorkspaceSection`.

## Non-goals

- Re-architect shell IA (S3 stays).
- Change shell page helper copy to mention workspace (section body owns workspace helper).
- Relocate FooterProfiles / daemon status bar.
- Add execution-mode or BYOK nav stubs.
- Multi-workspace switcher nav entries.
