# `schemas/` — Wire JSON Schema Tree

**Authority:** [`.agents/knowledge/specs/schemas-directory-layout.md`](../.agents/knowledge/specs/schemas-directory-layout.md) (folder layout) + [`.agents/knowledge/schemas-wire-platform-sync-boundary.md`](../.agents/knowledge/schemas-wire-platform-sync-boundary.md) (wire vs local rule).

**Local-only types** live in `crates/nexus-contracts/src/local/` — not here.

## Layout

| Directory | Files (approx.) | Purpose |
| --- | --- | --- |
| [common/](common/) | 3 | Shared IDs, enums, `SourceAnchor`, `VersionRef` |
| [domain/](domain/) | 12 | Wire entities (Creator, World, Bundle, Delta, …) |
| [platform/](platform/) | 33 | Platform HTTP request/response bodies |
| [cli-sync/](cli-sync/) | 4 | Sync bundle / pull / conflict (cloud line; target rename → `cloud-sync/`) |
| [meta/](meta/) | 0 JSON | Pointer — meta-schema is Rust-only in `src/local/meta.rs` |

## Commands

```bash
pnpm run validate-schemas
pnpm run codegen
./tooling/check-wire-drift.sh
```

After any edit under `schemas/`, run **codegen** and commit `crates/nexus-contracts/src/generated/` and `packages/nexus-contracts/src/generated/`.

## Product lines

| Line | Uses `schemas/`? |
| --- | --- |
| **Cloud** (CLI `sync` / `platform`, `nexus-cloud-sync`) | **Yes** — `platform/`, `cli-sync/`, `domain/`, `common/` |
| **Local** (daemon `/v1/local/*`, orchestration, ACP) | **No** — `nexus-contracts/src/local/` |

See [local-cloud-crate-architecture.md](../.agents/knowledge/specs/local-cloud-crate-architecture.md).
