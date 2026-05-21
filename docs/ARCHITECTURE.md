# Nexus Architecture

High-level map of the **nexus** open-source monorepo: wire contracts, Rust workspace crates, and how they connect at build time. Normative rules and API class boundaries live in [`.agents/knowledge/specs/local-cloud-crate-architecture.md`](../.agents/knowledge/specs/local-cloud-crate-architecture.md); this document tracks **what is linked today** (`cargo tree`).

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
| **Local** | Daemon supervisor, orchestration, agent-host, creator workspace, schedules | `nexus-daemon-runtime`, `nexus-orchestration`, `nexus-agent-host`, `nexus-acp-host`, `nexus-creator`, `nexus-creator-memory`, `nexus-local-db` | `nexus42 daemon …` → `/v1/local/*` |
| **Cloud** | Platform HTTP, registration, bundle sync (CLI-only) | `nexus-cloud-sync` (+ optional `legacy-sync` feature) | `nexus42 sync …`, `nexus42 platform …` |

**Hard isolation (enforced in `Cargo.toml`):** `nexus-daemon-runtime` does **not** depend on `nexus-cloud-sync` or `nexus-cloud-domain`. Platform sync and creator registration must not be exposed on the daemon Local API.

## Rust workspace members (16)

### Foundation (types & paths)

| Crate | Responsibility |
| --- | --- |
| `nexus-contracts` | Generated wire types + `src/local/` + enum conversions; no I/O |
| `nexus-home-layout` | Frozen `~/.nexus42/` path helpers |

### Local runtime stack (wired into `nexus42`)

| Crate | Responsibility | Direct deps (nexus crates) |
| --- | --- | --- |
| `nexus-acp-host` | ACP client SDK adapter | `nexus-contracts` |
| `nexus-agent-host` | Managed agent sessions (ACP client) | `nexus-acp-host`, `nexus-contracts`, `nexus-home-layout` |
| `nexus-creator` | Creator aggregate logic (types from contracts) | `nexus-contracts`, `nexus-home-layout` |
| `nexus-creator-memory` | SOUL, LTM, review, personality I/O | `nexus-creator`, `nexus-contracts`, `nexus-home-layout` |
| `nexus-local-db` | Shared SQLite access for CLI + daemon | `nexus-contracts` |
| `nexus-orchestration` | Presets, graph-flow engine, schedules, capability registry | `nexus-contracts`, `nexus-home-layout`, `nexus-local-db` |
| `nexus-daemon-runtime` | Lifecycle, Local API, hosts orchestration + agent-host | `nexus-agent-host`, `nexus-creator`, `nexus-local-db`, `nexus-orchestration`, `nexus-contracts`, `nexus-home-layout` |
| `nexus-moment-context-assembly` | Per-moment context (Stage-0 local; optional cloud Stage-1) | `nexus-creator-memory`, `nexus-contracts`; optional `nexus-cloud-sync` via `cloud-stage` |

### Cloud line (CLI; not daemon)

| Crate | Responsibility | Direct deps (nexus crates) |
| --- | --- | --- |
| `nexus-cloud-sync` | Platform HTTP, delta/outbox (`legacy-sync` feature) | `nexus-contracts`, `nexus-home-layout`, `nexus-local-db` |
| `nexus-cloud-domain` | User / Pairing **domain logic** (no HTTP) | `nexus-contracts` |

### Domain libraries (present in workspace; not yet linked from `nexus42`)

These crates were introduced in the V1.21 local/cloud split. They compile and have tests, but **no path from `nexus42` or `nexus-daemon-runtime` depends on them yet** (verified via `cargo tree`):

| Crate | Intended role | Current wiring |
| --- | --- | --- |
| `nexus-kb` | Narrative `KeyBlock` + `SourceAnchor` logic | Only used by `nexus-narrative` |
| `nexus-narrative` | Worlds, forks, timelines, manuscripts | Depends on `nexus-kb`; not linked from CLI/daemon |
| `nexus-knowledge` | Global reference KB (not narrative KeyBlocks) | Standalone; not linked from CLI/daemon |

