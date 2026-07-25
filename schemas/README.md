# `schemas/` — External-Consumer JSON Schema Tree

**Authority:** [`.mstar/specs/schemas-directory-layout.md`](../.mstar/specs/schemas-directory-layout.md) (folder layout) + [`.mstar/knowledge/schemas-external-consumer-boundary.md`](../.mstar/knowledge/schemas-external-consumer-boundary.md) (external-consumer vs local-only rule).

**Local-only types** live in `crates/nexus-contracts/src/local/` — not under `schemas/`.

## Layout

| Directory | Files (approx.) | Purpose |
| --- | --- | --- |
| [common/](common/) | 3 | Shared IDs, enums, `SourceAnchor`, `VersionRef` |
| [domain/](domain/) | 9 | Wire entities (Creator, World, TimelineEvent, …) |
| [platform/http-bff/](platform/http-bff/) | 34 | Platform HTTP request/response bodies |
| [platform/sync/](platform/sync/) | 7 | CLI ↔ platform sync protocol (bundle, delta, pull, conflict) |
| [daemon-api/common/](daemon-api/common/) | 1 | Shared Daemon API envelopes — `ErrorResponse` (F-E1, V1.64) |
| [daemon-api/compute/](daemon-api/compute/) | 2 | WASM compute ABI envelopes (ComputeInput / ComputeOutput). V1.61 origin, V1.62 moved. |
| [daemon-api/works/](daemon-api/works/) | 10 | Works CRUD request/response schemas (V1.63) |
| [daemon-api/kb/](daemon-api/kb/) | 8 | Work-scope KB entry CRUD schemas + shared `PaginationInfo` (V1.63) |
| [daemon-api/findings/](daemon-api/findings/) | 6 | Quality findings CRUD schemas + cursor list response (V1.63; list-response F-P2 V1.64) |
| [daemon-api/schedule/](daemon-api/schedule/) | 14 | Schedule + core-context CRUD schemas (V1.63) |
| [daemon-api/workspace/](daemon-api/workspace/) | 8 | Workspace management CRUD schemas (V1.63) |
| [daemon-api/creators/](daemon-api/creators/) | 8 | Creator management CRUD schemas (V1.63) |
| [daemon-api/orchestration/sessions/](daemon-api/orchestration/sessions/) | 4 | Orchestration engine session READ schemas (V1.63 P3) |
| [daemon-api/orchestration/capabilities/](daemon-api/orchestration/capabilities/) | 2 | Capability registry READ schemas (V1.63 P3) |
| [daemon-api/preset-management/](daemon-api/preset-management/) | 7 | Preset management full-surface schemas (V1.63 P3) |

**Removed paths (do not recreate):**

- `schemas/acp-runtime/` — → `crates/nexus-contracts/src/local/acp_runtime/`
- `schemas/meta/` — meta-schema → `crates/nexus-contracts/src/local/meta.rs`
- `schemas/cli-sync/` — renamed **`cloud-sync/`** (2026-05-20); `cloud-sync/` folded into **`platform/sync/`** (2026-06-23, V1.62 P0)
- `schemas/cloud-sync/` — → **`platform/sync/`** (2026-06-23, V1.62 P0)
- `schemas/compute/` — compute envelopes → **`daemon-api/compute/`**; entity-attributes/entity-state **deleted** (per-module shapes → `modules/<id>/manifest.json`) (2026-06-23, V1.62 P0)

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
| **Cloud** (CLI `sync` / `platform`, `nexus-cloud-sync`) | **Yes** — `platform/{http-bff,sync}/`, `domain/`, `common/` |
| **Daemon API** (external WASM modules; future WebApp/Web-UI) | **Yes** — `daemon-api/compute/` (V1.62), `daemon-api/common/` (F-E1 V1.64), `daemon-api/{works,kb,findings,schedule,workspace,creators}/` (V1.63 P1), `daemon-api/{orchestration,preset-management}/` (V1.63 P3) |
| **Local** (daemon `/v1/daemon/*`, orchestration, ACP) | **No** — `nexus-contracts/src/local/` |

See [local-cloud-crate-architecture.md](../.mstar/specs/local-cloud-crate-architecture.md).
