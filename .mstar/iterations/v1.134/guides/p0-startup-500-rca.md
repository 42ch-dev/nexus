# P0 desktop startup 500 — RCA (T1)

**plan_id:** `2026-07-23-v1.134-p0-desktop-startup-500`  
**worktree:** `plan/v1.134-p0-desktop-startup-500`  
**captured:** 2026-07-23 (local dogfood host)  
**daemon:** `nexus42 daemon start --foreground` on `127.0.0.1:8420` (`~/.cache/nexus-target/debug/nexus42`)  
**web:** `pnpm dev:web` → Vite `http://localhost:5173` (same proxy path as `pnpm dev:desktop` dist-load / `vite preview`)

## Executive summary

| Field | Value |
|-------|-------|
| **Status** | Reproduced |
| **Root cause class** | **Transport / Vite proxy** — not a daemon handler `NexusApiError::Internal` branch |
| **Trigger** | Desktop/web SPA issues startup probes **before** the sidecar HTTP listener accepts connections (`ECONNREFUSED` on `:8420`) |
| **Observed HTTP status** | **500** with **empty body** on every proxied `/v1/daemon/*` request |
| **User impact** | Non-blocking — `DaemonLaunchGate` keeps polling `GET /v1/daemon/runtime/health` until the sidecar reaches `running`; app continues |
| **Fix boundary (T2)** | Gate pre-ready fetches and/or change dev/preview proxy error mapping — **not** a single handler patch in `nexus-daemon-runtime` |

**One-line finding:** During the sidecar boot window, Vite's `/v1/daemon` proxy returns **HTTP 500** (upstream `ECONNREFUSED`) for every startup probe; once the daemon listens, the same routes return **200**.

## Reproduction

### Preconditions

1. **Do not** run the daemon on `:8420` (or stop it: `pkill -f "nexus42 daemon"`).
2. Start the web dev/preview server (proxy active):

```bash
cd apps/web && pnpm exec vite --port 5173 --strictPort
# dist-load desktop uses the same preview proxy via apps/desktop tauri.dev.dist.conf.json
```

3. Open the app (browser or Playwright):

```bash
# Browser: http://localhost:5173/works
# Or Playwright MCP against the same URL with network capture
```

### Expected result (repro)

Network log shows **multiple** `500 Internal Server Error` responses, typically including:

| Method | URL | Caller (apps/web) |
|--------|-----|-------------------|
| `GET` | `/v1/daemon/runtime/health` | `DaemonLaunchGate` → `client.health()` |
| `GET` | `/v1/daemon/creators` | `DefaultProfileCoordinator` / `FooterProfiles` → `useCreators()` |
| `GET` | `/v1/daemon/creators?limit=100` | `useCreators({ limit: 100 })` |
| `GET` | `/v1/daemon/creators/active` | `DefaultProfileCoordinator` → `getActiveCreator()` (404 when daemon up; 500 during proxy refusal) |
| `GET` | `/v1/daemon/works?limit=12` | `CreatorEntityListsPanel` → `useWorks({ limit: 12 })` |
| `GET` | `/v1/daemon/narrative/worlds` | `CreatorEntityListsPanel` → `useNarrativeWorlds()` |
| `POST` | `/v1/daemon/agent-host/scan` | `DaemonStatusBar` → `useScanAgents({ filter: 'all' })` (often duplicated) |

Response bodies are **empty**; this is **not** the daemon JSON error envelope (`success: false, error: { code, message }`).

### Vite server evidence

With daemon down, Vite logs (stderr):

```
[vite] http proxy error: /v1/daemon/runtime/health
Error: connect ECONNREFUSED 127.0.0.1:8420
```

Same pattern for `/v1/daemon/creators`, `/works`, `/narrative/worlds`, `/agent-host/scan`.

Playwright capture (daemon stopped, `localStorage` cleared, `GET /works`): **15× HTTP 500** across the routes above — all empty body.

### Control — daemon healthy

With `nexus42 daemon start --foreground` running and the same Vite proxy:

- Playwright sweep of `/`, `/works`, `/strategies`, `/memory`, `/sessions`, `/timeline` → **no** `status >= 500` on any `/v1/daemon/*` call.
- Direct curl against `127.0.0.1:8420` for the startup set → **200** (or **404** for `GET /creators/active` on clean home per `sidecar.rs` integration test).

