# Nexus Architecture

High-level map of the **nexus** open-source monorepo: wire contracts, Rust workspace crates, entity-scope ownership, and how crates connect at build time. Normative scope and naming rules live in [`.agents/knowledge/specs/entity-scope-model.md`](../.agents/knowledge/specs/entity-scope-model.md); long-term local/cloud crate rules live in [`.agents/knowledge/specs/local-cloud-crate-architecture.md`](../.agents/knowledge/specs/local-cloud-crate-architecture.md). This document distinguishes **current Cargo wiring** from the **V1.23 target** so contributors do not mistake planned edges for existing runtime reachability.

## Monorepo layout

| Area | Path | Role |
| --- | --- | --- |
| Wire contracts (truth source) | `schemas/` | JSON Schema → codegen |
| Generated Rust types | `crates/nexus-contracts/` | Workspace-internal library |
| Generated TypeScript | `packages/nexus-contracts/` | npm `@42ch/nexus-contracts` for `nexus-platform` |
| CLI + libraries | `crates/*` | Rust workspace (see below) |
| Codegen / validation | `tooling/` | `pnpm run codegen`, schema checks |
| Normative OSS specs | `.agents/knowledge/specs/` | CLI, daemon, orchestration, sync contracts |
| End-user docs | `docs/` | Install, contributing, this file |

## Truth source: JSON Schema

All cross-repo wire shapes are defined under `schemas/`.

```text
schemas/*.json
    → tooling/codegen
        → crates/nexus-contracts/src/generated/   (Rust)
        → packages/nexus-contracts/               (TypeScript / npm)
```

Local-only types (daemon HTTP, schedules, orchestration IPC) are hand-written under `crates/nexus-contracts/src/local/` and are **not** generated from `schemas/`. See [`.agents/knowledge/schemas-wire-platform-sync-boundary.md`](../.agents/knowledge/schemas-wire-platform-sync-boundary.md) and [`.agents/knowledge/specs/schemas-directory-layout.md`](../.agents/knowledge/specs/schemas-directory-layout.md).

**Design principles**

- Single DTO source — no parallel handwritten wire structs in application crates
- `schema_version` locks cross-language contract evolution
- Platform consumes npm package; OSS consumes the Rust crate in-tree

## Product lines (local vs cloud)

| Line | Purpose | Primary crates | User entry |
| --- | --- | --- | --- |
| **Local** | Daemon supervisor, orchestration, agent-host, Creator workspace, Creator memory, World KB / narrative state, User knowledge, Moment context assembly | Current daemon-local core: `nexus-daemon-runtime`, `nexus-orchestration`, `nexus-agent-host`, `nexus-acp-host`, `nexus-creator`, `nexus-local-db`; current CLI also reaches `nexus-creator-memory` and `nexus-moment-context-assembly` for non-daemon flows. V1.23 target wires `nexus-creator-memory`, `nexus-narrative`, `nexus-kb`, `nexus-knowledge`, `nexus-moment-context-assembly` into the daemon/local product graph where product paths require them | `nexus42 daemon …` → `/v1/local/*` |
| **Cloud** | Platform HTTP, registration, bundle sync (CLI-only), optional context Stage-1 | `nexus-cloud-sync` (+ optional `legacy-sync` feature); V1.23 target routes User/Pairing invariants through `nexus-cloud-domain` | `nexus42 sync …`, `nexus42 platform …` |

**Hard isolation (enforced in `Cargo.toml`):** `nexus-daemon-runtime` does **not** depend on `nexus-cloud-sync` or `nexus-cloud-domain`. Platform sync and creator registration must not be exposed on the daemon Local API.

## Entity scope hierarchy

V1.23 uses the scope model in [`entity-scope-model.md`](../.agents/knowledge/specs/entity-scope-model.md) as the normative ownership map:

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
| `nexus-daemon-runtime` | Lifecycle, Local API, hosts orchestration + agent-host | `nexus-agent-host`, `nexus-creator`, `nexus-local-db`, `nexus-orchestration`, `nexus-contracts`, `nexus-home-layout` |
| `nexus-moment-context-assembly` | Moment / session-start context aggregation (current Stage-0 local; optional cloud Stage-1) | Current: `nexus-creator-memory`, `nexus-contracts`; optional `nexus-cloud-sync` via `cloud-stage` |

