# Nexus Architecture

High-level map of the **nexus** open-source monorepo: wire contracts, Rust workspace crates, entity-scope ownership, and how crates connect at build time. Normative scope and naming rules live in [`.mstar/knowledge/specs/entity-scope-model.md`](../.mstar/knowledge/specs/entity-scope-model.md); long-term local/cloud crate rules live in [`.mstar/knowledge/specs/local-cloud-crate-architecture.md`](../.mstar/knowledge/specs/local-cloud-crate-architecture.md).

This document separates **Cargo dependency wiring** (what compiles and links) from **product integration** (what CLI commands and daemon HTTP handlers actually call). A full knowledge↔crates drift audit lives in [`.mstar/iterations/v1.24-knowledge-crates-alignment-audit-compass-v1.md`](../.mstar/iterations/v1.24-knowledge-crates-alignment-audit-compass-v1.md) (evidence date **2026-05-25**).

## Monorepo layout

| Area | Path | Role |
| --- | --- | --- |
| Wire contracts (truth source) | `schemas/` | JSON Schema → codegen |
| Generated Rust types | `crates/nexus-contracts/` | Workspace-internal library |
| Generated TypeScript | `packages/nexus-contracts/` | npm `@42ch/nexus-contracts` for `nexus-platform` |
| CLI + libraries | `crates/*` | Rust workspace (see below) |
| Codegen / validation | `tooling/` | `pnpm run codegen`, schema checks |
| Normative OSS specs | `.mstar/knowledge/specs/` | CLI, daemon, orchestration, sync contracts |
| End-user docs | `docs/` | Install, contributing, this file |

## Truth source: JSON Schema

All cross-repo wire shapes are defined under `schemas/`.

```text
schemas/*.json
    → tooling/codegen
        → crates/nexus-contracts/src/generated/   (Rust)
        → packages/nexus-contracts/               (TypeScript / npm)
```

Local-only types (daemon HTTP, schedules, orchestration IPC) are hand-written under `crates/nexus-contracts/src/local/` and are **not** generated from `schemas/`. See [`.mstar/knowledge/schemas-wire-platform-sync-boundary.md`](../.mstar/knowledge/schemas-wire-platform-sync-boundary.md) and [`.mstar/knowledge/specs/schemas-directory-layout.md`](../.mstar/knowledge/specs/schemas-directory-layout.md).

**Design principles**

- Single DTO source — no parallel handwritten wire structs in application crates
- `schema_version` locks cross-language contract evolution
- Platform consumes npm package; OSS consumes the Rust crate in-tree

## Product lines (local vs cloud)

| Line | Purpose | Primary crates | User entry |
| --- | --- | --- | --- |
| **Local** | Daemon supervisor, orchestration, agent-host, Creator workspace, Creator memory, World KB / narrative state, User knowledge, Moment context assembly | **Cargo:** `nexus-daemon-runtime` links orchestration, agent-host, creator, creator-memory, narrative, kb, knowledge, moment-context-assembly (default features), local-db. **Product:** memory + orchestration + agent-host + **work-scope KB file index** + **narrative read-only** (`GET /v1/local/narrative/*`) are wired on daemon HTTP; `assemble-moment` is the **single assembly SSOT** (no `assemble-local`); SQLite four-domain context assembly via `nexus-moment-context-assembly`. See § Product integration gaps. | `nexus42 daemon …` → `/v1/local/*` |
| **Cloud** | Platform HTTP, registration, bundle sync (CLI-only), optional context Stage-1 | `nexus-cloud-sync` (`legacy-sync` on CLI) → `nexus-cloud-domain` for User/Pairing invariants | `nexus42 sync …`, `nexus42 platform …` |

**Hard isolation (enforced in `Cargo.toml`):** `nexus-daemon-runtime` does **not** depend on `nexus-cloud-sync` or `nexus-cloud-domain`. Platform sync and creator registration must not be exposed on the daemon Local API.

