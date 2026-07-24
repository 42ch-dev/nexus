# Nexus

[![CI](https://github.com/42ch-dev/nexus/actions/workflows/ci.yml/badge.svg)](https://github.com/42ch-dev/nexus/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](LICENSE)
[![Node](https://img.shields.io/badge/node-%3E%3D20-brightgreen.svg?logo=nodedotjs&logoColor=white)](package.json)
[![pnpm](https://img.shields.io/badge/pnpm-%3E%3D8-F69220.svg?logo=pnpm&logoColor=white)](package.json)
[![TypeScript](https://img.shields.io/badge/TypeScript-contracts-3178C6.svg?logo=typescript&logoColor=white)](packages/nexus-contracts)
[![Rust](https://img.shields.io/badge/Rust-CLI%20%2B%20daemon-DEA584.svg?logo=rust&logoColor=black)](apps/nexus42)
[![Tauri](https://img.shields.io/badge/Tauri-v2-24C8DB.svg?logo=tauri&logoColor=white)](apps/desktop)
[![Schema](https://img.shields.io/badge/JSON%20Schema-SSOT-0B7285.svg)](schemas)
[![npm](https://img.shields.io/npm/v/@42ch/nexus-contracts.svg?logo=npm&logoColor=white)](https://www.npmjs.com/package/@42ch/nexus-contracts)
[![Last commit](https://img.shields.io/github/last-commit/42ch-dev/nexus)](https://github.com/42ch-dev/nexus/commits/main)
[![Greptile: The War on Bugs](https://www.greptile.com/badge.svg)](https://www.greptile.com/?utm_source=oss_badge&utm_medium=readme&utm_campaign=greptile_for_open_source)

[中文](README_CN.md) · [Concepts](CONCEPTS.md) · [Strategy](STRATEGY.md)

Nexus is a local-first creative writing tool.

## Quick Start

> **TBD** — end-user install, first run, and everyday usage.

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
| `pnpm run dev:design-studio` | Design Studio gallery — [http://localhost:5174](http://localhost:5174); no daemon required |
| `pnpm run dev:web` | Web UI — [http://localhost:5173](http://localhost:5173); start the daemon first (`nexus42 daemon start`) |
| `pnpm run dev:desktop` | Tauri desktop dev — starts web dev automatically via `tauri.conf.json` |

### Build

| Command | What it does |
|---------|----------------|
| `pnpm run build` | Build all TS workspaces **except** desktop (web, design-studio, contracts, ui, codegen) |
| `pnpm run build:web` | Production build of `apps/web` → `dist/` |
| `pnpm run build:design-studio` | Production build of `apps/design-studio` |
| `pnpm run build:desktop` | Unsigned macOS `.app` / `.dmg` (runs web build + sidecar + Tauri bundle) |
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

### Desktop sidecar

Desktop builds expect a bundled `nexus42` binary under `apps/desktop/src-tauri/binaries/` (gitignored on fresh clones):

```bash
pnpm run sidecar
```

On Intel Macs, pass an explicit target:

```bash
SIDECAR_TARGETS="x86_64-apple-darwin" pnpm run sidecar
```

### Cleanup

```bash
pnpm run clean    # remove dist/ in contracts, nexus-ui, and codegen packages
```

### Monorepo layout

| Directory | Contents |
|-----------|----------|
| `apps/` | Product surfaces — `nexus42` (Rust CLI + daemon), `desktop` (Tauri client), `web` (browser SPA) |
| `crates/` | Reusable Rust libraries (daemon runtime, orchestration, local DB, contracts, etc.) |
| `packages/` | npm packages — `@42ch/nexus-contracts` generated from `schemas/` |
| `modules/` | Domain content (embedded presets, WASM modules, reference data) |
| `tooling/` | Codegen pipeline and CI helpers |
| `schemas/` | JSON Schema wire contracts — single truth source for Rust + TypeScript types |

## License

Apache-2.0
