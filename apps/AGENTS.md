# apps/ — Polyglot Product-Surfaces Directory

`apps/` holds every **product surface** in the Nexus monorepo — runnable things you install or use, regardless of language.

| Entry | Language | Role | Boundary |
|-------|----------|------|----------|
| `nexus42` | Rust | **Producer** — CLI + integrated daemon runtime composition root | Owns the local daemon lifecycle and CLI commands; emits the Daemon API |
| `desktop-electron` | TypeScript + Electron | **Consumer** — Electron desktop host (macOS; unsigned arm64 + x64) | Reuses `apps/web` SPA; adds desktop-only native capabilities |
| `web` | TypeScript | **Consumer** — browser SPA served by the daemon | Talks to the Daemon API over HTTP; also bundled into `apps/desktop-electron` |

## Durable placement rule

> `apps/` = **product surfaces** — runnable things you install or use, any language.
> `crates/` = **reusable Rust libraries** — building blocks.
> `packages/` = **publishable npm libraries** — wire contracts.
>
> A new product surface of *any* language → `apps/`. A new reusable Rust library → `crates/`.
>
> `nexus42` is the **producer** (daemon + CLI composition root); `desktop-electron` and `web` are **consumers** (clients over the Daemon API / desktop preload bridge).
>
> App-owned nested implementation code (for example a desktop host's native/Rust helper) lives inside its app directory — it is product-surface implementation, not a shared library. Promote it to `crates/` only if it becomes a reusable building block shared across surfaces.

## Producer/consumer wire boundary

- The producer (`nexus42`) owns the daemon runtime, CLI commands, and local persistence.
- Consumers (`desktop-electron`, `web`) reach local data through the Daemon API HTTP surface (`http://127.0.0.1:<port>/v1/daemon/*`); the desktop host's preload bridge carries desktop-only native capabilities, not a second data channel.
- Wire contracts live in `schemas/` and are published as `@42ch/nexus-contracts`; no consumer invents its own DTOs.

## Per-entry authority

- `apps/nexus42`: [`apps/nexus42/AGENTS.md`](nexus42/AGENTS.md)
- `apps/desktop-electron`: [`apps/desktop-electron/AGENTS.md`](desktop-electron/AGENTS.md)
- `apps/web`: [`apps/web/AGENTS.md`](web/AGENTS.md)
