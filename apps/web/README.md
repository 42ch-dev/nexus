# Nexus Local Web UI (`apps/web`)

Daemon-served, local-first **Control Room + Setup** SPA for the Nexus runtime.
React 19 + Vite + TypeScript + TailwindCSS + shadcn/ui primitives + TanStack
Query + React Router. Transport-agnostic via the `NexusClient` adapter
(`BrowserClient` today; `DesktopClient` in the Electron shell). This is an OSS
local-first surface — **not** the private cloud SaaS.

- Product contract: [`.mstar/specs/active/surfaces/web-ui.md`](../../.mstar/specs/active/surfaces/web-ui.md)
- Design tokens (SSOT): [`DESIGN.md`](../../DESIGN.md)
- Plan: `2026-06-24-v1.64-web-app-scaffold` (local plan record)
- Frontend conventions: [`AGENTS.md`](./AGENTS.md)

## Scripts

```sh
pnpm --filter web dev        # Vite dev server (http://localhost:5173)
pnpm --filter web build      # tsc --noEmit + vite build → dist/
pnpm --filter web typecheck  # tsc --noEmit
pnpm --filter web preview    # serve the production build
```

> `build`/`typecheck` resolve types from `@42ch/nexus-contracts`, whose entry
> points to `dist/`. Build the contracts package first (CI does this; locally):
>
> ```sh
> pnpm --filter @42ch/nexus-contracts run build
> pnpm --filter web typecheck
> ```

## Dev workflow

The SPA runs on the Vite dev server, which proxies Daemon API requests to the
running daemon:

```sh
# 1. Start the daemon (default HTTP transport on 127.0.0.1:8420)
nexus42 daemon start

# 2. Run the UI against it
pnpm --filter web dev
```

Override the daemon target (e.g. a non-default port) with `VITE_DAEMON_URL`:

```sh
VITE_DAEMON_URL=http://127.0.0.1:9000 pnpm --filter web dev
```

All `/v1/daemon/*` requests are proxied to that origin. Daemon API data endpoints
are **keyless on loopback** (V1.20 model); the browser client sends no
credentials. In release the built SPA ships through the Electron desktop shell;
the standalone service (`apps/nexus-service`) can optionally serve that same
`apps/web/dist` build at `/` alongside the Local API at `/v1/daemon/*` on one
port, so browser clients stay same-origin.

## Roadmap (desktop-ready boundary)

The `NexusClient` interface (`src/lib/nexus/`) is the single transport boundary.
Screens depend only on the interface, never on `fetch`/IPC directly.

- **V1.64 (this package)** — `BrowserClient` (`fetch` same-origin). The desktop
  shell ships separately in `apps/desktop-electron`; `DesktopClient`
  (`src/lib/nexus/desktop-client.ts`) implements the same interface over the
  Electron preload bridge.
- **V1.65+** — structural preset/findings closures, DESIGN.md → Production
  completeness.

See `web-ui.md` §9 for the full roadmap.

## Stack notes

- **TailwindCSS v3 + CSS variables.** DESIGN.md tokens are projected to CSS
  custom properties in `src/index.css`; `tailwind.config.ts` references those
  variables. Dark mode swaps the variables under `.dark` (token names identical).
- **shadcn/ui primitives** are copied in under `src/components/ui/` and styled
  from the DESIGN.md component tables — no opaque runtime dependency. Add more
  with `npx shadcn@latest add <component>` (configure via `components.json`) or
  copy in manually to keep them on the design tokens.
