# Settings Agent Section + getAgentProfile (V1.103 P1)

**Status:** architect-locked (§5.2); writing-polished (§5.3)  
**Plan:** `2026-07-09-v1.103-settings-agent-preselect`  
**Shell:** [`settings-shell-ia.md`](settings-shell-ia.md)  
**Compass:** [`v1.103/delivery-compass.md`](../../v1.103/delivery-compass.md)  
**Tier:** Must (P1)  
**Wire:** `wire_contracts_changed: false`

## Goal

Agent is the default Settings section. Mount the existing app-shared `AgentPicker` and **preselect** the saved agent profile via Tauri-only `getAgentProfile` (G1).

## Author-facing outcome

Open Settings → **Agent** already reflects the current saved profile on desktop (when readable) → change selection → persist via `setAgentProfile`.

| Author state | Expected UI |
|--------------|-------------|
| Desktop, profile saved | AgentPicker shows saved profile pre-selected (G1) |
| Desktop, profile unreadable | AgentPicker mounts without crash; author can pick manually |
| Browser | AgentPicker mounts; no fake preselect/persist success |

## Architecture locks (implementer SSOT)

### Route & module

| Lock | Value |
|------|-------|
| Route | `/settings/agent` (index `/settings` redirects here per shell-ia) |
| Page module | `apps/web/src/pages/settings/settings-agent-section.tsx` |
| Shell | Renders inside `SettingsShellLayout` `<Outlet>` — section owns body only |

### AgentPicker & scan

| Concern | Identity |
|---------|----------|
| Picker | `apps/web/src/components/setup/agent-picker.tsx` |
| Scan | `useScanAgents({ filter: 'all', registry_refresh: true })` |
| Mapping | Reuse `mapScanEntriesToPickerItems`, `buildAgentsByPickerId`, `agentPickerId` from `apps/web/src/pages/setup-step-agent.tsx` (extract shared helper module only if needed to avoid duplication) |
| Write | `desktop.setAgentProfile(name, launchCommand?)` — setup parity |
| Save UX | Persist on author commit (Save button pattern from V1.102 thin host) |

### getAgentProfile (G1) — new desktop read path

| Layer | Lock |
|-------|------|
| Tauri command | **`get_agent_profile`** (snake_case invoke name) |
| Rust registration | Add to `generate_handler!` in `apps/desktop/src-tauri/src/lib.rs` alongside `set_agent_profile` |
| Config source | Read `~/.nexus42/agent-host/config.toml` (same file `set_agent_profile` writes) |
| Read rule | Return the **first** `providers[]` entry where `protocol === "native_cli"`; map `id` → `name`, optional `command` → `launchCommand` |
| Return shape (TS) | `Promise<{ name: string; launchCommand?: string } \| null>` |
| Empty | File missing, no providers, no `native_cli` entry, or TOML parse failure → **`null`** (not throw) |
| Invoke transport error | Surface as `null` for preselect path (log optional); section must not crash |
| `DesktopCapabilities` | Add **`getAgentProfile()`** to interface + `TauriDesktopCapabilities` implementation in `apps/web/src/lib/nexus/desktop-capabilities.ts` |
| Browser | `useDesktopCapabilities()` is `null` — skip preselect; show desktop-only toast on save attempt (V1.102 parity) |
| Schemas | **No** `schemas/` / `@42ch/nexus-contracts` / HTTP wire |

### Preselect behavior

1. On mount (desktop): `await desktop.getAgentProfile()`.
2. If non-null: map `name` (+ optional `launchCommand` custom path) onto scan results / picker selection state.
3. If `null`: fall back to V1.102 first-installed default **or** empty until author picks — **do not** block render.
4. Preselect runs **after** scan settles when possible (avoid racing empty scan).
5. Remove V1.102 comment "No getAgentProfile in Must" — G1 is Must for V1.103.

### Tests

| Test | Owner |
|------|-------|
| `getAgentProfile` invoke wiring | `desktop-capabilities.test.ts` |
| Section mount + mocked preselect | `settings-agent-section.test.tsx` |
| Shell default landing | `settings-shell` tests (P0) |

## Author-facing copy (DESIGN Voice)

| Surface | Copy (locked) |
|---------|---------------|
| Section title (in-body, if shown beyond shell) | **Agent** |
| Section helper | Choose which local ACP agent Nexus uses for creative work. |
| Primary persist action | **Save Agent** (or reuse V1.102 thin-host label if already shipped — must be Title Case Verb + Noun) |
| Browser-only helper | Agent selection is available on the desktop app only. |
| Browser save attempt toast | `title`: Save agent on desktop · `description`: Open the Nexus desktop app to change your local agent. |

Avoid protocol jargon (`native_cli`, TOML paths) in author-visible copy.

## In scope

1. `get_agent_profile` Rust command + `DesktopCapabilities.getAgentProfile`.
2. Agent section page under Settings shell.
3. Preselect on mount (desktop).
4. Persist on author commit (same as V1.102).
5. Studio Agent section fixtures.

## Out of scope

- BYOK; execution-mode matrix; package promotion; changing scan wire contracts.
- Connection / Setup / Workspace bodies.

## Acceptance (author-visible)

- Default Settings landing shows AgentPicker with section chrome.
- Desktop: saved profile is pre-selected when `getAgentProfile` returns a value.
- Desktop: changing selection persists via `setAgentProfile` (author can re-open Settings and see updated preselect).
- Vitest covers mount + preselect (mocked desktop caps).
- No `schemas/` change.