### Cloud line (CLI; not daemon)

| Crate | Responsibility | Direct deps (nexus crates) |
| --- | --- | --- |
| `nexus-cloud-sync` | Platform HTTP, delta/outbox (`legacy-sync` feature) | `nexus-contracts`, `nexus-home-layout`, `nexus-local-db` |
| `nexus-cloud-domain` | User / Pairing **domain logic** and invariants (no HTTP) | `nexus-contracts` |

### Domain libraries (current compile-only islands; V1.23 target product domains)

These crates were introduced in the V1.21 local/cloud split. They compile and have tests, but most are **not yet reachable from `nexus42` or `nexus-daemon-runtime` product paths** (verified via `cargo tree`). In the V1.23 target, they become the owning local product domains instead of remaining compile-only islands:

| Crate | Scope / intended role | Current wiring |
| --- | --- | --- |
| `nexus-kb` | **World KB** graph: KeyBlocks, SourceAnchors, graph insertion/query | Only used by `nexus-narrative`; not product-reachable from CLI/daemon |
| `nexus-narrative` | `World`, `Timeline`, `Event`: worlds, forks, timelines, story/manuscript projections | Depends on `nexus-kb`; not linked from CLI/daemon |
| `nexus-knowledge` | **User knowledge** / global knowledge index | Standalone; not linked from CLI/daemon |

