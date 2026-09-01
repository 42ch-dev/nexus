# Contributing to Nexus

Thank you for helping improve Nexus. This document is the contributor guide: setup, day-to-day workflows, and the pre-PR checklist that mirrors CI.

- **Command cheat sheet** (dev servers, builds, tests): root [`README.md`](../README.md) → **Development**
- **Repository layout, naming, agent rules**: root [`AGENTS.md`](../AGENTS.md)
- **Codegen details**: [`docs/CODEGEN.md`](CODEGEN.md)

## Code of conduct

Be respectful, constructive, and inclusive.

## Prerequisites

- **Node.js** 22 or newer (`engines.node` in root `package.json`)
- **pnpm** 11 or newer (CI uses pnpm 11)
- **Rust** stable with `clippy` (and `rustfmt` component on stable is not sufficient — see below)
- **Pinned nightly `rustfmt`** — required so local formatting matches CI. Current pin: **`nightly-2026-06-26`** (see `FMT_NIGHTLY` in [`.github/workflows/ci.yml`](../.github/workflows/ci.yml)).

  ```bash
  rustup toolchain install nightly-2026-06-26 --component rustfmt
  ```

  Stable `cargo fmt` ignores workspace `.rustfmt.toml` `ignore` rules and can incorrectly reformat generated code under `crates/nexus-contracts/src/generated/`.

