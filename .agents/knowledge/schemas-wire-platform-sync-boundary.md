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

## What still lives in `schemas/` today (2026-05)

| Tree | Platform sync? | Notes |
| --- | --- | --- |
| `schemas/platform/*` | **Yes** | Platform API request/response contracts |
| `schemas/cli-sync/*` | **Yes** | Bundle envelope, pull, conflict — sync protocol |
| `schemas/domain/*` (remaining files) | **Yes** | Entities embedded in bundles / platform domain model |
| `schemas/common/*`, `schemas/meta/*` | **Yes** (if `$ref`'d by wire) | Shared value objects |
| *(removed from `schemas/`)* | **No** | Moved to `src/local/` — e.g. `outbox_entry`, `daemon_status_v2`, `registry_manifest`, schedule HTTP types, orchestration preset types |

V1.20 removed **daemon local HTTP proxies** for `world/*` and `explore/*`; those operations use **platform HTTP** directly. The `schemas/platform/world-*` and `schemas/platform/explore-*` files remain **wire** for platform — they were never “daemon-only” contracts.

## Drift / housekeeping

- **Stale README risk**: `schemas/domain/README.md` may list schemas that already moved to `local/` — verify against `schemas/domain/*.json` on disk.
- **Codegen**: only files under `schemas/` generate TS in `@42ch/nexus-contracts`; platform upgrades follow npm semver + `schema_version`.
- **Full 64-file audit table**: [archived/knowledge/schemas-boundary.md](../archived/knowledge/schemas-boundary.md) §5.2 (53 wire / 10 local at audit time). Re-run audit before further moves; `rg <TypeName>` on `nexus-platform` before deleting generated TS.

## Related

- [daemon-api-workspace-write-architecture.md](daemon-api-workspace-write-architecture.md) — local API vs workspace writes (route inventory → V1.20 compass)
- [v1.20-delivery-compass-v1.md](../iterations/v1.20-delivery-compass-v1.md) — shipped local API redesign

---

*Created: 2026-05-20. Pointer doc; do not duplicate the archived audit table here.*
