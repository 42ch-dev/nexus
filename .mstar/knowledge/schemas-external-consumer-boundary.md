# Schemas — External-Consumer Boundary

**Status**: Active — in-repo pointer (full audit table archived)
**Supersedes**: `schemas-wire-platform-sync-boundary.md` (renamed 2026-06-23, V1.62 P0; same file, expanded scope). Companion to archived [schemas-boundary.md](../archived/knowledge/schemas-boundary.md).
**Aligned with**: `nexus` `schemas/AGENTS.md`, `crates/nexus-contracts/src/local/`

---

## Rule (authoritative)

A JSON Schema file belongs in `schemas/` **only if it is consumed by an external client** — either `nexus-platform` (wire) **OR** an external Local API client (e.g. the future WebApp/Web-UI). Concretely, a schema belongs here when **at least one** of these holds:

- **Platform wire** — `nexus-platform` observes the type on a wire boundary:
  - Platform HTTP BFF bodies (`schemas/platform/http-bff/*`)
  - CLI ↔ platform sync payloads (`schemas/platform/sync/*`)
  - Any payload the OSS CLI/daemon sends to platform that platform must parse
- **Local API** — an **external** client (separate process / language boundary) consumes the type via the daemon Local API:
  - Compute module ABI envelopes (`schemas/local-api/compute/*`) — consumed by external WASM compute modules and the future WebApp/Web-UI.
  - Core CRUD Local API schemas (`schemas/local-api/{works,kb,findings,schedule,workspace,creators}/*`) — consumed by the future WebApp/Web-UI (V1.63 P1). These were promoted from inline handler DTOs to cross-language JSON Schema so the TypeScript web-client can consume typed request/response shapes without duplicating Rust handler definitions.

Everything else is **local**: hand-written Rust under `crates/nexus-contracts/src/local/` — **no** `pnpm run codegen` entry in `@42ch/nexus-contracts` npm surface for those types.

**Corollary**: `/v1/local/*` daemon API DTOs, orchestration schedules, ACP registry manifest, worker IPC, SQLite row shapes → **local**, not `schemas/`. See [creator-schedule-and-core-context.md](specs/creator-schedule-and-core-context.md) §9. The daemon's own internal Local API request/response shapes stay local **unless** an external client (e.g. WebApp) must consume them cross-language — at which point they migrate into `schemas/local-api/<concern>/`.

## Directory layout (normative)

Folder names, consumer-scope tree, and product-line mapping: **[specs/schemas-directory-layout.md](specs/schemas-directory-layout.md)**. On-disk index: [schemas/README.md](../../schemas/README.md).

## What still lives in `schemas/` today (2026-06, post-V1.62 P0)

| Tree | External consumer? | Notes |
| --- | --- | --- |
| `schemas/platform/http-bff/*` | **Yes** — `nexus-platform` | Platform HTTP request/response contracts (was flat `schemas/platform/*` pre-V1.62) |
| `schemas/platform/sync/*` | **Yes** — `nexus-platform` | CLI ↔ platform sync wire: bundle envelope (codegen canonical), pull request/response, conflict, delta, sync-command. `bundle-refinement.schema.json` is a validation-only refinement (codegen-skipped). |
| `schemas/domain/*` | **Yes** — `nexus-platform` (transitive via `$ref`) | Wire entities embedded in sync bundles & platform bodies — **not** the Rust `nexus-domain`/`nexus-cloud-domain` logic crates |
| `schemas/common/*` | **Yes** (when `$ref`'d by wire) | Shared identifiers, enums, value objects (`SourceAnchor`, `VersionRef`) |
| `schemas/local-api/compute/*` | **Yes** — external WASM modules + future WebApp | Compute module ABI envelopes (`ComputeInput`/`ComputeOutput`). V1.62 added the `local-api/` tree for cross-language Local API contracts. |
| `schemas/local-api/{works,kb,findings,schedule,workspace,creators}/*` | **Yes** — future WebApp/Web-UI | Core CRUD Local API request/response schemas for the daemon's `/v1/local/*` endpoints (V1.63 P1).
| *(removed from `schemas/`)* | **No** | `cli-sync/` (→ `cloud-sync/` → `platform/sync/`), `acp-runtime/`, `meta/`, `cloud-sync/`, `compute/` (entity-attributes/entity-state → `modules/<id>/manifest.json` in P1), `outbox_entry`, daemon/orchestration types → `src/local/` |

V1.20 removed **daemon local HTTP proxies** for `world/*` and `explore/*`; those operations use **platform HTTP** directly. The `schemas/platform/http-bff/world-*` and `.../explore-*` files remain **wire** for platform — they were never "daemon-only" contracts.

V1.62 reorganized `schemas/` along consumer-scope lines (compass v1.62 §1.3): the flat `platform/` split into `platform/{http-bff,sync}/`, sync payloads consolidated under `platform/sync/`, and compute envelopes moved to a new `local-api/compute/` tree. Per-module entity shape schemas (`compute/entity-attributes`, `compute/entity-state`) were **deleted** — per-module shapes now live in `modules/<id>/manifest.json` (V1.62 P1).

## Drift / housekeeping

- **README SSOT**: [schemas/README.md](../../schemas/README.md) + per-folder READMEs; layout rules in [specs/schemas-directory-layout.md](specs/schemas-directory-layout.md). Re-verify after moves.
- **Stale path risk**: do not reference `schemas/cli-sync/`, `schemas/meta/`, `schemas/acp-runtime/`, `schemas/cloud-sync/`, or `schemas/compute/` — removed or renamed (see layout spec §1 + §5 historical renames).
- **Codegen**: only files under `schemas/` generate TS in `@42ch/nexus-contracts`; platform upgrades follow npm semver + `schema_version`.
- **Full audit table**: [archived/knowledge/schemas-boundary.md](../archived/knowledge/schemas-boundary.md) §5.2 (53 wire / 10 local at audit time). Re-run audit before further moves; `rg <TypeName>` on `nexus-platform` before deleting generated TS.

## Related

- [local-cloud-crate-architecture.md](specs/local-cloud-crate-architecture.md) — local vs cloud product lines, crate graph, daemon API classes
- [archived/knowledge/daemon-api-workspace-write-architecture.md](../archived/knowledge/daemon-api-workspace-write-architecture.md) — **superseded** route table (use V1.20 compass + `daemon-runtime` / crate architecture SSOT)
- [v1.20-delivery-compass-v1.md](../iterations/v1.20-delivery-compass-v1.md) — shipped local API redesign
- [v1.21-local-platform-isolation-delivery-compass-v1.md](../iterations/v1.21-local-platform-isolation-delivery-compass-v1.md) — daemon sync route removal program
- [v1.62-compute-architecture-and-schemas-reorganization-delivery-compass-v1.md](../iterations/v1.62-compute-architecture-and-schemas-reorganization-delivery-compass-v1.md) — consumer-scope reorganization + `local-api/` tree

---

*Created: 2026-05-20 (as `schemas-wire-platform-sync-boundary.md`). Renamed + scope expanded 2026-06-23 (V1.62 P0). Pointer doc; do not duplicate the archived audit table here.*
