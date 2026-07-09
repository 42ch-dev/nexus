# Settings Shell IA — S3 (V1.103 P0)

**Status:** architect-locked (§5.2); writing-polished (§5.3)  
**Plan:** `2026-07-09-v1.103-settings-shell-nav`  
**Compass:** [`v1.103-delivery-compass.md`](../../v1.103-delivery-compass.md)  
**Tier:** Must (P0)  
**Wire:** `wire_contracts_changed: false` (hard preference)  
**Related specs:** [`settings-agent-section.md`](settings-agent-section.md) · [`settings-connection-section.md`](settings-connection-section.md) · [`settings-setup-section.md`](settings-setup-section.md) · [`settings-workspace-section.md`](settings-workspace-section.md) (Stretch)

## Goal

Upgrade the V1.102 thin `/settings` host into a **Settings shell** with secondary section navigation and nested section routes (S3).

## Author-facing outcome

From Control Room → Settings → see section nav (**Agent** / **Connection** / **Setup**; **Workspace** only when P4 Stretch runs) → default landing is **Agent**.

Authors never need a sidebar Connect entry or a top-level Connect product destination after this iteration.

## V1.102 supersession

V1.102 `settings-thin-host.md` locked **no nested `/settings/*`**. V1.103 **supersedes that lock** for the compass section allowlist (Agent, Connection, Setup, optional Workspace). The V1.102 thin host (`settings-page.tsx` Agent-only body) is refactored — not deleted wholesale — into the shell + `SettingsAgentSection`.

## Architecture locks (implementer SSOT)

### Route tree (`apps/web/src/App.tsx`)

Placement: child of `SetupGate` → `RootLayout`, sibling of `/works`, `/memory`, etc. **`/setup` stays outside this tree.**

```tsx
<Route path="settings" element={<SettingsShellLayout />}>
  <Route index element={<Navigate to="agent" replace />} />
  <Route path="agent" element={<SettingsAgentSection />} />
  <Route path="connection" element={<SettingsConnectionSection />} />
  <Route path="setup" element={<SettingsSetupSection />} />
  {/* P4 Stretch only — omit route + nav item when P4 defers */}
  <Route path="workspace" element={<SettingsWorkspaceSection />} />
</Route>
<Route path="connect" element={<Navigate to="/settings/connection" replace />} />
```

| Path | Module | Role |
|------|--------|------|
| `/settings` | `SettingsShellLayout` + index redirect | Shell chrome; index → Agent |
| `/settings/agent` | `SettingsAgentSection` | Agent section body |
| `/settings/connection` | `SettingsConnectionSection` | Connection section body |
| `/settings/setup` | `SettingsSetupSection` | Setup re-run section |
| `/settings/workspace` | `SettingsWorkspaceSection` | **P4 only** — omit when Stretch defers |
| `/connect` | `<Navigate …>` in `App.tsx` | Permanent legacy redirect (C1) |

**Module directory:** `apps/web/src/pages/settings/` — shell layout + one file per section.

### Shell vs section ownership

| Layer | Owner | Responsibility |
|-------|-------|----------------|
| App router | `App.tsx` | Nested `settings` tree + `/connect` redirect |
| Shell | `SettingsShellLayout` | Page title/helper, secondary section nav, active-route highlight, `<Outlet />` |
| Sections | `Settings*Section` pages | Section body only — no duplicate shell chrome |
| Root chrome | `RootLayout` | Sidebar, header (`Settings` title for all `/settings/*`), `DaemonStatusBar` |

### Section nav

- Rendered inside `SettingsShellLayout` (horizontal tabs or vertical subnav within Settings page chrome).
- **Not** Creator/Orchestrator tab items; **not** a second app-wide sidebar.
- Labels: **Agent**, **Connection**, **Setup** (Stretch: **Workspace** — nav link **absent** until P4 runs).
- `NavLink` targets: `/settings/agent`, `/settings/connection`, `/settings/setup` (+ `/settings/workspace` when P4).
- Icons: lucide only.

### Header title

Keep `ROUTE_TITLES['/settings'] = 'Settings'` in `root-layout.tsx`. Existing top-segment resolution covers nested paths.

### Studio-first

Studio fixtures for shell chrome + empty section frames before App nested-route wiring.

## Author-facing copy (DESIGN Voice)

Per repo-root [`DESIGN.md`](../../../../DESIGN.md) §Voice & Content: **Title Case** for page title, section nav labels, and primary actions; **sentence case** for helpers and inline guidance.

| Surface | Copy (locked) |
|---------|---------------|
| Page title (header) | **Settings** |
| Shell helper (below title) | Manage your local agent, daemon connection, and setup options from one place. |
| Section nav labels | **Agent** · **Connection** · **Setup** (+ **Workspace** when P4 Stretch runs) |
| Default section | **Agent** (index redirect) |

Section bodies own their own helpers — see each section spec. Do not duplicate Connect or Setup product copy in the shell helper.

## In scope

1. Nested Settings routes + section nav per tree above.
2. Default section = Agent (index redirect).
3. Refactor V1.102 `settings-page.tsx` → shell layout + `SettingsAgentSection` (Agent body moves; shell gains nav/outlet).
4. Studio shell fixtures.
5. Vitest: shell nav + index redirect + section outlet mounts (stub section bodies OK in P0).

## Out of scope

- Section body product logic beyond P0 Agent refactor scaffold (P1–P4 own bodies).
- BYOK / execution-mode matrix / API-key execution settings (deferred post-V1.103; no placeholder nav).
- AgentPicker package promotion.
- Moving FooterProfiles or daemon strip into Settings.
- Registering Workspace nav/routes before P4 Stretch is authorized.
- `/connect` form logic (P2).

## Acceptance (author-visible)

- Opening Settings shows section nav with Agent, Connection, Setup; default content is Agent.
- Workspace nav item is **absent** unless P4 Stretch runs.
- Nested routes resolve without leaving RootLayout (sidebar + header remain).
- `/settings` redirects to Agent content (via `agent` child).
- Studio shell fixtures accepted before App wiring claim.
- No `schemas/` change.