CLI `nexus42 creator kb` today implements **work-scope file index** storage under the workspace tree — it does **not** call `nexus-kb` or `nexus-knowledge` yet. See audit notes in PM reports / `.agents/knowledge/specs/local-cloud-crate-architecture.md` §3.5.

### Executable surface

| Artifact | Crate | Notes |
| --- | --- | --- |
| **`nexus42`** | `crates/nexus42` | Sole user-facing binary; ACP **client** only |
| Daemon runtime | (library) `nexus-daemon-runtime` | Started via `nexus42 daemon start` / hidden `daemon-run` — not a separate product binary |
| ACP worker | `nexus42 acp-worker` (hidden) | Subprocess; uses `nexus-acp-host` |

`nexus42` enables `nexus-moment-context-assembly/cloud-stage` and `nexus-cloud-sync/legacy-sync` for CLI cloud workflows. Daemon builds use `nexus-daemon-runtime` without those cloud features on the runtime crate itself.

## Dependency graph (build-time, actual)

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

**Spec vs code gap:** [local-cloud-crate-architecture.md](../.agents/knowledge/specs/local-cloud-crate-architecture.md) §4 shows `cloud-sync → cloud-domain` and daemon hosting `kb` / `narrative` / `knowledge` / `moment-context-assembly` with full local deps. The graph above reflects **current** `Cargo.toml` edges; wiring the split crates into CLI/daemon is follow-up work (see deferred tracker DF-29–DF-41 and V1.21 compass).

## CLI command groups (frozen surface)

Six top-level groups ([`cli-spec.md`](../.agents/knowledge/specs/cli-spec.md) §6):

| Group | Role |
| --- | --- |
| `daemon` | Runtime lifecycle, schedules, orchestration control |
| `acp` | Registry, agents, skills, probe/doctor |
| `creator` | Identity, workspace, soul, memory, **work-scope kb index** |
| `sync` | Structured platform sync (`nexus-cloud-sync`) |
| `platform` | Auth, explore, context assemble, publish |
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

**Classes allowed on daemon:** runtime health, workspace, local creator listing, orchestration, agent-host, preset management, knowledge **index** endpoints as implemented.

**Classes forbidden on daemon:** `/sync/*`, platform registration proxies, public `/acp/*` as a server (ACP stays client/worker path).

## Versioning

- Wire: `schema_version` in schemas and bundle envelopes
- CLI / crate: SemVer per crate; breaking wire → coordinated bump of schemas, Rust crate, and npm major
- Pre-release (&lt;1.0): breaking CLI, paths, and on-disk layout allowed without migration

## Architecture constraints

- Daemon runtime is a **local supervisor**, not an ACP Agent/Server
- Default sync is **structured deltas/bundles**, not full manuscript upload
- **World history is immutable** — changes go through Fork, not in-place mutation
- Wire types in code must match `schemas/` (run `pnpm run codegen` after schema edits)

## Further reading

| Topic | Document |
| --- | --- |
| Crate responsibilities & forbidden deps | [local-cloud-crate-architecture.md](../.agents/knowledge/specs/local-cloud-crate-architecture.md) |
| Daemon layering | [daemon-runtime.md](../.agents/knowledge/specs/daemon-runtime.md) |
| Orchestration | [orchestration-engine.md](../.agents/knowledge/specs/orchestration-engine.md) |
| CLI behavior | [cli-spec.md](../.agents/knowledge/specs/cli-spec.md) |
| Spec index | [specs/README.md](../.agents/knowledge/specs/README.md) |
| Per-crate rules | `crates/*/AGENTS.md` |
| Deferred wiring / stubs | [deferred-features-cross-version-tracker.md](../.agents/knowledge/deferred-features-cross-version-tracker.md) |