## Entity scope hierarchy

V1.23 uses the scope model in [`entity-scope-model.md`](../.mstar/knowledge/specs/entity-scope-model.md) as the normative ownership map:

```text
Global
└── User
    ├── Creator
    │   └── World
    │       ├── Timeline
    │       │   └── Event
    │       │       └── Moment
    │       └── KB graph / narrative knowledge assets
    └── User knowledge index
```

Key contributor rules:

- Every scoped entity has exactly one canonical owning scope.
- `User` / `Pairing` invariants belong to `nexus-cloud-domain`; cloud transport belongs to `nexus-cloud-sync`.
- `Creator` aggregate and local operational identity belong to `nexus-creator`; Creator memory belongs to `nexus-creator-memory`.
- **World KB** / **narrative KB** assets belong under `World` and are owned by `nexus-kb` with `nexus-narrative` coordinating narrative context.
- **User knowledge** / **global knowledge index** belongs to `nexus-knowledge`; it is not Creator-scoped and does not own World KeyBlocks.
- **CLI local work KB index** means today’s `nexus42 creator kb --scope work` file/index workflow under the active `creator_id` + `workspace_slug`; it is not equivalent to `nexus-kb` or `nexus-knowledge`.

## Rust workspace members (16)

### Foundation (types & paths)

| Crate | Responsibility |
| --- | --- |
| `nexus-contracts` | Generated wire types + `src/local/` + enum conversions; no I/O |
| `nexus-home-layout` | Frozen `~/.nexus42/` path helpers; no entity invariants |

### Local runtime stack (wired into `nexus42`)

| Crate | Responsibility | Direct deps (nexus crates) |
| --- | --- | --- |
| `nexus-acp-host` | ACP client SDK adapter | `nexus-contracts` |
| `nexus-agent-host` | Managed agent sessions (ACP client) | `nexus-acp-host`, `nexus-contracts`, `nexus-home-layout` |
| `nexus-creator` | Creator aggregate logic, credential/cache hooks, active Creator local state | `nexus-contracts`, `nexus-home-layout` |
| `nexus-creator-memory` | Creator-scoped SOUL, long-term memory, review, personality / experience I/O | `nexus-creator`, `nexus-contracts`, `nexus-home-layout` |
| `nexus-local-db` | Shared SQLite mechanics for Creator/workspace working copies; does not own narrative or User semantics | `nexus-contracts` |
| `nexus-orchestration` | Presets, graph-flow engine, schedules, capability registry; carries scope IDs as execution context | `nexus-contracts`, `nexus-home-layout`, `nexus-local-db` |
| `nexus-daemon-runtime` | Lifecycle, Local API, hosts orchestration + agent-host | `nexus-agent-host`, `nexus-creator`, `nexus-creator-memory`, `nexus-local-db`, `nexus-orchestration`, `nexus-narrative`, `nexus-kb`, `nexus-knowledge`, `nexus-moment-context-assembly` (default features), `nexus-contracts`, `nexus-home-layout` |
| `nexus-moment-context-assembly` | Moment / session-start context aggregation (`assemble_moment`; Stage-0 + optional cloud Stage-1) | `nexus-creator-memory`, `nexus-narrative`, `nexus-kb`, `nexus-knowledge`, `nexus-contracts`; optional `nexus-cloud-sync` via `cloud-stage` |

### Cloud line (CLI; not daemon)

| Crate | Responsibility | Direct deps (nexus crates) |
| --- | --- | --- |
| `nexus-cloud-sync` | Platform HTTP, delta/outbox (`legacy-sync` feature) | `nexus-cloud-domain`, `nexus-contracts`, `nexus-home-layout`, `nexus-local-db` |
| `nexus-cloud-domain` | User / Pairing **domain logic** and invariants (no HTTP) | `nexus-contracts` |

### Domain libraries (Cargo-linked; product integration partial)

