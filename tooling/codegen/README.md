# Nexus Codegen Tool

Schema-to-code generation pipeline for Nexus wire contracts.

## Purpose

Transform JSON Schema files in `schemas/` into:
- TypeScript types (`packages/nexus-contracts/src/generated/`)
- Rust types (`crates/nexus-contracts/src/generated/`)

## Usage

```bash
# Run full codegen pipeline (build + generate)
pnpm run codegen

# Watch mode (regenerate on schema changes)
pnpm run codegen:watch

# Build codegen tool only
cd tooling/codegen && npm run build
```

## Codegen targets

| Item | Path / Command |
|---|---|
| Source schemas | `schemas/**/*.schema.json` |
| Regenerate command | `pnpm run codegen` |
| Validate command | `pnpm run validate-schemas` |
| Wire-drift check | `./tooling/check-wire-drift.sh` |
| TypeScript destination | `packages/nexus-contracts/src/generated/` |
| Rust destination | `crates/nexus-contracts/src/generated/` |
| Web app consumption | `@42ch/nexus-contracts` package (published from `packages/nexus-contracts/`) |

There is no `apps/web/src/api-types/` directory. The web app imports all wire DTOs from the generated `@42ch/nexus-contracts` package.

## Architecture

The pipeline is a port of the sibling spoke repo's codegen orchestrator (spoke base URI →
`nexus42.invalid`). Three stages run in sequence (orchestrator: `src/index.ts`):

| Stage | Module | Engine | Output |
|-------|--------|--------|--------|
| 1. prep | `schema-prep.ts` | `@apidevtools/json-schema-ref-parser` | localized + dereferenced schema trees |
| 2. ts-gen | `ts-gen.ts` | `json-schema-to-typescript` | `packages/nexus-contracts/src/generated/` |
| 3. rust-gen | `rust-generator.ts` | bespoke (hand-tuned) | `crates/nexus-contracts/src/generated/` |

**TypeScript is library-driven** (`json-schema-to-typescript`). Stage 1 rewrites each
schema's base-URI `$id`/`$ref` into POSIX-relative paths under `.schemas-localized/`; stage 2
then compiles each non-skip schema via the library with the `title` overridden to the
basename-derived PascalCase type name, emitting a nested tree that mirrors `schemas/`.

**Rust is still bespoke** (`rust-generator.ts`, hand-tuned) and consumes `loadAllSchemas()`
directly. It will be superseded by a `typify`-based generator over the dereferenced tree
(`.schemas-dereferenced/`, produced by stage 1) — tracked as **P1**.

## Workflow

1. **prep** — localize every schema's base-URI `$id`/`$ref` to POSIX-relative paths
   (`.schemas-localized/`), then dereference cross-file `$ref` via `json-schema-ref-parser`
   (`.schemas-dereferenced/`). Shared by TS generation and the future typify-based Rust path.
2. **ts-gen** — compile each non-skip schema with `json-schema-to-typescript` (title
   overridden to the basename-derived PascalCase name) → nested tree under
   `packages/nexus-contracts/src/generated/`, plus the `SCHEMA_VERSIONS` / `LATEST_SCHEMA_VERSION` stamp.
3. **rust-gen** — bespoke hand-tuned generator over `loadAllSchemas()` → nested module tree
   under `crates/nexus-contracts/src/generated/`. *(P1: replace with typify over the deref tree.)*

Schema validation is a separate concern: run `pnpm run validate-schemas` (not part of the
codegen pipeline itself).

## Schema Handling

- **Common types** (`common.schema.json`, `source-anchor.schema.json`): Extracted into `CommonTypes.ts` / `common_types.rs` — no standalone struct generated
- **Domain schemas** (`domain/*.schema.json`): Each generates a TypeScript interface and Rust struct
- **Platform** (`platform/*.schema.json`) and **cloud sync** (`cloud-sync/*.schema.json`): Same — one struct per schema file
- **Cloud-sync bundle refinement** (`cloud-sync/bundle.schema.json`): Skipped for struct generation (canonical `Bundle` from `domain/bundle.schema.json`; see `schema-loader.ts`)
- **Meta schema**: Not in `schemas/` — hand-written `crates/nexus-contracts/src/local/meta.rs` only

## Type Mapping

| JSON Schema | TypeScript | Rust |
|---|---|---|
| `integer` (schema_version) | `number` | `u32` |
| `integer` (min 0) | `number` | `u64` |
| `integer` | `number` | `i64` |
| `number` | `number` | `f64` |
| `string` | `string` | `String` |
| `string` + `enum` | `'a' \| 'b' \| ...` | `enum` |
| `boolean` | `boolean` | `bool` |
| `array` | `T[]` | `Vec<T>` |
| `$ref` (common def) | type alias | type alias |
| `$ref` (common enum) | union type | `enum` |
| `$ref` (SourceAnchor) | `SourceAnchor` | `SourceAnchor` |
| `["string", "null"]` | `string \| null` | `Option<String>` |

## Output Structure

### TypeScript
```
packages/nexus-contracts/src/generated/
├── index.ts                  # export * from each subdir + SCHEMA_VERSIONS / LATEST_SCHEMA_VERSION
├── common/
│   ├── index.ts              # named re-exports
│   ├── CommonTypes.ts        # common.schema.json + source-anchor.schema.json (skip-listed but referenced)
│   └── version-ref.ts
├── domain/                   # one <base>.ts per schema + a subdir index.ts barrel
│   ├── index.ts
│   ├── world.ts
│   ├── creator.ts
│   └── …
├── platform/
│   ├── http-bff/             # index.ts + <base>.ts …
│   └── sync/                 # index.ts + <base>.ts …
└── daemon-api/               # nested canvas/ works/ worlds/ … each with its own index.ts
```

The tree mirrors the consumer-scope `schemas/` layout (folder names preserved as written,
e.g. `http-bff`). Each consumer-scope subdir gets an `index.ts` barrel of **named** root
exports (one per file, `export type { TypeName } from './<base>'`) so inline
`declareExternallyReferenced` declarations do not collide when barrel-re-exported. The root
`index.ts` does `export * from './<subdir>'` for each subdir, keeping the package public API flat.

### Rust
```
crates/nexus-contracts/src/generated/
├── mod.rs                # Module declarations + SCHEMA_VERSIONS
├── common_types.rs       # Shared types and enums
├── bundle.rs             # DeltaBundle envelope struct
├── creator.rs            # Creator entity struct
├── world.rs              # World entity struct
├── key_block.rs          # KeyBlock entity struct
├── timeline_event.rs     # TimelineEvent entity struct
├── memory.rs             # MemoryItem entity struct
├── sync_command.rs       # SyncCommand entity struct
├── outbox_entry.rs       # OutboxEntry entity struct
├── world_membership.rs   # WorldMembership entity struct
├── pairing.rs            # Pairing entity struct
├── story_manifest.rs     # StoryManifest entity struct
├── version_ref.rs        # VersionRef value object struct
└── meta.rs               # Meta schema struct
```

## Do Not Modify Generated Types

All generated files have headers: `AUTO-GENERATED - DO NOT MODIFY`

To change types:
1. Update schema in `schemas/`
2. Run `pnpm run codegen`
3. Commit schema + generated changes together

## CI Integration

CI workflow (`validate-schemas` → `verify-codegen` → `rust-checks` + `typescript-checks`) ensures:
- Schemas are valid before codegen
- Generated types compile in both TypeScript and Rust
- Generated files are archived as artifacts
