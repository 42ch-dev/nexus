# Schemas — JSON Schema Wire Contracts

This directory is the **single truth source** for all Nexus wire types. Everything else (Rust types, TypeScript types) is generated from here.

## Schema URI Placeholder

Committed schema files use `https://nexus42.invalid` in `$id` / `$ref` paths (RFC 6761 reserved name; production domain TBD).

- In prose docs, use `{NEXUS42_BASE_URL}` as the origin placeholder.
- Do **not** embed `{NEXUS42_BASE_URL}` inside JSON `$id` / `$ref` strings.

See `schemas/meta/README.md` and `docs/CODEGEN.md`.

## Codegen Flow

```
schemas/  →  pnpm run codegen  →  crates/nexus-contracts/src/generated/  (Rust)
                                  packages/nexus-contracts/src/generated/  (TypeScript)
```

- JSON Schema (`schemas/`) → single codegen pass → Rust + TypeScript
- The npm package (`@42ch/nexus-contracts`) must be published and version-locked with `schema_version`; the Rust crate is monorepo-internal

## ⚠️ Mandatory: Run Codegen After Any Schema Change

The CI job `verify-codegen` runs `pnpm run codegen` and then checks `git diff` on the generated output directories. If generated files are out of sync with committed versions, **CI will fail**.

**Rule:** any commit that touches files under `schemas/` MUST also include the corresponding regenerated output. Before committing:

```bash
pnpm run codegen
git add packages/nexus-contracts/src/generated/ crates/nexus-contracts/src/generated/
```

Do NOT hand-edit files under `*/generated/` — always regenerate from schemas.

## `enum_conversions.rs` (Manual Companion)

`crates/nexus-contracts/src/enum_conversions.rs` is maintained **next to** generated types, not produced by codegen. When JSON Schema adds or renames enum values, update this file in the same commit as regenerated `src/generated/` and verify with `cargo test -p nexus-contracts`.

## Wire `schema_version`

- `**LATEST_SCHEMA_VERSION`:** `**1`** — constant emitted by codegen into both Rust and TypeScript generated modules.
- Individual DTOs carry a per-type `schema_version`; the **bundle envelope** and tooling align on the latest value after `pnpm run codegen`.