Introduced in the V1.21 split; **linked in `Cargo.toml` since V1.23 alignment** (verified 2026-05-25). Domain logic and tests live in-crate; most **daemon HTTP / CLI commands** still use legacy file/SQLite paths instead of these APIs.

| Crate | Scope / role | Cargo reachability | Product integration (2026-05-25) |
| --- | --- | --- | --- |
| `nexus-kb` | **World KB** graph: KeyBlocks, SourceAnchors, `KbStore` | `nexus-narrative`, `nexus-moment-context-assembly`, `nexus-daemon-runtime` | Used by moment-assembly and narrative read-only routes; `GET /v1/local/narrative/*` exposes World KB reads via `nexus-narrative` gateway. `/v1/local/kb/*` remains **work file index** (not `nexus-kb`) |
| `nexus-narrative` | `World`, `Timeline`, `Event`: worlds, forks, timelines, manuscripts | `nexus-moment-context-assembly`, `nexus-daemon-runtime` | `NarrativeGateway` powers `GET /v1/local/narrative/*` (read-only). **World fork is platform-only** (PD-01; see [`entity-scope-model.md`](../.mstar/knowledge/specs/entity-scope-model.md)); no local fork CLI. |
| `nexus-knowledge` | **User knowledge** entries + reference-source types | `nexus-moment-context-assembly`, `nexus-daemon-runtime` | SQLite persistence shipped V1.27; `GET /v1/local/references` lists via **`nexus-local-db`** |

