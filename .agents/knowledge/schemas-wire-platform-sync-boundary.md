# Schemas — Wire vs Platform Sync Boundary

**Status**: Active — in-repo pointer (full audit table archived)
**Supersedes**: — (companion to archived [schemas-boundary.md](../archived/knowledge/schemas-boundary.md))
**Aligned with**: `nexus` `schemas/AGENTS.md`, `crates/nexus-contracts/src/local/`

---

## Rule (authoritative)

A JSON Schema file belongs in `schemas/` **only if** `nexus-platform` observes the type on a **wire** boundary:

- Platform HTTP BFF bodies (`schemas/platform/*`)
- CLI ↔ platform sync payloads (`schemas/cli-sync/*`, bundle/delta types in `schemas/domain/*` carried on sync)
- Any payload the OSS CLI/daemon sends to platform that platform must parse

Everything else is **local**: hand-written Rust under `crates/nexus-contracts/src/local/` — **no** `pnpm run codegen` entry in `@42ch/nexus-contracts` npm surface for platform.

**Corollary**: `/v1/local/*` daemon API DTOs, orchestration schedules, ACP registry manifest, worker IPC, SQLite row shapes → **local**, not `schemas/`. See [creator-schedule-and-core-context.md](creator-schedule-and-core-context.md) §9.

## Directory layout (normative)

Folder names, product-line mapping, and rename policy (`cli-sync/` → target `cloud-sync/`): **[specs/schemas-directory-layout.md](specs/schemas-directory-layout.md)**. On-disk index: [schemas/README.md](../../schemas/README.md).

## What still lives in `schemas/` today (2026-05)

| Tree | Platform sync? | Notes |
| --- | --- | --- |
| `schemas/platform/*` | **Yes** | Platform HTTP request/response contracts (flat; prefix grouping) |
| `schemas/cli-sync/*` | **Yes** | Bundle envelope, pull, conflict — cloud sync wire (target dir name `cloud-sync/`) |
| `schemas/domain/*` | **Yes** | Wire entities in bundles / platform bodies — **not** the Rust `nexus-domain` crate |
| `schemas/common/*` | **Yes** (when `$ref`'d by wire) | Shared identifiers and value objects |
| `schemas/meta/` | **No** | README pointer only; `Meta` type is `src/local/meta.rs` |
| *(removed from `schemas/`)* | **No** | `acp-runtime/`, `outbox_entry`, `daemon_status_v2`, `registry_manifest`, schedule/orchestration HTTP types → `src/local/` |

V1.20 removed **daemon local HTTP proxies** for `world/*` and `explore/*`; those operations use **platform HTTP** directly. The `schemas/platform/world-*` and `schemas/platform/explore-*` files remain **wire** for platform — they were never “daemon-only” contracts.

## Drift / housekeeping

- **Stale README risk**: `schemas/domain/README.md` may list schemas that already moved to `local/` — verify against `schemas/domain/*.json` on disk.
- **Codegen**: only files under `schemas/` generate TS in `@42ch/nexus-contracts`; platform upgrades follow npm semver + `schema_version`.
- **Full 64-file audit table**: [archived/knowledge/schemas-boundary.md](../archived/knowledge/schemas-boundary.md) §5.2 (53 wire / 10 local at audit time). Re-run audit before further moves; `rg <TypeName>` on `nexus-platform` before deleting generated TS.

## Related

- [local-cloud-crate-architecture.md](specs/local-cloud-crate-architecture.md) — local vs cloud product lines, crate graph, daemon API classes
- [archived/knowledge/daemon-api-workspace-write-architecture.md](../archived/knowledge/daemon-api-workspace-write-architecture.md) — **superseded** route table (use V1.20 compass + `daemon-runtime` / crate architecture SSOT)
- [v1.20-delivery-compass-v1.md](../iterations/v1.20-delivery-compass-v1.md) — shipped local API redesign
- [v1.21-local-platform-isolation-delivery-compass-v1.md](../iterations/v1.21-local-platform-isolation-delivery-compass-v1.md) — daemon sync route removal program

---

*Created: 2026-05-20. Pointer doc; do not duplicate the archived audit table here.*
