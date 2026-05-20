# `schemas/` — Wire JSON Schema Tree

**Authority:** [`.agents/knowledge/specs/schemas-directory-layout.md`](../.agents/knowledge/specs/schemas-directory-layout.md) (folder layout) + [`.agents/knowledge/schemas-wire-platform-sync-boundary.md`](../.agents/knowledge/schemas-wire-platform-sync-boundary.md) (wire vs local rule).

**Local-only types** live in `crates/nexus-contracts/src/local/` — not under `schemas/`.

## Layout

| Directory | Files (approx.) | Purpose |
| --- | --- | --- |
| [common/](common/) | 3 | Shared IDs, enums, `SourceAnchor`, `VersionRef` |
| [domain/](domain/) | 12 | Wire entities (Creator, World, Bundle, Delta, …) |
| [platform/](platform/) | 33 | Platform HTTP request/response bodies |
| [cloud-sync/](cloud-sync/) | 4 | Sync bundle / pull / conflict (`nexus-cloud-sync` wire) |

**Removed paths (do not recreate):**

- `schemas/acp-runtime/` — → `crates/nexus-contracts/src/local/acp_runtime/`
- `schemas/meta/` — meta-schema → `crates/nexus-contracts/src/local/meta.rs`
- `schemas/cli-sync/` — renamed **`cloud-sync/`** (2026-05-20)

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
| **Cloud** (CLI `sync` / `platform`, `nexus-cloud-sync`) | **Yes** — `platform/`, `cloud-sync/`, `domain/`, `common/` |
| **Local** (daemon `/v1/local/*`, orchestration, ACP) | **No** — `nexus-contracts/src/local/` |

See [local-cloud-crate-architecture.md](../.agents/knowledge/specs/local-cloud-crate-architecture.md).
