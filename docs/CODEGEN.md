# Schema Code Generation

Nexus uses **JSON Schema as single truth source** for wire contracts.

## Philosophy

**One schema, two languages:**

```
schemas/*.schema.json → TypeScript + Rust types
```

All wire types are **generated**, not handwritten. This ensures:
- Consistency across CLI and platform
- Schema-driven versioning
- Automatic validation support
- No drift between implementations

## How It Works

### Define Schema

Write JSON Schema in `schemas/domain/*.schema.json`.

**Schema URIs:** committed files use `https://nexus42.invalid/schemas/...` (valid URI placeholder, RFC 6761). In documentation for a future public deployment, the same path is written as **`{NEXUS42_BASE_URL}/schemas/...`** (origin only, no trailing slash). Layout: [schemas/README.md](../schemas/README.md).

```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "$id": "https://nexus42.invalid/schemas/domain/bundle.schema.json",
  "schema_version": 1,
  "title": "Nexus Bundle Envelope",
  "type": "object",
  "required": ["schema_version", "bundle_id", "world_id"],
  "properties": {
    "schema_version": { "type": "integer", "const": 1 },
    "bundle_id": { "type": "string" },
    "world_id": { "$ref": "https://nexus42.invalid/schemas/common/common.schema.json#/definitions/WorldId" }
  }
}
```

### Run Codegen

```bash
pnpm run codegen
```

### Generated Output

**TypeScript:** `packages/nexus-contracts/src/generated/Bundle.ts`

```typescript
import type { SchemaVersion } from './CommonTypes';

export interface Bundle {
  schema_version: number;
  bundle_id: string;
  world_id: string;
}
```

**Rust:** `crates/nexus-contracts/src/generated/bundle.rs`

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct Bundle {
    pub schema_version: u32,
    pub bundle_id: String,
    pub world_id: String,
}
```

## Key Design Decisions

- `schema_version` is always `integer` → TypeScript `number`, Rust `u32`
- Common enums use `#[serde(rename_all = "snake_case")]` in Rust
- Rust reserved words (e.g., `type`) use `r#` prefix + serde rename
- `$`-prefixed fields use `dollar_` prefix + serde rename in Rust
- SourceAnchor is embedded in CommonTypes (not a standalone struct)

## Versioning

Schema version (`schema_version`) is embedded in generated types.

**Version bump rules:**
- **Major**: Breaking field changes
- **Minor**: New optional fields
- **Patch**: Documentation only

## Development Workflow

1. Update schema in `schemas/`
2. Run `pnpm run validate-schemas` (validate first)
3. Run `pnpm run codegen` (generate types)
4. Verify: `cargo check --workspace` (Rust)
5. Verify: `pnpm run typecheck` (TypeScript)
6. Implement features using generated types
7. Commit schema + generated changes together

## Never Edit Generated Files

Generated files have header: `AUTO-GENERATED - DO NOT MODIFY`

Edit schemas instead, then regenerate.

## CI Pipeline

CI ensures generated types match schemas:
1. `validate-schemas`: Validate JSON Schema syntax
2. `verify-codegen`: Run codegen and verify output
3. `rust-checks`: cargo fmt + clippy
4. `typescript-checks`: tsc --noEmit

If codegen fails, CI fails — no drift allowed.