- **Optional — desktop shell:** macOS + [Tauri v2 prerequisites](https://v2.tauri.app/start/prerequisites/) when working on `apps/desktop`
- **Optional — WASM host crate:** `rustup target add wasm32-unknown-unknown` when touching `nexus-wasm-host`

## Getting started

```bash
git clone --filter=blob:none --recurse-submodules https://github.com/42ch/nexus.git
cd nexus
pnpm install --frozen-lockfile
```

`--recurse-submodules` initializes [`.agents/skills/`](../.agents/skills/) (ACP skill root). After a plain clone or pull, if skill dirs are empty:

```bash
git submodule update --init --recursive
```

### Shared Rust build cache (recommended)

To share `target/` across checkouts and worktrees, use [direnv](https://direnv.net/) with the repo-root [`.envrc`](../.envrc):

```bash
direnv allow
```

Without direnv, for the current shell session:

```bash
export CARGO_TARGET_DIR="${XDG_CACHE_HOME:-$HOME/.cache}/nexus-target"
```

Do **not** set `target-dir` in `~/.cargo/config.toml` — that applies to every Rust project on the machine.

## Day-to-day development

Root [`package.json`](../package.json) exposes shortcuts for common tasks. Run from the repository root.

| Task | Command |
|------|---------|
| CLI + web dev (one command) | `pnpm run dev` → builds `nexus42`, ensures the daemon on 127.0.0.1:8420, then Vite dev server in the foreground (`scripts/dev-cli-web.sh`) |
| Desktop dev | `pnpm run dev:desktop` (starts web via Tauri `beforeDevCommand`) |
| Desktop dev | `pnpm run dev:desktop` (starts web via Tauri `beforeDevCommand`) |
| TS workspaces build | `pnpm run build` (all workspaces **except** desktop — avoids accidental Tauri bundle) |
| Web / Studio build | `pnpm run build:web`, `pnpm run build:design-studio` |
| Desktop bundle | `pnpm run build:desktop` (web build + sidecar + Tauri; see below) |
| CLI build | `pnpm run build:cli` or `pnpm run build:cli:release` |
| TS tests | `pnpm run test`, or `pnpm run test:web` / `pnpm run test:design-studio` |
| TS typecheck | `pnpm run typecheck` |
| Schema validate + codegen | `pnpm run validate-schemas`, `pnpm run codegen` |
| Desktop sidecar binary | `pnpm run sidecar` |

Build individual npm packages when needed:

```bash
pnpm -F @42ch/nexus-contracts build
pnpm -F @42ch/nexus-ui build
```

### Iteration vs pre-PR scope

During daily work, prefer **scoped** commands for the crate or app you are editing:

```bash
cargo check -p <crate>
cargo test -p <crate>
cargo clippy -p <crate> -- -D warnings
```

Before opening a PR, run the **full** gates in [Local checks (mirror CI)](#local-checks-mirror-ci) below (`cargo clippy --all`, `cargo test --all`, workspace `pnpm run typecheck`, etc.).

See [`AGENTS.md`](../AGENTS.md) for `target/` disk hygiene and when to run `cargo clean`.

### Desktop sidecar

`apps/desktop` bundles a `nexus42` sidecar at compile time. The binary under `apps/desktop/src-tauri/binaries/` is gitignored — on a fresh clone, before `pnpm run dev:desktop` or `pnpm run build:desktop`:

```bash
pnpm run sidecar
```

On Intel Macs:

```bash
SIDECAR_TARGETS="x86_64-apple-darwin" pnpm run sidecar
```

## Schema-first development

JSON Schemas under `schemas/` are the source of truth. TypeScript and Rust contract types are generated; do not hand-edit generated files.

1. Edit or add schemas in `schemas/`.
2. Validate: `pnpm run validate-schemas`
3. Regenerate: `pnpm run codegen` (also rebuilds `@42ch/nexus-contracts`)
4. Implement against generated types in `packages/nexus-contracts/` and `crates/nexus-contracts/`.
5. Add or update tests.
6. **Commit schema changes and all generated output together** so CI’s codegen check passes.

Generated paths checked in CI:

- `packages/nexus-contracts/src/generated/`
- `crates/nexus-contracts/src/generated/`

For wire-type changes, also run `bash tooling/check-wire-drift.sh` before pushing (see checklist below).

## Local checks (mirror CI)

Run these before requesting review. Order follows the main pipeline in [`.github/workflows/ci.yml`](../.github/workflows/ci.yml). Stop and fix at the first failure.

### 1. Schemas and codegen

```bash
pnpm run validate-schemas
pnpm run codegen
git diff --exit-code packages/nexus-contracts/src/generated/ crates/nexus-contracts/src/generated/
```

### 2. Schema consistency and wire drift

```bash
bash tooling/check-schema-drift.sh
bash tooling/check-wire-drift.sh
```

### 3. Rust: format, lint, sqlx offline check, tests

Formatting uses the **pinned** nightly rustfmt:

```bash
cargo +nightly-2026-06-26 fmt --all -- --check
cargo clippy --all -- -D warnings
SQLX_OFFLINE=true cargo check --all --all-targets
SQLX_OFFLINE=true cargo test --all
```

To apply formatting locally (instead of check-only):

```bash
cargo +nightly-2026-06-26 fmt --all
```

### 4. TypeScript

`pnpm run codegen` already builds `@42ch/nexus-contracts`. Then:

```bash
pnpm run typecheck
pnpm run build
pnpm run test
```

When you only touch `apps/web` or UI guardrails:

```bash
pnpm run build:web
pnpm run test:web
bash tooling/check-ui-guardrails.sh
```

CI also runs dedicated `web` and `@42ch/nexus-ui` jobs; match those when your change is limited to those packages.

## Code style

- **Rust:** `cargo +nightly-2026-06-26 fmt --all`; `cargo clippy --all -- -D warnings`. Fix all clippy warnings. Do not add `#[allow(...)]` without a brief justification comment.
- **TypeScript:** Strict mode in package tsconfigs. Run `pnpm run typecheck` when you touch TS; run scoped app tests when you change `apps/web` or `apps/design-studio`.

## Testing expectations

- **Rust:** unit and integration tests for non-trivial behavior; `cargo test -p <crate>` while iterating, `cargo test --all` before PR.
- **TypeScript:** Vitest in `apps/web`, `apps/design-studio`, and `@42ch/nexus-ui`; `pnpm run test` or scoped `pnpm run test:web` / `pnpm run test:design-studio`.
- Prefer extending existing test patterns over adding trivial assertions.

## Branching and PRs

- Branch from `main`.
- Use clear branch names, for example `feature/<short-name>` or `fix/<short-name>`.
- Keep PRs focused: one feature or fix per PR when practical.
- Update contributor or user docs when workflows or behavior change.
- Ensure CI is green before requesting review.

### Documentation-only changes

CI ignores pushes that only touch paths such as `docs/**` or certain `README.md` / `AGENTS.md` files (see `paths-ignore` in `.github/workflows/ci.yml`). Doc-only PRs may not run the full pipeline automatically; note in the PR description if reviewers should run checks locally.

## Security and dependencies

- Prefer minimal, well-maintained dependencies.
- Do not commit secrets or machine-specific credentials.

## Where to put documentation

- **Stable, clone-ready docs** (install, architecture, codegen, contributing): `docs/` and root `README.md` (**Development** section for maintainer commands; **Quick Start** is reserved for end-user docs).
- **Plan-specific design notes and review artifacts**: `.mstar/knowledge/` — see [`AGENTS.md`](../AGENTS.md).

## Questions

Open an issue for bugs or feature discussion, or ask in the project’s preferred chat channel if one is listed in the repository README.
