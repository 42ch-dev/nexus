# Contributing to Nexus

Thank you for helping improve Nexus. This document describes how to work in this monorepo, align your changes with CI, and open pull requests. For repository layout, naming, and agent-oriented conventions, see [`AGENTS.md`](../AGENTS.md) at the repository root.

## Code of conduct

Be respectful, constructive, and inclusive.

## Prerequisites

- **Node.js** 20 or newer (`engines.node` in root `package.json`)
- **pnpm** 8 or newer (CI uses pnpm 9)
- **Rust** stable with `rustfmt` and `clippy` components
- **Nightly `rustfmt`** — required for `cargo fmt` in this workspace (codegen output uses workspace `.rustfmt.toml` rules). Install with:

  ```bash
  rustup toolchain install nightly --component rustfmt
  ```

## Getting started

```bash
git clone https://github.com/42ch/nexus.git
cd nexus
pnpm install --frozen-lockfile
```

## Schema-first development

JSON Schemas under `schemas/` are the source of truth. TypeScript and Rust contract types are generated; do not hand-edit generated files.

1. Edit or add schemas in `schemas/` (JSON Schema).
2. Validate schemas: `pnpm run validate-schemas`
3. Regenerate types: `pnpm run codegen`
4. Implement features against the generated contracts in `packages/nexus-contracts/` and `crates/nexus-contracts/`.
5. Add or update tests.
6. **Commit schema changes and all generated output together** in the same commit(s) so CI’s “generated code up to date” check passes.

Generated paths checked in CI include:

- `packages/nexus-contracts/src/generated/`
- `crates/nexus-contracts/src/generated/`

## Local checks (mirror CI)

Running these before pushing reduces CI surprises. Order matches the main pipeline in [`.github/workflows/ci.yml`](../.github/workflows/ci.yml).

### 1. Schemas and codegen

```bash
pnpm run validate-schemas
pnpm run codegen
git diff --exit-code packages/nexus-contracts/src/generated/ crates/nexus-contracts/src/generated/
```

If the last command prints a diff, run `pnpm run codegen` again, review, and commit the updates.

### 2. Schema consistency

```bash
bash tooling/check-schema-drift.sh
```

### 3. Rust: format, lint, sqlx offline check, tests

Formatting uses **nightly** rustfmt for the whole workspace:

```bash
cargo +nightly fmt --all
cargo clippy --all -- -D warnings
SQLX_OFFLINE=true cargo check --all --all-targets
SQLX_OFFLINE=true cargo test --all
```

To match CI exactly for formatting (check-only):

```bash
cargo +nightly fmt --all -- --check
```

### 4. TypeScript

After codegen, build the contracts package and typecheck all workspaces:

```bash
pnpm --filter @42ch/nexus-contracts run build
pnpm run typecheck
```

### Optional: full script

You can chain the above in one shell session when preparing a PR; stop and fix at the first failure.

## Code style

- **Rust:** `cargo +nightly fmt --all`; `cargo clippy --all -- -D warnings`. Fix all clippy warnings.
- **TypeScript:** Strict TypeScript (`strict: true` in package tsconfigs). Run `pnpm run typecheck` after contract builds when you touch TS.

## Testing expectations

- Rust crates should include unit tests where behavior is non-trivial.
- TypeScript packages should remain type-safe under `pnpm run typecheck`; add tests where the project already uses them for contracts or tooling.
- Prefer integration or higher-level tests for critical paths when they already exist in the crate or package.

## Branching and PRs

- Branch from `main`.
- Use clear branch names, for example `feature/<short-name>` or `fix/<short-name>`.
- Keep PRs focused: one feature or fix per PR when practical.
- Update user-facing or contributor docs when behavior or workflows change.
- Ensure CI is green on your branch before requesting review.

### Documentation-only changes

CI ignores pushes that only touch paths such as `docs/**` or certain `README.md` / `AGENTS.md` files (see `paths-ignore` in `.github/workflows/ci.yml`). Doc-only PRs may not run the full pipeline automatically; mention that in the PR description if reviewers should run checks locally.

## Security and dependencies

- Prefer minimal, well-maintained dependencies.
- Do not commit secrets or machine-specific credentials.

## Where to put documentation

- **Stable, clone-ready docs** (install, architecture overview, codegen, contributing): `docs/` and root `README.md`.
- **Plan-specific design notes and review artifacts**: `.mstar/knowledge/` — see [`AGENTS.md`](../AGENTS.md) for boundaries and indexing.

## Questions

Open an issue for bugs or feature discussion, or ask in the project’s preferred chat channel if one is listed in the repository README.