CLI `nexus42 creator kb` today implements **CLI local work KB index** storage under the active Creator/workspace tree — it does **not** call `nexus-kb` or `nexus-knowledge` yet. Future `--scope world` behavior must route to `nexus-kb` + `nexus-narrative`; future User/global knowledge behavior must route to `nexus-knowledge`. See [`entity-scope-model.md` §5](../.agents/knowledge/specs/entity-scope-model.md#5-naming-clarifications) and [`local-cloud-crate-architecture.md` §3.5](../.agents/knowledge/specs/local-cloud-crate-architecture.md#35-nexus-kb-vs-nexus-knowledge).

### Executable surface

| Artifact | Crate | Notes |
| --- | --- | --- |
| **`nexus42`** | `crates/nexus42` | Sole user-facing binary; ACP **client** only |
| Daemon runtime | (library) `nexus-daemon-runtime` | Started via `nexus42 daemon start` / hidden `daemon-run` — not a separate product binary |
| ACP worker | `nexus42 acp-worker` (hidden) | Subprocess; uses `nexus-acp-host` |

`nexus42` enables `nexus-moment-context-assembly/cloud-stage` and `nexus-cloud-sync/legacy-sync` for CLI cloud workflows. Daemon builds use `nexus-daemon-runtime` without those cloud features on the runtime crate itself.

## Dependency graph — current Cargo wiring (verified 2026-05-21)

This graph describes current `Cargo.toml` / `cargo tree` reality. It is intentionally separate from the V1.23 target graph below.

```text
                         schemas/
                            │
                            ▼
                   nexus-contracts ◄────────────────────────────┐
                            ▲                                    │
         ┌──────────────────┼──────────────────┐                 │
         │                  │                  │                 │
  nexus-home-layout    nexus-local-db     nexus-cloud-domain     │
         │                  │              (unwired)             │
         │                  │                  │                 │
         ├──────── nexus-creator ──────────────┤                 │
         │           │                         │                 │
         │    nexus-creator-memory             │                 │
         │           │                         │                 │
         │    nexus-moment-context-assembly ───┼──[cloud-stage]──┤
         │           │                         │       │         │
         │           │                         │       ▼         │
         │           │                    nexus-cloud-sync       │
         │           │                         │                 │
         ├──── nexus-orchestration ◄───────────┘                 │
         │           ▲                                           │
         │           │                                           │
         ├──── nexus-agent-host ──► nexus-acp-host               │
         │           ▲                                           │
         │           │                                           │
         └──── nexus-daemon-runtime ─────────────────────────────┘
                         ▲
                         │
                      nexus42  ─── (also: cloud-sync, moment-assembly w/ cloud-stage)

  Unwired cluster (compile-only today):
      nexus-kb ◄── nexus-narrative
      nexus-knowledge (standalone)
```

**Forbidden edge (normative):** `nexus-daemon-runtime` → `nexus-cloud-sync` | `nexus-cloud-domain` — satisfied.

**Current V1.23 alignment gaps:** `nexus-cloud-sync` does not yet depend on `nexus-cloud-domain`; `nexus-moment-context-assembly` Stage-0 does not yet depend on `nexus-narrative`, `nexus-kb`, or `nexus-knowledge`; `nexus-daemon-runtime` does not yet depend on `nexus-creator-memory`, `nexus-moment-context-assembly`, `nexus-narrative`, `nexus-kb`, or `nexus-knowledge`. These are target gaps, not contradictions in the current graph. See [`local-cloud-crate-architecture.md` §4–§5](../.agents/knowledge/specs/local-cloud-crate-architecture.md#4-currently-wired-cargo-graph-verified-2026-05-21).

## Dependency graph — V1.23 target wiring

The following graph is the V1.23 target from the locked plan and [`local-cloud-crate-architecture.md` §5](../.agents/knowledge/specs/local-cloud-crate-architecture.md#5-v123-target-wiring). It does **not** claim these edges exist today unless they also appear in the current graph above.

```text
nexus42
  ├── nexus-daemon-runtime
  │   ├── nexus-orchestration
  │   ├── nexus-agent-host
  │   ├── nexus-creator
  │   ├── nexus-creator-memory
  │   ├── nexus-narrative
  │   ├── nexus-kb
  │   ├── nexus-knowledge
  │   ├── nexus-moment-context-assembly (default features only)
  │   └── nexus-local-db
  ├── nexus-cloud-sync
  │   └── nexus-cloud-domain
  └── nexus-moment-context-assembly (cloud-stage only for CLI/platform flows)

nexus-moment-context-assembly (default Stage-0 target)
  ├── nexus-creator-memory
  ├── nexus-narrative
  ├── nexus-kb
  ├── nexus-knowledge
  └── nexus-contracts
```

Target constraints:

- Daemon wiring must use `nexus-moment-context-assembly` default features only.
- Daemon wiring must still have no `nexus-cloud-sync`, no `nexus-cloud-domain`, and no platform HTTP path.
- Cloud transport must use `nexus-cloud-domain` for User/Pairing invariants instead of reimplementing them in `nexus-cloud-sync`.
- Moment assembly is a read-only pre-session aggregation point; it can include Creator memory, narrative state, World KB assets, and User knowledge without moving ownership between scopes.

## CLI command groups (frozen surface)

Six top-level groups ([`cli-spec.md`](../.agents/knowledge/specs/cli-spec.md) §6):

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

Route tables change per release. Authoritative lists:

- `crates/nexus-daemon-runtime/src/api/mod.rs`
- [`.agents/knowledge/specs/local-runtime-boundary.md`](../.agents/knowledge/specs/local-runtime-boundary.md)
- Active iteration compass under `.agents/iterations/`

**Classes allowed on daemon:** runtime health, workspace, local Creator listing, orchestration, agent-host, preset management, CLI local work KB index endpoints as implemented today; V1.23 target local endpoints may also reach User knowledge, World KB / narrative state, and default-feature Moment context assembly through their owning crates.

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
| Entity scopes, crate ownership, KB / knowledge naming | [entity-scope-model.md](../.agents/knowledge/specs/entity-scope-model.md) |
| Crate responsibilities & forbidden deps | [local-cloud-crate-architecture.md](../.agents/knowledge/specs/local-cloud-crate-architecture.md) |
| Daemon layering | [daemon-runtime.md](../.agents/knowledge/specs/daemon-runtime.md) |
| Orchestration | [orchestration-engine.md](../.agents/knowledge/specs/orchestration-engine.md) |
| CLI behavior | [cli-spec.md](../.agents/knowledge/specs/cli-spec.md) |
| Spec index | [specs/README.md](../.agents/knowledge/specs/README.md) |
| Per-crate rules | `crates/*/AGENTS.md` |
| Deferred wiring / stubs | [deferred-features-cross-version-tracker.md](../.agents/knowledge/deferred-features-cross-version-tracker.md) |
