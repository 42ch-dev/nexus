# Nexus Architecture

## Monorepo Architecture

### Truth Source: JSON Schema

All wire contracts defined in `schemas/` directory.

**Code Generation Flow:**

```
schemas/*.json → codegen → Rust (crates/nexus-contracts) + TypeScript (packages/nexus-contracts)
```

**Why JSON Schema?**

- Single source of truth for DTOs
- Automatic type generation for both languages
- Version-locked contracts (`schema_version` field)
- Easy validation and testing

### Rust Workspace

**Members:**

- `nexus-contracts`: Generated wire types (library crate)
- `nexus42` (future): CLI executable
- `nexus42d` (future): Daemon/supervisor
- `nexus-sync` (future): Bundle/outbox state machine

**Design Principles:**

- Use official ACP Rust SDK
- Share generated contract types
- Client-only (not ACP agent/server)

### TypeScript Workspace

**Packages:**

- `@42ch/nexus-contracts`: Generated wire types (npm package)

**Design Principles:**

- Consumed by private `nexus-platform` repo
- No handwritten second DTO source
- All types come from this repo's schemas

## Versioning

- Schema contracts use `schema_version` field
- CLI SemVer reflects breaking wire changes
- npm package major bump → coordinated update

## Constraints

- **Do not** treat `nexus42d` as ACP Agent/Server - it's client-only
- **Do not** sync full manuscript text by default - only deltas/bundles
- **World history is immutable** - changes via Fork only
- **Wire contracts must match schemas** - no drift

