# Settings Connection Section + Redirect (V1.103 P2)

**Status:** architect-locked (§5.2); writing-polished (§5.3)  
**Plan:** `2026-07-09-v1.103-settings-connection`  
**Shell:** [`settings-shell-ia.md`](settings-shell-ia.md)  
**Compass:** [`v1.103/delivery-compass.md`](../../v1.103/delivery-compass.md)  
**Tier:** Must (P2)  
**Wire:** `wire_contracts_changed: false`

## Goal

Close V1.94 deferred Connect-in-Settings: host the existing Connect-to-Daemon UI under `/settings/connection` and permanently redirect `/connect` → `/settings/connection` (C1).

> V1.94 §5: "Connect is reachable only from Settings" — V1.102 shipped thin Settings without Connection body; this spec delivers the body.

## Author-facing outcome

Settings → **Connection** → configure remote daemon endpoint / API key / TOFU fingerprint pinning. No Connect item in sidebar or mobile product nav. Legacy `/connect` bookmarks still work via redirect.

## Architecture locks (implementer SSOT)

### Route & modules

| Lock | Value |
|------|-------|
| Section route | `/settings/connection` |
| Section module | `apps/web/src/pages/settings/settings-connection-section.tsx` |
| Form module | **Extract** from `apps/web/src/pages/connect-daemon-page.tsx` → `apps/web/src/components/settings/connect-daemon-form.tsx` (name locked; adjust only if collision) |
| Legacy page | Remove product use of `ConnectDaemonPage` as a routed page; file may re-export form during migration then delete wrapper |

### Migration strategy: extract (not move-as-page)

1. **Extract** the form/UI logic from `ConnectDaemonPage` into `ConnectDaemonForm` (props-driven where needed).
2. `SettingsConnectionSection` renders section chrome (if any beyond shell) + `<ConnectDaemonForm />`.
3. **Delete** `<Route path="connect" element={<ConnectDaemonPage />}>` from `App.tsx`.
4. Preserve TOFU / fingerprint / `useConnectionConfig` / `useSetConnectionConfig` behavior unchanged.

### Post-action navigation (behavior change from legacy)

| Action | Legacy (`ConnectDaemonPage`) | V1.103 lock |
|--------|------------------------------|-------------|
| Activate / trust connect | `navigate('/')` | **Stay on** `/settings/connection` — toast only, or `navigate('/settings/connection', { replace: true })` if needed |
| Revert to local | `navigate('/')` | Same — **no** redirect to Control Room home |

Rationale: Connection is a Settings destination; authors should remain in Settings after save.

### `/connect` redirect (C1)

| Lock | Value |
|------|-------|
| Placement | `App.tsx`, sibling under `SetupGate` → `RootLayout` (same parent as `settings` tree) |
| Element | `<Route path="connect" element={<Navigate to="/settings/connection" replace />} />` |
| Type | Permanent redirect (`replace`) |

### Nav

- No top-level Connect sidebar/mobile product entry (already true).
- Settings section nav is the product entry.

### Wire

- No new schemas; existing fingerprint/connection local storage remains.

### Test ownership

| Concern | Test file |
|---------|-----------|
| Form states (fingerprint, mismatch, revert) | Migrate from `connect-daemon-page.test.tsx` → `connect-daemon-form.test.tsx` |
| `/connect` redirect | `App` route test or `settings-connection-section.test.tsx` |
| Section mount under shell | `settings-connection-section.test.tsx` |

## Author-facing copy (DESIGN Voice)

Settings hosts Connect — update legacy first-run wording from `ConnectDaemonPage` where it implied setup is incomplete.

| Surface | Copy (locked) |
|---------|---------------|
| Section title (in-body, if shown beyond shell) | **Connection** |
| Section helper | Connect this app to a remote Nexus daemon. Your local daemon stays the default until you activate a remote connection. |
| Form card title | **Connect to Daemon** |
| Form card description | Enter the remote daemon URL and API key. Local mode remains available — you can revert here at any time. |
| Daemon URL field helper | The full HTTPS address of the daemon, including port. |
| API key field helper | The API key from the daemon machine (`nexus42 daemon api-key` on that host). |
| Fingerprint trust helper | Confirm the certificate fingerprint matches what you see on the daemon machine before connecting. |
| Post-save toast (activate) | `title`: Connected to daemon · `description`: Using {endpointUrl} (sentence case; no trailing period) |
| Post-save toast (revert to local) | `title`: Using local daemon · `description`: Remote settings are saved but inactive. |

Primary connect/trust CTAs (Title Case per DESIGN Voice): **Trust This Certificate and Connect** · **Reconnect With These Settings** · **Use Local Daemon** (or equivalent revert label from extracted form).

## In scope

1. Connection section under Settings shell.
2. Extracted `ConnectDaemonForm` + section wrapper.
3. Permanent `/connect` redirect in `App.tsx`.
4. Studio Connection section chrome fixtures (form can be fixture-driven).
5. Vitest: redirect + section mount + form behavior (migrated).

## Out of scope

- Changing TLS/TOFU security model (R-V192SEC-001 remains separate residual).
- BYOK / execution-mode matrix product modes.
- CSRF framework.
- Restoring Connect as a sidebar or top-level product nav entry.

## Acceptance (author-visible)

- Author finds connection config only via Settings → Connection (no sidebar Connect).
- Connection section hosts the full Connect UI; save/pin flows behave as before migration.
- After connect/revert, author remains on Connection section (not kicked to `/works`).
- Navigating to `/connect` lands on `/settings/connection` (permanent redirect).
- No `schemas/` change.
