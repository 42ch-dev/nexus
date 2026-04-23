# Tooling — Codegen Pipeline & CI

This directory contains the codegen pipeline (`tooling/codegen/`) that transforms JSON Schema into Rust and TypeScript types.

## Schema Validation

```bash
pnpm run validate-schemas
```

## Codegen

```bash
pnpm run codegen
```

Verify output matches committed versions:

```bash
git diff --exit-code packages/nexus-contracts/src/generated/ crates/nexus-contracts/src/generated/
```

## Pre-merge / PR Checklist

Run the same checks as the CI workflow (`.github/workflows/ci.yml`) so local results match GitHub Actions:

```bash
# 1) JSON Schemas
pnpm run validate-schemas

# 2) Codegen matches committed output (must produce no diff)
pnpm run codegen
git diff --exit-code packages/nexus-contracts/src/generated/ crates/nexus-contracts/src/generated/

# 3) Rust formatting (nightly rustfmt required)
cargo +nightly fmt --all -- --check

# 4) Rust lints (warnings fail CI)
cargo clippy --all -- -D warnings

# 5) TypeScript contract package
pnpm install   # if needed
pnpm run typecheck
```

CI does not run `cargo test`; run `cargo test --all` locally when you touch Rust behavior.

## Rust Formatting

- Use `cargo +nightly fmt --all` before commit.
- The workspace `.rustfmt.toml` ignores `crates/nexus-contracts/src/generated/` (stable `cargo fmt` cannot apply `ignore`, and formatting generated Rust would desync CI `verify-codegen` from `pnpm run codegen`).
- Install once: `rustup toolchain install nightly --component rustfmt`
