# Thin Settings Host — DF-70 Slice A (V1.102 P1)

**Status:** architect-locked (iteration-start §5.2)  
**Plan:** `2026-07-09-v1.102-settings-shell`  
**Tier:** Must (P1)  
**Wire:** `wire_contracts_changed: false` (hard preference; any `schemas/` proposal = hard stop)

## Goal

Ship a **thin Settings host** so authors can change the local ACP agent after first-run setup, by mounting the existing app-shared `AgentPicker`.

## Author-facing outcome

From Control Room → **Settings** → AgentPicker → select installed agent or custom launch → profile persists via the same desktop path as setup. Authors do **not** re-enter the full setup wizard.

## Architecture locks (implementer SSOT)

### Route

| Lock | Value |
|------|-------|
| Path string | **`/settings`** |
| Router placement | Child of `SetupGate` → `RootLayout` in `apps/web/src/App.tsx` (sibling of `/works`, `/memory`, … — **not** under `/setup`) |
| Page module | `apps/web/src/pages/settings-page.tsx` (new) |
| Nested Settings routes | **Forbidden** this slice (no `/settings/*` IA) |
| Header title | Add `'/settings': 'Settings'` to `ROUTE_TITLES` in `apps/web/src/components/layout/root-layout.tsx` |

### Nav entry

| Lock | Value |
|------|-------|
| Label | **`Settings`** (Title Case; DESIGN Voice) |
| Icon | `lucide-react` **`Settings`** only (no Iconify) |
| Desktop slot | **Footer utility** in `apps/web/src/components/layout/sidebar.tsx`: always-visible link **above** `FooterProfiles`, **outside** Creator / Orchestrator tab groups (Settings is cross-cutting, not tab-scoped) |
| Mobile slot | Add `{ to: '/settings', label: 'Settings' }` to `MOBILE_NAV` in `root-layout.tsx` |
| Second nav system | **Forbidden** — reuse existing sidebar + mobile strip only |

### Page composition

1. Settings page is a **host**, not a wizard re-run: no multi-step `WizardState`, no Welcome/Daemon/Done steps.
2. Primary product content: presentational `AgentPicker` from `apps/web/src/components/setup/agent-picker.tsx`.
3. Host owns scan → picker-item mapping, selection state, persist-on-confirm (or persist-on-select — implementer choice; Must requires profile write on author commit of a choice).
4. Reuse mapping helpers from `apps/web/src/pages/setup-step-agent.tsx` (`mapScanEntriesToPickerItems`, `buildAgentsByPickerId`, `agentPickerId`, `assignCollisionSafePickerIds`) — extract a shared module under `apps/web/src/components/setup/` or `pages/` **only if** needed to avoid duplication; do **not** promote to `@42ch/nexus-ui`.
5. Shell chrome: inherit `RootLayout` (sidebar, header, `DaemonStatusBar`). Page-local chrome = title/helper copy + picker region only.

### Persistence (same as setup wizard consumer)

| Concern | Identity |
|---------|----------|
| Capability surface | `DesktopCapabilities` from `apps/web/src/lib/nexus/desktop-capabilities.ts` via `useDesktopCapabilities()` |
| **Write hook** | **`desktop.setAgentProfile(name, launchCommand?)`** → Tauri IPC **`set_agent_profile`** → `~/.nexus42/agent-host/config.toml` |
| Setup parity | Same call site pattern as `SetupWizardPage.finish()` (`apps/web/src/pages/setup-wizard-page.tsx`) |
| Scan | `useScanAgents({ filter: 'all', registry_refresh: true })` / `NexusClient.scanAgents` — same as `SetupStepAgent` |
| Browser build | `useDesktopCapabilities()` is `null`: page still mounts AgentPicker; persist is no-op or desktop-only toast — **do not** invent HTTP wire for profile write |
| Schemas | **No** `schemas/` / `@42ch/nexus-contracts` changes for this slice |

**Read / preselect (locked policy):** There is **no** `getAgentProfile` today. Thin slice **Must** does **not** require a new read IPC. Initial selection may follow setup’s first-installed default (or empty until author picks). Optional UX improvement within P1 capacity: add Tauri-only `getAgentProfile` on `DesktopCapabilities` (**still** `wire_contracts_changed: false`). If deferred, human smoke still validates write + reload survival.

### Studio-first

- Studio fixtures for Settings chrome / Agent page visual states **before** App route wiring (see `guides/studio-first-visual-then-app.md`).
- Studio may mount AgentPicker via `@web-setup/*` with fixture props (no daemon).

### Automated vs human

| Gate | Blocks automated Done? |
|------|------------------------|
| Vitest: `/settings` renders + AgentPicker mounts | **Yes** |
| Studio visual acceptance for Settings chrome | **Yes** (before App wiring claim) |
| Desktop reload / PATH reality smoke | **No** — separate human gate |

## In scope (slice A)

1. `/settings` route + `RootLayout` chrome + nav entry as locked above.
2. One Settings page mounting app-shared `AgentPicker`.
3. Persist via `setAgentProfile` (setup parity).
4. Studio-first Settings chrome fixtures.
5. Inherit polished shell language from Surfaces/App shell tokens (`sidebar-nav`, etc.).

## Out of scope (locked Non-Goals)

- Full multi-section Settings IA (≥2 product sections).
- Nested Settings sidebar taxonomy or `/settings/*` sub-routes.
- BYOK / API-key execution modes; in-app agent installers.
- Promoting AgentPicker to `@42ch/nexus-ui`.
- Iconify; schemas / wire changes.
- Stretch P2 Surfaces menu or chrome polish as Must blockers for this plan.
- Treating optional `getAgentProfile` as a Must blocker if write path + mount tests pass.

## DF-70 disposition

- This iteration **closes DF-70 for the accepted thin slice**.
- Fuller Settings IA remains deferred (tracker update at iteration-close).

## Acceptance

- From Control Room, author opens **Settings** (`/settings`) and sees AgentPicker as the page’s primary content.
- Selecting an installed agent (or custom launch) calls **`setAgentProfile`** with the same semantics as setup finish; Vitest covers route + mount; desktop reload survival confirmed by **separate human smoke** when scheduled.
- Studio Settings chrome fixtures pass visual acceptance before App wiring.
- No `schemas/` change.

## Non-goals reminder

Settings is **not** a re-run of the full setup wizard; it is a host for the reusable picker. Do not grow slice A into full Settings IA during implement.
