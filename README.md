# Nexus

[![CI](https://github.com/42ch-dev/nexus/actions/workflows/ci.yml/badge.svg)](https://github.com/42ch-dev/nexus/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](LICENSE)
[![Node](https://img.shields.io/badge/node-%3E%3D22-brightgreen.svg?logo=nodedotjs&logoColor=white)](package.json)
[![pnpm](https://img.shields.io/badge/pnpm-%3E%3D11-F69220.svg?logo=pnpm&logoColor=white)](package.json)
[![TypeScript](https://img.shields.io/badge/TypeScript-contracts-3178C6.svg?logo=typescript&logoColor=white)](packages/nexus-contracts)
[![Rust](https://img.shields.io/badge/Rust-CLI-DEA584.svg?logo=rust&logoColor=black)](apps/nexus42)
[![Electron](https://img.shields.io/badge/Electron-desktop%20host-47848F.svg?logo=electron&logoColor=white)](apps/desktop-electron)
[![Schema](https://img.shields.io/badge/JSON%20Schema-SSOT-0B7285.svg)](schemas)
[![npm](https://img.shields.io/npm/v/@42ch/nexus-contracts.svg?logo=npm&logoColor=white)](https://www.npmjs.com/package/@42ch/nexus-contracts)
[![Last commit](https://img.shields.io/github/last-commit/42ch-dev/nexus)](https://github.com/42ch-dev/nexus/commits/main)
[![Greptile: The War on Bugs](https://www.greptile.com/badge.svg)](https://www.greptile.com/?utm_source=oss_badge&utm_medium=readme&utm_campaign=greptile_for_open_source)

[中文](README_CN.md) · [Concepts](CONCEPTS.md) · [Strategy](STRATEGY.md)

Nexus is a local-first, AI-driven narrative orchestration engine.

## Quick Start

TBD — the end-user experience is not ready yet; see [Development](#development) to build from source.

---

## Development

For contributors and maintainers working in this monorepo. Root `package.json` scripts wrap common `pnpm -F <workspace>` invocations — run from the repo root.

### Setup

```bash
git clone https://github.com/42ch/nexus.git
cd nexus
pnpm install
```

Prerequisites and the full pre-PR checklist: [`docs/CONTRIBUTING.md`](docs/CONTRIBUTING.md).

### App dev servers

| Command | What it does |
|---------|----------------|
| `pnpm run dev` | CLI + web local dev — reuses a compatible `nexus42` artifact when manifest/hash/protocol match, ensures the standalone TS service is running on the selected loopback endpoint (default 127.0.0.1:8420; starts it detached when it is not), validates service health and identity, then runs Vite in the foreground (`scripts/dev-cli-web.sh`). There is no daemon fallback; missing or incompatible artifacts fail fast with `pnpm dev:backend:refresh`. |
| `pnpm run dev:backend:refresh` | Explicit backend refresh — the only ordinary DX path that may run Cargo build/codegen after Rust/contract edits (`scripts/refresh-dev-backend.mjs`). |
| `pnpm run dev:desktop:web` | Desktop dev with Vite HMR — the Electron host runs against the Vite dev origin instead of the built web dist. |
| `pnpm run dev:desktop` | Electron desktop dev — the host serves the built `apps/web` dist; the driver builds the TS closure and host itself (prepared native payload required). |

Warm Vite HMR for web/Studio/shared UI predates the stable-artifact path; P0 records it as baseline rather than a new speedup claim.

The dev shortcut talks to the **standalone TypeScript service** (`apps/nexus-service/dist/main.js`, normally started with `--home <home> --host 127.0.0.1 --port <port>`). The retired `nexus42 daemon` composition is gone, so no CLI command starts, stops, statuses or proxies the service — that lifecycle belongs to the dev shortcut and to the desktop host. `pnpm run dev:backend:refresh` is the only ordinary DX path that runs Cargo or codegen, and only after Rust or contract edits. The end-to-end first-workflow driver (real `dsh` runtime, prepared-artifact prerequisites) is [`scripts/public-first-workflow.mjs`](scripts/public-first-workflow.mjs) — a developer example, not an end-user flow.

### Build

| Command | What it does |
|---------|----------------|
| `pnpm run build` | Build all TS workspaces (web, design-studio, contracts, ui, codegen, desktop host — no packaging) |
| `pnpm run build:web` | Production build of `apps/web` → `dist/` |
| `pnpm run build:design-studio` | Production build of `apps/design-studio` |
| `pnpm run build:desktop` | Unsigned macOS `.app` / `.dmg` for arm64/x64 (`-- --arch <arch>`; Electron packaging, no signing) |
| `pnpm run build:cli` | Debug build of `nexus42` |
| `pnpm run build:cli:release` | Release build of `nexus42` |

Build individual packages when needed:

```bash
pnpm -F @42ch/nexus-contracts build
pnpm -F @42ch/nexus-ui build
```

### Test & typecheck

| Command | What it does |
|---------|----------------|
| `pnpm run test` | Run tests in every workspace that defines a `test` script |
| `pnpm run test:web` | Web UI Vitest suite |
| `pnpm run test:design-studio` | Design Studio Vitest suite |
| `pnpm run typecheck` | TypeScript `--noEmit` across workspaces that define `typecheck` |

### Schemas & codegen

| Command | What it does |
|---------|----------------|
| `pnpm run validate-schemas` | Validate all JSON Schemas under `schemas/` |
| `pnpm run codegen` | Regenerate Rust + TypeScript types from schemas; rebuild `@42ch/nexus-contracts` |
| `pnpm run codegen:watch` | Watch mode for the codegen tool during schema work |

After editing `schemas/`, run `validate-schemas` then `codegen`, and commit generated output together with schema changes. See [`docs/CONTRIBUTING.md`](docs/CONTRIBUTING.md) for the full pre-PR checklist.

### Desktop (Electron)

The desktop host lives in [`apps/desktop-electron`](apps/desktop-electron) — unsigned macOS builds for arm64 and x64. Development requires a prepared native payload (`@42ch/nexus-native`); the dev driver builds the TypeScript closure and host itself.

```bash
pnpm run dev:desktop                     # host over the built apps/web dist
pnpm run dev:desktop:web                 # Vite HMR + Electron host
pnpm run build:desktop -- --arch arm64   # unsigned .app + .dmg (native arch by default)
```

`nexus42 desktop bundle --arch <arch>` delegates to the same driver. There is no sidecar fetch step — packaging stages the service and native payload itself.

### Headless runtime

`nexus-runtime` is the standalone headless binary for the integrator channel — Connect-only, shares the `~/.nexus42` home with `nexus42`, and ships without the embedded Web UI. Integrator setup and the Connect invoke surface are covered in the [integrator docs](docs/README.md): [nexus-runtime](docs/nexus-runtime.md) (install/run + Connect surface), [strategy-authoring](docs/strategy-authoring.md) (strategy bundles + prompt templates), and [module-authoring](docs/module-authoring.md) (WASM compute modules). The [integrator walkthrough](strategy-samples/README.md) is the end-to-end companion.

### Cleanup

```bash
pnpm run clean    # remove dist/ in contracts, nexus-ui, and codegen packages
```

### Monorepo layout

| Directory | Contents |
|-----------|----------|
| `apps/` | Product surfaces — `nexus42` (Rust CLI: ordinary `cli` + Connect-only `nexus-runtime`), `desktop-electron` (Electron desktop host), `web` (browser SPA) |
| `crates/` | Reusable Rust libraries (core authority, orchestration, local DB, contracts, etc.) |
| `packages/` | npm packages — `@42ch/nexus-contracts` generated from `schemas/` |
| `modules/` | Domain content (embedded presets, WASM modules, reference data) |
| `tooling/` | Codegen pipeline and CI helpers |
| `schemas/` | JSON Schema wire contracts — single truth source for Rust + TypeScript types |

## License

Apache-2.0
