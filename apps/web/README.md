# Nexus Local Web UI (`apps/web`)

Daemon-served, local-first **Control Room + Setup** SPA for the Nexus runtime.
React 18 + Vite + TypeScript + TailwindCSS + shadcn/ui primitives + TanStack
Query + React Router. Transport-agnostic via the `NexusClient` adapter
(`BrowserClient` today; `TauriClient` in V1.65). This is an OSS local-first
surface — **not** the private cloud SaaS.

- Product contract: [`knowledge/specs/web-ui.md`](../../.mstar/knowledge/specs/web-ui.md)
- Design tokens (SSOT): [`DESIGN.md`](./DESIGN.md)
- Plan: [`2026-06-24-v1.64-web-app-scaffold.md`](../../.mstar/plans/2026-06-24-v1.64-web-app-scaffold.md)
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

The SPA runs on the Vite dev server, which proxies Local API requests to the
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

All `/v1/local/*` requests are proxied to that origin. Local API data endpoints
are **keyless on loopback** (V1.20 model); the browser client sends no
credentials. In release the daemon serves the embedded SPA at `/` and the Local
API at `/v1/local/*` on the same port, so the client stays same-origin.

## Roadmap (Tauri-ready boundary)

The `NexusClient` interface (`src/lib/nexus/`) is the single transport boundary.
Screens depend only on the interface, never on `fetch`/`invoke` directly.

- **V1.64 (this package)** — `BrowserClient` (`fetch` same-origin). The desktop
  shell does **not** ship; `TauriClient` exists as a documented stub that throws
  `not_implemented_in_browser_build`.
- **V1.65** — `apps/desktop` Tauri v2 shell loads this `dist`, swaps in
  `TauriClient` (Tauri `invoke`), and hosts the daemon (sidecar first). No screen
  rewrite — only the active client implementation changes.
- **V1.66+** — mobile (Tauri v2 mobile targets), structural preset/findings
  closures, DESIGN.md → Production completeness.

See `web-ui.md` §9 for the full roadmap.

## Stack notes

- **TailwindCSS v3 + CSS variables.** DESIGN.md tokens are projected to CSS
  custom properties in `src/index.css`; `tailwind.config.ts` references those
  variables. Dark mode swaps the variables under `.dark` (token names identical).
- **shadcn/ui primitives** are copied in under `src/components/ui/` and styled
  from the DESIGN.md component tables — no opaque runtime dependency. Add more
  with `npx shadcn@latest add <component>` (configure via `components.json`) or
  copy in manually to keep them on the design tokens.