CLI `nexus42 creator kb` and daemon `/v1/local/kb/entries` implement the **CLI local work KB index** (files under `~/.nexus42/.../kb/`) — **not** `nexus-kb`. World KB scope (`--scope world`) routes to `nexus-narrative` + `nexus-kb` through the narrative read-only API. **World fork is platform-only** (PD-01; no local fork CLI — see [`entity-scope-model.md`](../.mstar/knowledge/specs/entity-scope-model.md)). See [`entity-scope-model.md` §5](../.mstar/knowledge/specs/entity-scope-model.md#5-naming-clarifications) and audit compass **KCA-003**.

### Executable surface

| Artifact | Crate | Notes |
| --- | --- | --- |
| **`nexus42`** | `crates/nexus42` | Sole user-facing binary; ACP **client** only |
| Daemon runtime | (library) `nexus-daemon-runtime` | Started via `nexus42 daemon start` / hidden `daemon-run` — not a separate product binary |
| ACP worker | `nexus42 acp-worker` (hidden) | Subprocess; uses `nexus-acp-host` |

`nexus42` enables `nexus-moment-context-assembly/cloud-stage` and `nexus-cloud-sync/legacy-sync` for CLI cloud workflows. Daemon builds use `nexus-daemon-runtime` without those cloud features on the runtime crate itself.

## Dependency graph — Cargo wiring (verified 2026-05-25)

Direct workspace dependencies from `Cargo.toml` / `cargo tree`. This is the **build-time** graph.

```text
                         schemas/
                            │
                            ▼
                   nexus-contracts ◄──────────────────────────────────────┐
                            ▲                                               │
    nexus-home-layout ──────┼───────────────────────────────────────────────┤
    nexus-local-db ─────────┤                                               │
    nexus-cloud-domain ─────┤                                               │
         │                  │                                               │
    nexus-creator ──────────┤                                               │
         │                  │                                               │
    nexus-creator-memory ───┤                                               │
         │                  │                                               │
    nexus-kb ◄── nexus-narrative ──┐                                        │
         ▲              ▲          │                                        │
         │              │          │                                        │
    nexus-knowledge ─────┼──────────┼── nexus-moment-context-assembly       │
                         │          │         │ [cloud-stage] ──► cloud-sync  │
    nexus-orchestration ◄┘          │         │ (CLI enables cloud-stage)     │
    nexus-agent-host ──► nexus-acp-host       │                             │
         ▲                                    │                             │
         │                                    │                             │
    nexus-daemon-runtime ◄────────────────────┘                             │
         ▲                                                                  │
         │                                                                  │
      nexus42 ──────────────► cloud-sync (legacy-sync)                      │
              └────────────► moment-context-assembly (cloud-stage) ───────┘

nexus-cloud-sync ──► nexus-cloud-domain
```

**Forbidden edges (normative, satisfied):** `nexus-daemon-runtime` must not depend on `nexus-cloud-sync` or `nexus-cloud-domain`. Enforced in `Cargo.toml` and `architecture_assertions` tests.

**Daemon moment assembly:** `nexus-daemon-runtime` depends on `nexus-moment-context-assembly` with **default features** (no `cloud-stage`). CLI depends on the same crate with **`cloud-stage`** for platform-enhanced assembly.

**Narrative ↔ KB:** `nexus-narrative` → `nexus-kb` only (narrative coordinates World-scoped KB graph).

Normative spec §4 in [`local-cloud-crate-architecture.md`](../.mstar/knowledge/specs/local-cloud-crate-architecture.md) is refreshed to match this graph; remaining discrepancies are tracked as product-integration gaps in [v1.24 audit compass](../.mstar/iterations/v1.24-knowledge-crates-alignment-audit-compass-v1.md).

## Product integration gaps (runtime behavior, 2026-05-25)

Cargo edges alone do not mean daemon HTTP or CLI commands call a crate. Known gaps:

| Area | Wired in Cargo? | Product path today | Gap ID |
| --- | --- | --- | --- |
| Moment `assemble_moment` | Yes (daemon + CLI libs) | **Shipped:** `nexus42 platform context assemble-moment` calls `assemble_moment` in-process (four-domain SQLite assembly). `assemble-local` is **removed** in pre-release. Daemon has no `POST /v1/local/context/assemble` (KCA-002 B2 retired). | KCA-002 |
| World KB (`nexus-kb`) | Yes | `/v1/local/kb/*` = **work file index**, not `KbStore`. World KB reads exposed via `GET /v1/local/narrative/*` | KCA-003 |
| Narrative gateway | Yes | `GET /v1/local/narrative/*` (read-only) — shipped V1.27. No write/mutation routes (fork is platform-only per PD-01). | — |
| User knowledge store | Yes | SQLite persistence shipped V1.27; `GET /v1/local/references` still lists via **local-db** | KCA-004 |
| Orchestration engine in daemon lifecycle | Yes (orchestration crate) | Engine/worker stubs (DF-38–DF-40) | tracker |
| Author Intelligence loop (V1.29) | Yes (`nexus-creator-memory`, `nexus-orchestration`) | CLI `creator memory pending-*` / `creator soul refresh-experience` shipped. Orchestration `kb.extract_work` / `soul.experience.aggregate` registered; `acp_prompt` partially de-stubbed for preset paths. Full de-stub deferred (FL-D). | tracker |
| KB extract queue (V1.29) | Yes (`nexus-local-db`, `nexus-orchestration`) | CLI `creator kb queue-extract` / `extract-status` shipped. Extraction runs via preset + `acp_prompt` IPC. | tracker |

See [v1.24-knowledge-crates-alignment-audit-compass-v1.md](../.mstar/iterations/v1.24-knowledge-crates-alignment-audit-compass-v1.md) for remediation themes.

## CLI command groups (frozen surface)

Six top-level groups ([`cli-spec.md`](../.mstar/knowledge/specs/cli-spec.md) §6):

| Group | Role |
| --- | --- |
| `daemon` | Runtime lifecycle, schedules, orchestration control |
| `acp` | Registry, agents, skills, probe/doctor |
| `creator` | Identity, workspace, soul, memory, **CLI local work KB index** (`creator kb --scope work`) |
| `sync` | Structured platform sync (`nexus-cloud-sync`) |
| `platform` | Auth, explore, context assemble, publish; future User knowledge entry points route to `nexus-knowledge` |
| `system` | version, doctor, config, completion, debug |

Hidden entries: `acp-worker`, `daemon-run` (internal process modes).

## TypeScript workspace

| Package | Role |
| --- | --- |
| `@42ch/nexus-contracts` | Generated wire types; consumed by private `nexus-platform` via npm semver |

No second handwritten DTO set in platform — types must come from this repo’s schemas.

## Local API authority

**Authoritative route list:** `crates/nexus-daemon-runtime/src/api/mod.rs` (registered routes). [`.mstar/knowledge/specs/local-runtime-boundary.md`](../.mstar/knowledge/specs/local-runtime-boundary.md) §3.2.1 is refreshed to mark unregistered context/research/agent-session rows as retired or not implemented (audit **KCA-002**, **KCA-006**).

### Registered route families (2026-05-25)

| Family | Prefix / examples |
| --- | --- |
| Runtime | `GET /v1/local/runtime/health`, `…/status`, `GET /v1/local/daemon/status` |
| Workspace | `GET /v1/local/workspace`, `POST …/workspace/init`, `GET|POST /v1/local/workspaces`, `…/workspaces/active` |
| Creator | `GET /v1/local/creators`, `GET|PUT …/creators/active`, `GET …/creators/{id}`, `POST …:logout` |
| References | `GET /v1/local/references` (SQLite via local-db) |
| Work KB index | `GET|POST /v1/local/kb/entries`, `GET|DELETE …/kb/entries/{id}` (**file index**, not `nexus-kb`) |
| Narrative (read-only) | `GET /v1/local/narrative/*` — World KB reads via `NarrativeGateway` |
| Memory review | `GET|POST|DELETE /v1/local/memory/pending-review…` |
| Presets | `GET|POST /v1/local/presets`, `POST …/presets:validate`, `POST …/presets/{id}:reload` |
| Orchestration | `GET|POST /v1/local/orchestration/sessions`, schedules, capabilities, presets |
| Agent host | `/v1/local/agent-host/*` (sessions, operations, events SSE) |
| Monitoring | `GET /v1/local/monitoring/pool` |

**Not registered today (but referenced elsewhere):** `POST /v1/local/context/assemble` (retired; `assemble-moment` replaces it), `GET /v1/local/research/sources`, `POST /v1/local/research/scan`, `POST /v1/local/agent-sessions/restart`, legacy `/v1/local/sync/*` (correctly removed per V1.21).

**Classes allowed on daemon:** runtime health, workspace, local Creator listing, orchestration, agent-host, preset management, work-scope KB file index, memory pending-review, references list.

**Classes forbidden on daemon:** `/sync/*`, platform registration proxies, public `/acp/*` as a server (ACP stays client/worker path).

## Versioning

- Wire: `schema_version` in schemas and bundle envelopes
- CLI / crate: SemVer per crate; breaking wire → coordinated bump of schemas, Rust crate, and npm major
- Pre-release (&lt;1.0): breaking CLI, paths, and on-disk layout allowed without migration

## Pre-release local storage and breaking changes

Nexus is pre-release (version &lt; 1.0). Per [`AGENTS.md`](../AGENTS.md), breaking changes are expected and allowed — API shapes, CLI flags, on-disk paths, config file layout, and behavior may change without a deprecation period. **Local persistence may be wiped rather than migrated.**

### What is stored locally

| Path | Content | Owner crate |
| --- | --- | --- |
| `~/.nexus42/config.toml` | CLI preferences (active creator, workspace slug, URLs, runtime mode) | `nexus42` (CLI surface) |
| `~/.nexus42/agents.toml` | Multi-agent strategy/role overrides | `nexus42` (CLI surface) |
| `~/.nexus42/auth.json` | Platform JWT tokens, creator auth state, API keys | `nexus42` (CLI surface; planned daemon migration) |
| `~/.nexus42/creator-identities.json` | Creator display name/handle cache | `nexus42` (CLI surface; planned SQLite migration) |
| `~/.nexus42/state.db` | Global identity database | `nexus-local-db` via `nexus42` |
| `~/.nexus42/creators/<cid>/workspaces/<slug>/state.db` | Per-creator/workspace working copy (creators, reference_sources, outbox, etc.) | `nexus-local-db` |
| `~/.nexus42/creators/<cid>/workspaces/<slug>/kb/index.json` + `entries/*.md` | CLI local work KB index (distinct from World KB and User knowledge) | `nexus42` (CLI surface; future routing to `nexus-kb` / `nexus-knowledge`) |
| `<workspace_root>/.nexus42/workspace.json` | Workspace display config | `nexus42` (CLI surface) |

### What users should expect before 1.0

- **On-disk paths may change** between versions. After an upgrade, existing `~/.nexus42/` data may not be readable by the new version.
- **No automatic migration.** If a breaking schema or layout change occurs, the recommended action is to delete `~/.nexus42/` (or the affected workspace's data) and re-initialize.
- **CLI config may reset.** `config.toml` fields may be added, removed, or renamed. If the file cannot be parsed, it is backed up and defaults are used.
- **Auth tokens may be invalidated.** `auth.json` tokens have expiry times and may not survive across CLI version changes if the platform token format changes.
- **KB work index may be reset.** The CLI local work KB index (`creator kb --scope work`) uses a simple JSON + Markdown file layout. This is a temporary implementation — future versions will route to the proper domain crates (`nexus-kb`, `nexus-knowledge`), and the existing file-based index may not be migrated.
- **SQLite schemas may change.** `state.db` uses a versioned migration system (`db_schema_version`), but pre-release migrations may be destructive (drop-and-recreate rather than in-place alter).

### Storage ownership summary

The CLI crate `nexus42` is a **command/router layer**. It does not own domain storage semantics:

- All SQLite mechanics are delegated to `nexus-local-db`.
- Creator memory operations are delegated to `nexus-creator-memory`.
- Cloud sync operations are delegated to `nexus-cloud-sync`.
- Path layout is delegated to `nexus-home-layout`.
- File-based caches in `nexus42` (auth, identities, KB work index) are pre-release conveniences acknowledged for future migration to proper domain crates or SQLite.

## Architecture constraints

- Daemon runtime is a **local supervisor**, not an ACP Agent/Server
- Default sync is **structured deltas/bundles**, not full manuscript upload
- **World history is immutable** — changes go through Fork, not in-place mutation
- Wire types in code must match `schemas/` (run `pnpm run codegen` after schema edits)

## Further reading

| Topic | Document |
| --- | --- |
| Entity scopes, crate ownership, KB / knowledge naming | [entity-scope-model.md](../.mstar/knowledge/specs/entity-scope-model.md) |
| Crate responsibilities & forbidden deps | [local-cloud-crate-architecture.md](../.mstar/knowledge/specs/local-cloud-crate-architecture.md) |
| Daemon layering | [daemon-runtime.md](../.mstar/knowledge/specs/daemon-runtime.md) |
| Orchestration | [orchestration-engine.md](../.mstar/knowledge/specs/orchestration-engine.md) |
| CLI behavior | [cli-spec.md](../.mstar/knowledge/specs/cli-spec.md) |
| Spec index | [specs/README.md](../.mstar/knowledge/specs/README.md) |
| Per-crate rules | `crates/*/AGENTS.md` |
| Knowledge↔crates audit (V1.24) | [v1.24-knowledge-crates-alignment-audit-compass-v1.md](../.mstar/iterations/v1.24-knowledge-crates-alignment-audit-compass-v1.md) |
| V1.23 wiring reference | [v1.23-architecture-crate-wiring-reference-compass-v1.md](../.mstar/iterations/v1.23-architecture-crate-wiring-reference-compass-v1.md) |
| Deferred wiring / stubs | [deferred-features-cross-version-tracker.md](../.mstar/knowledge/deferred-features-cross-version-tracker.md) |
