# Schemas — External-Consumer Boundary

**Status**: Active — current external daemon contracts use the Daemon API namespace; the bundled Web UI is an external API consumer
**Document class**: Companion
**Supersedes**: `schemas-wire-platform-sync-boundary.md` (same file, expanded scope).
**Aligned with**: `nexus` `schemas/AGENTS.md`, `crates/nexus-contracts/src/local/`
**Last reconciled**: 2026-09-04 — current schema tree, generated module names, and Daemon API namespace.

---

## Rule (authoritative)

A JSON Schema file belongs in `schemas/` **only if it is consumed by an external client** — either `nexus-platform` (wire) **OR** an external Daemon API client. The bundled Web UI (`apps/web`) counts as an external Daemon API consumer because it is TypeScript code consuming JSON over the daemon boundary, even though it is shipped inside the same OSS binary. Concretely, a schema belongs here when **at least one** of these holds:

- **Platform wire** — `nexus-platform` observes the type on a wire boundary:
  - Platform HTTP BFF bodies (`schemas/platform/http-bff/*`)
  - CLI ↔ platform sync payloads (`schemas/platform/sync/*`)
  - Any payload the OSS CLI/daemon sends to platform that platform must parse
- **Daemon API** — an **external** client (separate process / language boundary) consumes the type via `/v1/daemon/*`:
  - Compute module ABI envelopes (`schemas/daemon-api/compute/*`) — consumed by external WASM compute modules and generated clients.
  - Resource contracts under `schemas/daemon-api/<concern>/*` — consumed by the bundled Web UI, desktop shell, WASM modules, or generated clients.
  - Shared Daemon API error detail (`schemas/daemon-api/common/error-response.schema.json`) — consumed by the bundled Web UI and generated Daemon API clients; the runtime's outer failure envelope remains source-defined.
  - The complete current concern set is owned by [schemas-directory-layout.md](schemas-directory-layout.md); do not freeze a second directory list here.

Everything else is **local**: hand-written Rust under `crates/nexus-contracts/src/local/` — **no** `pnpm run codegen` entry in `@42ch/nexus-contracts` npm surface for those types.

**Corollary**: internal orchestration state, ACP registry manifests, worker IPC, SQLite row shapes, and daemon-only DTOs remain in Rust unless an external client must consume them cross-language. Externally consumed `/v1/daemon/*` contracts are promoted to `schemas/daemon-api/<concern>/`.

**DTO drift-closure criterion**: once a Daemon API shape is promoted under `schemas/daemon-api/`, the corresponding handler must emit `generated::daemon_api::*` shapes (or a structurally equivalent type covered by strict drift detection). `schema_drift_detection.rs` with `CheckMode::Strict` is the enforcement gate.

## Directory layout (normative)

Folder names, consumer-scope tree, and product-line mapping:
**[schemas-directory-layout.md](schemas-directory-layout.md)**. On-disk index:
[schemas/README.md](../../../../schemas/README.md).

## What still lives in `schemas/` today

| Tree | External consumer? | Notes |
| --- | --- | --- |
| `schemas/platform/http-bff/*` | **Yes** — `nexus-platform` | Platform HTTP request/response contracts |
| `schemas/platform/sync/*` | **Yes** — `nexus-platform` | CLI ↔ platform sync wire: bundle envelope (codegen canonical), pull request/response, conflict, delta, sync-command. `bundle-refinement.schema.json` is a validation-only refinement (codegen-skipped). |
| `schemas/domain/*` | **Yes** — `nexus-platform` (transitive via `$ref`) | Wire entities embedded in sync bundles & platform bodies — **not** the Rust `nexus-domain`/`nexus-cloud-domain` logic crates |
| `schemas/common/*` | **Yes** (when `$ref`'d by wire) | Shared identifiers, enums, value objects (`SourceAnchor`, `VersionRef`) |
| `schemas/daemon-api/compute/*` | **Yes** — external WASM modules + generated clients | Compute module ABI envelopes (`ComputeInput`/`ComputeOutput`). |
| `schemas/daemon-api/<concern>/*` | **Yes** — cross-language Daemon API consumers | Current concern inventory is normative in [schemas-directory-layout.md](schemas-directory-layout.md) §1; it includes agent-host, authoring/diagnostic, narrative-resource, runtime/tool, CRUD, orchestration, preset, compute, and common surfaces. |

There are no **daemon local HTTP proxies** for `world/*` and `explore/*`; those operations use **platform HTTP** directly. The `schemas/platform/http-bff/world-*` and `.../explore-*` files are **wire** for platform — never "daemon-only" contracts.

Per-module entity shape schemas do not exist; module-local shapes live in `modules/<id>/manifest.json`.

## Drift / housekeeping

- **README SSOT**: [schemas/README.md](../../../../schemas/README.md) + per-folder READMEs; layout rules in [schemas-directory-layout.md](schemas-directory-layout.md). Re-verify after moves.
- **Stale path risk**: do not reference `schemas/cli-sync/`, `schemas/meta/`, `schemas/acp-runtime/`, `schemas/cloud-sync/`, or `schemas/compute/` — removed or renamed (see layout spec §1).
- **Codegen**: only files under `schemas/` generate TS in `@42ch/nexus-contracts`; platform upgrades follow npm semver + `schema_version`.
- **Promoted Daemon API handlers**: handlers must return generated contract shapes for promoted schemas; strict drift detection is required before a schema-promoted route is considered consumer-safe.
- **Before further moves**: re-run an equivalent source scan; search `<TypeName>` in `nexus-platform` before deleting generated TypeScript.

## Related

- [local-cloud-crate-architecture.md](../../archived/architecture/2026-09-21-local-cloud-crate-architecture.md) — historical record: local vs cloud product lines, crate graph, daemon API classes

---

*Pointer doc; do not duplicate the archived audit table here.*
