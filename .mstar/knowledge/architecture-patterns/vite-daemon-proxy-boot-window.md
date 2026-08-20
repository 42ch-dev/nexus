---
module: apps/web (vite.config + DaemonLaunchGate + query mount)
date: 2026-07-23
problem_type: architecture-pattern
category: architecture-patterns
severity: medium
tags: [vite, proxy, daemon, ECONNREFUSED, startup, desktop-dev]
applies_when: debugging desktop/web startup HTTP 500s; wiring Vite proxy to local daemon; placing queries outside DaemonLaunchGate
last_updated: 2026-07-23
source: iteration:v1.134/guides/p0-startup-500-rca.md
---

# Vite daemon proxy boot-window (no HTTP 500 on ECONNREFUSED)

**Track:** Bug → Knowledge. Distilled from V1.134 P0.

## Context

In `pnpm dev:web` / `pnpm dev:desktop` (dist-load on `:5173`), `TauriClient` uses same-origin `/v1/daemon/*` via the Vite proxy to `127.0.0.1:8420`. While the sidecar is still booting, upstream `ECONNREFUSED` was mapped by Vite’s default proxy to **HTTP 500 with an empty body**. Network logs looked like daemon `Internal` failures; the daemon never received the request.

Packaged Tauri (non-5173) uses direct loopback — refusal surfaces as fetch failure (status 0), not Vite 500.

## Invariants

1. **Proxy connect refusal ≠ daemon Internal.** Empty-body 500 on `/v1/daemon/*` during boot almost always means proxy transport, not `NexusApiError::Internal`.
2. **Map connect errors to 503** (`daemon_unavailable` F-E1 envelope) in both `server` and `preview` proxies (`createDaemonProxyRoute` / `handleDaemonProxyError`).
3. **Gate data queries on daemon ready.** Mount network-heavy coordinators (e.g. `DefaultProfileCoordinator`) **inside** `DaemonLaunchGate` children so creator/works/worlds/scan do not fire during the splash window.
4. **Health polls may still 503** during attach — treat as not-ready; do not paper over real daemon JSON 500 envelopes after the listener is up.

## Failure modes

- Fixing a random daemon handler for a “startup 500” that never hits the server.
- Leaving `DefaultProfileCoordinator` above the gate → probe storm + false error toast once 503 retries exhaust.
- Assuming packaged desktop shows the same 500 (it usually does not).

## See also

- `daemon-ready-gate-pattern.md` — Tauri readiness ownership
- Iteration guide: `p0-startup-500-rca.md`