## Startup fetch sequence (code map)

Mount order in `apps/web/src/App.tsx`:

1. **`DefaultProfileCoordinator`** — mounted **outside** `DaemonLaunchGate` → `useCreators()` + `getActiveCreator()` fire immediately on app load.
2. **`DaemonLaunchGate`** — polls `client.health()` every 1.5s until Tauri reports `running` (desktop) or passes through (browser).
3. After gate opens: **`RootLayout`** → `DaemonStatusBar` (`useScanAgents`), **`CreatorEntityListsPanel`** on `/works` (`useWorks`, `useNarrativeWorlds`), **`FooterProfiles`** (`useCreators` again).

Transport (`apps/web/src/lib/nexus/tauri-client.ts`):

- On Vite origin `:5173`, `TauriClient` uses **same-origin** relative `/v1/daemon/*` → **Vite proxy** (`apps/web/vite.config.ts` → `127.0.0.1:8420`).
- Packaged desktop (non-5173) uses `http://localhost:<port>/v1/daemon/*` **direct** — connection refused surfaces as fetch failure (status 0), not Vite's 500.

## Server-side handler analysis (ruled out for this defect)

Searched `crates/nexus-daemon-runtime/src/api/errors.rs`: `NexusApiError::Internal` → HTTP 500 with JSON envelope. **Not observed** during repro — daemon never receives the request while `ECONNREFUSED`.

When daemon **is** listening, startup handlers return expected domain codes:

| Route | Healthy daemon | Notes |
|-------|----------------|-------|
| `GET /v1/daemon/runtime/health` | 200 | Unguarded liveness |
| `GET /v1/daemon/creators` | 200 | Tier-1; empty list OK without pool |
| `GET /v1/daemon/creators/active` | 200 or 404 | 404 = no active profile (client catches) |
| `GET /v1/daemon/works` | 200 | Empty list OK |
| `GET /v1/daemon/narrative/worlds` | 200 | `{"worlds":[]}` |
| `POST /v1/daemon/agent-host/scan` | 200 | Registry + PATH scan |

`pool_or_uninit()` → **409** `uninitialized`, not 500.

## Architecture note (why probes run early)

`DefaultProfileCoordinator` sits **above** `DaemonLaunchGate` in `App.tsx`, so creator list/active probes are **not** gated on daemon readiness. Combined with dist-load desktop loading the SPA from Vite preview **in parallel** with Tauri sidecar spawn, the first probe wave often hits the proxy while `:8420` is still down.

## Recommended fix boundaries (T2 — do not implement here)

| Option | Owner | Notes |
|--------|-------|-------|
| **A. Gate data queries on daemon ready** | `apps/web` | Move `DefaultProfileCoordinator` inside `DaemonLaunchGate`, or disable TanStack queries until `daemonReady` / health 200 |
| **B. Proxy error mapping** | `apps/web` `vite.config.ts` | Custom `configure` hook: map `ECONNREFUSED` → 503 + JSON instead of Vite default 500 |
| **C. Direct loopback in dist-load dev** | `apps/web` / desktop | Use `http://localhost:<port>` even on `:5173` (trade-off: WebKit CORS — documented in `tauri-client.ts`) |

Regression test ideas: Playwright or MSW asserting **no 500** on startup probe set after sidecar health 200; optional integration test that proxy refusal does not present as 500 once fixed.

## Commands (evidence)

```bash
# Repro (daemon stopped)
pkill -f "nexus42 daemon" || true
cd apps/web && pnpm exec vite --port 5173 --strictPort
# open http://localhost:5173/works → Network tab shows 500s on /v1/daemon/*

# Control (daemon up)
~/.cache/nexus-target/debug/nexus42 daemon start --foreground
curl -sS -w "\nHTTP:%{http_code}\n" http://127.0.0.1:8420/v1/daemon/runtime/health
curl -sS -w "\nHTTP:%{http_code}\n" http://127.0.0.1:8420/v1/daemon/creators
```

## Out of scope / not reproduced

- Single-handler `Internal` 500 on empty workspace (all startup routes 200 with initialized profile).
- Framework 404 from `{param}` routes (closed V1.133).
- Direct daemon crash on `GET /v1/daemon/presets` (one empty-reply observation while daemon was dying; not part of default `/works` startup set).
