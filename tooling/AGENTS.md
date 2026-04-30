# Tooling — Codegen Pipeline & CI

## Pre-merge / PR Checklist

Before merging, run the same checks as CI (`.github/workflows/ci.yml`):

1. `pnpm run validate-schemas` — validate JSON Schema files
2. `pnpm run codegen` then `git diff --exit-code packages/nexus-contracts/src/generated/ crates/nexus-contracts/src/generated/` — generated output must match committed versions
3. `./tooling/check-wire-drift.sh` — wire schema drift detection (runs `cargo test --test schema_drift_detection`)
4. `cargo +nightly fmt --all -- --check` — nightly rustfmt required
5. `cargo clippy --all -- -D warnings` — lint warnings fail CI
6. `pnpm run typecheck` — TypeScript contract package

## Wire Schema Drift Detection

`./tooling/check-wire-drift.sh` validates that every registered JSON Schema matches its generated Rust struct. Add new schemas to `build_schema_map()` in `crates/nexus-contracts/tests/schema_drift_detection.rs`. Use `CheckMode::Strict` for wire types, `CheckMode::Subset` for local-only superset types.

## Rust Formatting

- Use `cargo +nightly fmt --all` before commit. Stable `cargo fmt` cannot apply `.rustfmt.toml`'s `ignore` field and would incorrectly reformat generated code under `crates/nexus-contracts/src/generated/`.
- CI does not run `cargo test`; run `cargo test --all` locally when touching Rust behavior.
