# apps/ — Polyglot Product-Surfaces Directory

`apps/` holds every **product surface** in the Nexus monorepo — runnable things you install or use, regardless of language.

| Entry | Language | Role | Boundary |
|-------|----------|------|----------|
| `nexus42` | Rust | **Producer** — CLI + integrated daemon runtime composition root | Owns the local daemon lifecycle and CLI commands; emits the Local API |
| `desktop` | TypeScript + Tauri/Rust | **Consumer** — Tauri desktop client over IPC + bundled `nexus42` sidecar | Reuses `apps/web` SPA; adds desktop-only native capabilities |
| `web` | TypeScript | **Consumer** — browser SPA served by the daemon | Talks to the Local API over HTTP; also bundled into `apps/desktop` |

## Durable placement rule

> `apps/` = **product surfaces** — runnable things you install or use, any language.  
> `crates/` = **reusable Rust libraries** — building blocks.  
> `packages/` = **publishable npm libraries** — wire contracts.
>
> A new product surface of *any* language → `apps/`. A new reusable Rust library → `crates/`.
>
> `nexus42` is the **producer** (daemon + CLI composition root); `desktop` and `web` are **consumers** (clients over the Local API / IPC boundary).
>
> App-owned nested Rust (for example `apps/desktop/src-tauri/`) lives inside its app directory — it is product-surface implementation, not a shared library. Promote it to `crates/` only if it becomes a reusable building block shared across surfaces.

## Producer/consumer wire boundary

- The producer (`nexus42`) owns the daemon runtime, CLI commands, and local persistence.
- Consumers (`desktop`, `web`) talk to the producer through the Local API (`http://127.0.0.1:<port>/v1/local/*`) or Tauri IPC wrappers.
- Wire contracts live in `schemas/` and are published as `@42ch/nexus-contracts`; no consumer invents its own DTOs.

## Per-entry authority

- `apps/nexus42`: [`apps/nexus42/AGENTS.md`](nexus42/AGENTS.md)
- `apps/desktop`: [`apps/desktop/AGENTS.md`](desktop/AGENTS.md)
- `apps/web`: [`apps/web/AGENTS.md`](web/AGENTS.md)
