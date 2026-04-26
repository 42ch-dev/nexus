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

## Wire/Local Schema Drift Detection (WS-D)

A test-time CI gate (`cargo test --test schema_drift_detection`) validates that
all registered JSON Schema files match their corresponding Rust struct definitions.
This catches drift when a schema is modified but the generated (or local) Rust types
are not updated.

### How to Register a New Schema

When adding a new schema file to `schemas/`:

1. Create the `.schema.json` file and run `pnpm run codegen` to generate Rust/TypeScript types
2. Add a new entry to `build_schema_map()` in `crates/nexus-contracts/tests/schema_drift_detection.rs`
   using the `entry!` macro:

   ```rust
   // Single struct per schema:
   entry!("schemas/domain/my-type.schema.json", Strict, MyType),

   // Multiple structs from one schema:
   entry!("schemas/domain/my-type.schema.json", Strict, [MyType, MySubType]),
   ```

3. Run `./tooling/check-wire-drift.sh` to verify the new entry passes

### Check Modes

- **`Strict`** (default for wire types): Every schema property must have a corresponding
  field in the Rust struct, and every Rust serialized field must be declared in the schema.
  Use for all generated wire DTOs.
- **`Subset`** (for local-only types): Only required schema fields are enforced. The Rust
  struct may have extra internal fields that don't appear in the schema.
  Use when local types intentionally extend the wire contract shape.

### When Drift Is Detected

The test reports the specific schema file, struct name, and field(s) that mismatch:

```
SCHEMA DRIFT DETECTED
  [schemas/domain/world.schema.json] World: MISSING field 'new_field' (type: string)
  [schemas/domain/world.schema.json] World: EXTRA field 'old_field' not in schema
```

Fix the drift by either updating the schema or regenerating types, then run
`./tooling/check-wire-drift.sh` to confirm.

## Wire `schema_version`

- `**LATEST_SCHEMA_VERSION`:** `**1`** — constant emitted by codegen into both Rust and TypeScript generated modules.
- Individual DTOs carry a per-type `schema_version`; the **bundle envelope** and tooling align on the latest value after `pnpm run codegen`.
