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

## Workflow

1. **Load schemas** from `schemas/**/*.schema.json`
2. **Validate** each schema has required fields (`$schema`, `$id`, `schema_version`, `title`, `type`)
3. **Parse common types** from `schemas/common/common.schema.json` definitions
4. **Generate TypeScript** interfaces in `packages/nexus-contracts/src/generated/`
5. **Generate Rust** structs in `crates/nexus-contracts/src/generated/`

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
├── index.ts              # Re-exports all types + SCHEMA_VERSIONS
├── CommonTypes.ts        # Shared types (type aliases, enums, SourceAnchor)
├── Bundle.ts             # DeltaBundle envelope
├── Creator.ts            # Creator entity
├── World.ts              # World entity
├── KeyBlock.ts           # KeyBlock entity
├── TimelineEvent.ts      # TimelineEvent entity
├── Memory.ts             # MemoryItem entity
├── SyncCommand.ts        # SyncCommand entity
├── OutboxEntry.ts        # OutboxEntry entity
├── WorldMembership.ts    # WorldMembership entity
├── Pairing.ts            # Pairing entity
├── StoryManifest.ts      # StoryManifest entity
├── VersionRef.ts         # VersionRef value object
└── Meta.ts               # Meta schema
```

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
