# Local–Cloud Crate Architecture (OSS)

## 0. Document position

| Attribute | Value |
| --- | --- |
| **Status** | **Historical crate-graph record (v1.193 P2).** The integrated daemon host and `nexus-daemon-runtime` crate were deleted; current topology authority is [rust-core-service-boundary.md](../architecture/rust-core-service-boundary.md). |
| **Document class** | Master |
| **Scope** | Historical local/cloud rules, crate responsibilities, contracts usage, dependency edges, current-vs-target wiring, and Daemon API classes; not the current workspace inventory or host topology. |
| **Scope model SSOT** | [entity-scope-model.md](../architecture/entity-scope-model.md) — authoritative for scope hierarchy, crate ownership, and `kb`/`knowledge` naming boundaries |
| **Related** | [entity-scope-model.md](../architecture/entity-scope-model.md), [local-runtime-boundary.md](../architecture/local-runtime-boundary.md), [daemon-runtime.md](daemon-runtime.md), [cli-spec.md](../cli/cli-spec.md), [schemas-directory-layout.md](../architecture/schemas-directory-layout.md), [schemas-external-consumer-boundary.md](../architecture/schemas-external-consumer-boundary.md) |

> **Historical as of v1.193 P2.** Sections §1–§9 retain the earlier crate-graph and product-integration record; “current”, “target”, and daemon ownership below refer to that record, not today's implementation. The retained `/v1/daemon/*` wire families are served by the standalone TypeScript service, `apps/nexus-service`, over Rust core authority, not by `nexus42 daemon`. Current topology: [rust-core-service-boundary.md](../architecture/rust-core-service-boundary.md). Current crate inventory and responsibilities: root [`AGENTS.md`](../../../AGENTS.md) and each crate's `AGENTS.md`, with workspace membership in root `Cargo.toml`.

**This file is not an implementation checklist.** Do not add migration batches, branch names, or “done by V1.21” task tables here — put those in the matching **iteration compass** and local plan documents (harness process artifacts, not tracked specs).

**Current-vs-target rule:** sections below explicitly separate **Cargo dependency wiring** from **product integration**. A Cargo edge is not a claim that CLI commands or daemon HTTP handlers use that crate at runtime.

---

## 1. Two product lines (frozen)

> **Historical host assignment.** The `nexus42 daemon` integration surface and `nexus-daemon-runtime` isolation edge below describe the deleted host; the local/cloud product separation does not reinstate it.

| Line | Purpose | Integration surface |
| --- | --- | --- |
| **Local product** | Orchestration, agent-host, workspace, Creator + Creator memory, World-scoped narrative KB, User-scoped knowledge, narrative graph, Moment context assembly | `nexus42 daemon` → `/v1/daemon/*` |
| **Cloud enhancement** | Platform HTTP, bundle sync, registration, optional context Stage-1, User/Pairing persistence | `nexus-cloud-sync` + CLI cloud subcommands — **never** Daemon API |

**Hard isolation:** `nexus-daemon-runtime` MUST NOT depend on `nexus-cloud-sync` and MUST NOT register HTTP handlers that perform platform HTTP or proxy sync.

**Verified current Cargo reality (2026-05-22):** `cargo tree -p nexus-daemon-runtime --edges normal --depth 1` shows direct local-domain edges to `nexus-creator-memory`, `nexus-narrative`, `nexus-knowledge`, and `nexus-moment-context-assembly`, and no `nexus-cloud-sync` or `nexus-cloud-domain` edge. The daemon/cloud forbidden-edge boundary remains satisfied. (Note: `nexus-kb` was merged into `nexus-knowledge` in V1.139.)

**Agent identity:** Operational actor for agents and orchestration is **`Creator`** (`creator_id`). `User` / `Pairing` are platform-bridge concepts only.

**Local Web UI (V1.64):** `apps/web` is part of the **local product** line. It is a bundled React/Vite SPA that consumes `/v1/daemon/*` through generated `@42ch/nexus-contracts` TypeScript types. It is not `nexus-platform`, does not call platform HTTP directly, and must not import or mirror private platform app assumptions.

---

## 2. Contracts boundary (frozen)

External wire shapes come from the **`nexus-contracts`** crate's generated modules under `src/generated/`, sourced from `schemas/` (layout: [schemas-directory-layout.md](../architecture/schemas-directory-layout.md)). Shared Rust-only DTOs live under `src/local/` per [schemas-external-consumer-boundary.md](../architecture/schemas-external-consumer-boundary.md).

| Rule | Detail |
| --- | --- |
| **No second DTO set** | Crates MUST NOT redefine structs that duplicate generated wire types (e.g. a hand-rolled `Creator` with the same fields as `nexus_contracts::Creator`). |
| **`nexus-creator`** | Imports wire/local contract types for Creator and related IDs; adds **domain logic** (validation, conversions, local cache records, path helpers). Any field that must match platform JSON MUST use contract types verbatim (`snake_case` on wire). |
| **`nexus-cloud-domain`** | Imports contract types for `User`, `Pairing`, and platform-bridge enums; adds **domain logic** only (invariants, mapping from platform responses). |
| **`nexus-cloud-sync`** | Uses contracts for request/response bodies on HTTP; no parallel HTTP DTOs in application crates. User/Pairing invariants MUST route through `nexus-cloud-domain`. |
| **Local-only types** | Internal orchestration engine state, ACP registry manifests, worker IPC, and daemon-only DTOs → `nexus-contracts/src/local/`. Cross-language Daemon API DTOs observed by the Web UI/desktop/generated clients → `schemas/daemon-api/<concern>/`. |

**Corollary:** If a type appears in a platform API or sync bundle, its definition lives in **`nexus-contracts`**, not in `nexus-creator` or `nexus-cloud-domain`.

---

## 3. Crate responsibility and scope ownership

> **Historical inventory.** Preserve this inventory and the 2026-05-22 Cargo snapshot in §4 as recorded, including the now-deleted daemon crate; neither is a current workspace list.

Post-2026-05 workspace additions — `nexus-core`, `nexus-core-node`, `nexus-preset`, `nexus-provider-conformance`, `nexus-provider-ports`, and `nexus-storage-guard` — are indexed with their per-crate `AGENTS.md` roles in [rust-core-service-boundary.md §2](../architecture/rust-core-service-boundary.md#current-workspace-boundary-inventory); they are not backfilled into this frozen inventory.

This table follows [entity-scope-model.md §4](../architecture/entity-scope-model.md#4-crate-ownership-map). If older wording conflicts with that model, the entity scope model is authoritative.

### 3.1 Foundation (types & paths)

These crates are **not** split by the local/cloud program; they sit **under** all application crates. See §2 for how application code must use them.

| Crate | Scope ownership | Responsibility boundary | `nexus-cloud-sync` dep? |
| --- | --- | --- | --- |
| **`nexus-contracts`** | Cross-scope type foundation | **Wire and local type SSOT** for the OSS monorepo: `src/generated/` from `schemas/` (`pnpm run codegen`) for external platform and Daemon API consumers; hand-written `src/local/` for Rust-only internal contracts; `enum_conversions.rs` for shared enums. **No business logic**, no HTTP, no I/O. Published to npm as `@42ch/nexus-contracts` for external TypeScript consumers; Rust crate is workspace-internal. | N/A (leaf type library) |
| **`nexus-home-layout`** | Storage paths for `User`/`Creator` local material | Frozen `~/.nexus42/` path resolution and safe path helpers only. No entity invariants. | N/A |
| **`nexus-local-db`** | Storage mechanics across Creator/workspace working copies | SQLite initialization, migration, versioning, and shared local persistence APIs. Does not own narrative or User semantics. | No |

**Rules for `nexus-contracts`:** application crates (`nexus-creator`, `nexus-cloud-domain`, `nexus-cloud-sync`, …) **depend on** `nexus-contracts`; they MUST NOT duplicate types defined in `generated/` or `local/`. External contract changes start in `schemas/` → codegen → then application/domain updates; Rust-only internal contracts stay in `src/local/`.

### 3.2 Application inventory (local + cloud)

| Crate | Scope ownership | Responsibility boundary | `nexus-cloud-sync` dep? |
| --- | --- | --- | --- |
| **`nexus-creator`** | `Creator` | Creator aggregate logic, credential/cache hooks, active Creator local state, and conversions over contract types. No platform HTTP. | No |
| **`nexus-creator-memory`** | `Creator` memory subdomain | Creator-scoped SOUL, long-term memory, review, personality, and experience I/O. | No — depends on **`nexus-creator`** |
| **`nexus-knowledge`** | `World` (narrative KB) + `User` (global knowledge) | Two-tier knowledge crate merged in V1.139 (former `nexus-kb`). World-scoped: narrative KnowledgeEntries, SourceAnchors, graph insertion/query. User-scoped: global knowledge/reference indexing and storage. Does not own Creator memory semantics. | No |
| **`nexus-narrative`** | `World`, `Timeline`, `Event` | Creative-work narrative state: current work background, world state, forks, timelines, events, story/manuscript projections, and narrative consistency. | No — currently depends on **`nexus-knowledge`** |
| **`nexus-cloud-domain`** | `User`, `Pairing` | Platform-bridge domain logic for User/Pairing invariants and mappings from contract types. No HTTP transport. | No HTTP; dependency of **`nexus-cloud-sync`** |
| **`nexus-moment-context-assembly`** | `Moment` | Per-moment, pre-session context aggregation. **`assemble_moment` is the single local CLI SSOT** (V1.28+): aggregates Creator memory, narrative state, World KB assets, and User knowledge via `nexus42 platform context assemble-moment`. Stage0 / degradation / optional two-stage behavior are flags on that command (`assemble-local` **removed** pre-release). User knowledge reads from **SQLite** (V1.27+). Optional `cloud-stage` may merge future platform context; direct platform cloud assembly remains deferred (tracked DR-51). | Only with `cloud-stage` |
| **`nexus-cloud-sync`** | Cloud transport for User/Pairing and sync bundles | Platform HTTP and sync transport. It MUST use `nexus-cloud-domain` for User/Pairing invariants. | N/A |
| **`nexus-daemon-runtime`** | Runtime host, not entity owner | Daemon API, lifecycle, DB handles, orchestration, and agent-host. It MUST NOT own cloud transport or platform User/Pairing invariants. | **Forbidden** |
| **`nexus-orchestration`** | Execution sessions/schedules, not hierarchy owner | Presets, schedules, workers, and capability registry. Carries `creator_id`/workspace/world references as execution context; does not redefine entity ownership. | No cloud-sync (sync capabilities stubbed locally) |
| **`nexus42`** | CLI surface | User-facing command routing and wording. It invokes owning crates; it MUST NOT become a second domain implementation for scope rules. | CLI may use cloud-sync for cloud commands |

### 3.2A Local Web UI app (V1.64)

> **Historical serving model (retired in v1.193 P2).** The `rust-embed`/daemon-router edge below is deleted, not a current build or serving instruction. Current browser/desktop host boundaries are in [rust-core-service-boundary.md](../architecture/rust-core-service-boundary.md) and [desktop-shell.md](../surfaces/desktop-shell.md).

| App | Product line | Responsibility boundary | Cloud/platform dep? |
| --- | --- | --- | --- |
| **`apps/web`** | Local product | Browser SPA for Control Room + Setup. Consumes daemon `/v1/daemon/*` via `@42ch/nexus-contracts` generated TS types and a `NexusClient` transport boundary. Build output (`dist/`) is served by the daemon in release and proxied to the daemon in dev. | **No** direct platform/cloud dependency; no private `nexus-platform` imports or assumptions. |

Embedding edge:

```text
apps/web (Vite build → dist/)
  └─ embedded by rust-embed at release build
      └─ nexus42 binary / nexus-daemon-runtime router static serving
```

`rust-embed` is a build-time/static-asset edge only. It does not make the Web UI an owning Rust crate and does not permit the frontend to bypass the Daemon API. `tower-http::ServeDir`-style serving may expose the unauthenticated SPA shell, but data remains behind `/v1/daemon/*` auth boundaries (see [daemon-runtime.md](daemon-runtime.md) §4.4).

### 3.3 Why `nexus-cloud-domain` (not `nexus-domain`)

The historical **`nexus-domain`** name implied “all domain logic” and encouraged platform types to leak across the monorepo.

**Decision (frozen):** the narrowed platform-bridge crate is named **`nexus-cloud-domain`** to:

1. Pair symmetrically with **`nexus-cloud-sync`** (transport vs domain logic).
2. Make dependency reviews obvious: anything importing `nexus-cloud-domain` is on the cloud line.
3. Avoid resurrecting the old god-crate mental model.

The legacy crate name `nexus-domain` is **not** retained after the split program completes.

### 3.4 `nexus-creator` vs `nexus-cloud-domain`

| | **`nexus-creator`** | **`nexus-cloud-domain`** |
| --- | --- | --- |
| **Actor** | Creator (agent-facing) | User, Pairing (account bridge) |
| **Contracts** | `Creator`, `CreatorId`, creator-local records | `User`, `Pairing`, platform pairing enums |
| **Typical callers** | daemon, orchestration, creator-memory, local product modules | cloud-sync, CLI after registration |
| **HTTP** | Never | Never (cloud-sync owns HTTP) |

### 3.5 `nexus-knowledge` — two-tier knowledge (merged from `nexus-kb`)

- **`nexus-knowledge` (World-scoped):** Narrative KB graph assets (KnowledgeEntries, SourceAnchors, graph insertion/query) coordinated with `nexus-narrative`. Formerly the separate `nexus-kb` crate (merged in V1.139).
- **`nexus-knowledge` (User-scoped):** Global knowledge/reference material. Tag-driven, may be pulled into Moment context assembly. Not Creator-scoped, does not own World narrative KnowledgeEntries.
- **CLI `creator kb`:** today is a local work-scope file/index workflow under the active Creator/workspace. It is not equivalent to the World-scoped narrative KB model until later tasks route or rename it. **Durable roadmap:** DR-46.

### 3.6 `nexus-moment-context-assembly`

> **Historical product status.** The daemon route/default-build claims below belong to the retired host; retained TS-service context routes and Rust owners are described in [rust-core-service-boundary.md §7.5](../architecture/rust-core-service-boundary.md#75-family-destinations-program-keys-not-extra-iterations).

- **Shipped local four-domain Moment path (V1.26+, SSOT V1.28):** `assemble_moment` depends on `nexus-creator-memory`, `nexus-narrative`, `nexus-knowledge`, and `nexus-contracts`. (The former `nexus-kb` crate was merged into `nexus-knowledge` in V1.139.) `nexus42 platform context assemble-moment` is the **single** local assembly command; it calls `assemble_moment` in-process with persistent narrative / World KB / User knowledge stores (SQLite User knowledge since V1.27).
- **Stage0 / TwoStage on assemble-moment (V1.28):** `--max-tokens`, `--no-fragments`, `--hint`, and runtime/degradation routing are flags on `assemble-moment`, not a separate subcommand.
- **Removed path:** `nexus42 platform context assemble-local` was removed in V1.28 (pre-release breaking change).
- **Deferred platform cloud path:** `nexus42 platform context assemble` is not yet available as direct platform cloud assembly and should guide users to `assemble-moment`.
- **Daemon product status:** the daemon intentionally does **not** expose context assembly after the V1.24 KCA-002 B2 decision; no daemon context-assemble proxy route should be reintroduced.
- **Stage-1 (optional):** `cloud-stage` feature can call cloud-sync for platform context; not used on daemon default build.

```toml
[features]
default = []
cloud-stage = ["dep:nexus-cloud-sync"]
```

### 3.7 Retired crate names

| Old | New |
| --- | --- |
| `nexus-sync` | `nexus-cloud-sync` |
| `nexus-memory` | `nexus-creator-memory` |
| `nexus-domain` (monolith) | Split; platform slice → **`nexus-cloud-domain`** |

---

## 4. Historical Cargo graph (snapshot verified 2026-05-22)

> **Frozen historical snapshot.** The original table, diagram, and “current” claims below are preserved verbatim as the 2026-05-22 record, not a graph to build today; `nexus-daemon-runtime` and its embedded-SPA edge were deleted in v1.193 P2.

This section describes the current `Cargo.toml` and `cargo tree` reality. It is intentionally separate from product integration gaps in §6 and the V1.24 audit compass.

### 4.1 Current direct dependencies for alignment-sensitive crates

| Crate | Currently wired direct workspace dependencies | Current product reachability / notes |
| --- | --- | --- |
| `nexus42` | `nexus-acp-host`, `nexus-cloud-sync` with `legacy-sync`, `nexus-contracts`, `nexus-creator`, `nexus-creator-memory`, `nexus-daemon-runtime`, `nexus-home-layout`, `nexus-local-db`, `nexus-moment-context-assembly` with `cloud-stage`, `nexus-orchestration` | CLI currently reaches cloud-sync and moment assembly. Because the CLI enables `cloud-stage`, `cargo tree -p nexus42` shows `nexus-moment-context-assembly -> nexus-cloud-sync`; this is CLI/cloud-line reachability, not daemon reachability. |
| `nexus-daemon-runtime` | `nexus-agent-host`, `nexus-contracts`, `nexus-creator`, `nexus-creator-memory`, `nexus-home-layout`, `nexus-knowledge`, `nexus-local-db`, `nexus-moment-context-assembly`, `nexus-narrative`, `nexus-orchestration` | Cargo-wired to the local domain graph with `nexus-moment-context-assembly` default features only. No daemon edge to `nexus-cloud-sync` or `nexus-cloud-domain`. Product wiring remains partial: daemon handlers do not expose moment assembly or narrative/user-knowledge domain HTTP yet. |
| `apps/web` | npm workspace app consuming `@42ch/nexus-contracts` via workspace version and browser build tooling | Local Web UI product surface. Runtime data access is `/v1/daemon/*` only; release assets are embedded into `nexus42`/`nexus-daemon-runtime` static serving. Not a cloud app and not part of private `nexus-platform`. |
| `nexus-moment-context-assembly` | `nexus-contracts`, `nexus-creator-memory`, `nexus-knowledge`, `nexus-narrative`; optional `nexus-cloud-sync` behind `cloud-stage` | Four-domain Moment library dependencies are wired. Current CLI Stage-0/TwoStage product flow remains narrower and does not call `assemble_moment`; CLI can enable `cloud-stage`, daemon default build does not. |
| `nexus-narrative` | `nexus-contracts`, `nexus-knowledge` | World/Timeline/Event domain library wired to World KB (`nexus-knowledge`). No dedicated daemon narrative routes yet. |
| `nexus-knowledge` | `nexus-contracts` | Two-tier knowledge crate (World KB + User knowledge); reachable from narrative, moment assembly, and daemon Cargo graph. Daemon `/v1/daemon/kb/*` remains the CLI local work KB file index, not the World-scoped narrative KB. |
| `nexus-knowledge` | `nexus-contracts` | User knowledge/reference-source library; reachable from moment assembly and daemon Cargo graph. `GET /v1/daemon/references` still uses `nexus-local-db`, not this crate. |
| `nexus-cloud-domain` | `nexus-contracts` | Cloud-domain library for User/Pairing invariants. |
| `nexus-cloud-sync` | `nexus-cloud-domain`, `nexus-contracts`, `nexus-home-layout`, `nexus-local-db` | Cloud HTTP/sync transport is wired to `nexus-cloud-domain`; this is CLI/cloud-line only, not daemon reachability. |

### 4.2 Current wiring diagram

```text
schemas/ ──codegen──► nexus-contracts

nexus42 ──┬── nexus-daemon-runtime ──┬── nexus-agent-host
          │                          ├── nexus-creator
          │                          ├── nexus-creator-memory
          │                          ├── nexus-local-db
          │                          ├── nexus-orchestration
          │                          ├── nexus-narrative ──► nexus-knowledge
          │                          ├── nexus-knowledge
          │                          ├── nexus-moment-context-assembly
          │                          ├── nexus-contracts
          │                          └── nexus-home-layout
          ├── nexus-cloud-sync (legacy-sync enabled by CLI)
          │   └── nexus-cloud-domain
          ├── nexus-moment-context-assembly (cloud-stage enabled by CLI)
          │   ├── nexus-creator-memory
          │   ├── nexus-narrative ──► nexus-knowledge
          │   ├── nexus-knowledge
          │   └── [cloud-stage] nexus-cloud-sync
          ├── nexus-creator-memory ── nexus-creator
          ├── nexus-creator
          ├── nexus-local-db
          └── nexus-orchestration

apps/web ──► @42ch/nexus-contracts (workspace TS types)
         └── build dist ──► rust-embed ──► nexus42/nexus-daemon-runtime static route

nexus-narrative ──► nexus-knowledge ──► nexus-contracts
nexus-knowledge ──► nexus-contracts
nexus-cloud-domain ──► nexus-contracts
nexus-cloud-sync ──► nexus-cloud-domain, nexus-contracts, nexus-home-layout, nexus-local-db
```

**Current daemon/cloud boundary:** `nexus-daemon-runtime` has no `nexus-cloud-sync` or `nexus-cloud-domain` edge. This matches the forbidden-edge policy.

**Current Web/cloud boundary (V1.64):** `apps/web` is local-only. It consumes generated Daemon API contracts and daemon loopback routes; it must not share code or runtime dependencies with private `nexus-platform` cloud surfaces.

---

## 5. V1.23 dependency wiring target (achieved for Cargo edges)

> **Historical V1.23 target and results.** “Current Cargo shape”, “already wired”, and daemon constraints below are statements at that milestone, not current runtime ownership.

The following graph was the normative V1.23 dependency target and is now the current Cargo shape for the alignment-sensitive edges. Remaining gaps are product integration gaps, not missing Cargo dependencies.

### 5.1 Target dependency shape

```text
schemas/ ──codegen──► nexus-contracts

nexus42
  ├── nexus-daemon-runtime
  │   ├── nexus-orchestration
  │   ├── nexus-agent-host
  │   ├── nexus-creator
  │   ├── nexus-creator-memory
  │   ├── nexus-narrative
  │   ├── nexus-knowledge
  │   ├── nexus-moment-context-assembly (default features only)
  │   └── nexus-local-db
  ├── nexus-cloud-sync
  │   └── nexus-cloud-domain
  └── nexus-moment-context-assembly (cloud-stage only for CLI/platform flows)

nexus-moment-context-assembly (default four-domain library target)
  ├── nexus-creator-memory
  ├── nexus-narrative
  ├── nexus-knowledge
  └── nexus-contracts
```

**Daemon target constraint:** if `nexus-moment-context-assembly` keeps optional `cloud-stage`, daemon wiring MUST use default features only. The daemon target MUST still have no `nexus-cloud-sync`, no `nexus-cloud-domain`, and no platform HTTP path.

### 5.2 V1.23 alignment results

| Crate pair / path | Cargo status (2026-05-22) | Product note |
| --- | --- | --- |
| `nexus-cloud-sync -> nexus-cloud-domain` | **Wired.** | Cloud transport must route User/Pairing invariants through `nexus-cloud-domain`. |
| `nexus-moment-context-assembly -> nexus-narrative` | **Wired.** | Full `assemble_moment` may read narrative World/Timeline/Event context through `nexus-narrative`; current CLI Stage-0/TwoStage flow does not call this four-domain path. |
| `nexus-moment-context-assembly -> nexus-knowledge` (World KB) | **Wired.** | Full `assemble_moment` may include World-scoped narrative KB slices; current CLI Stage-0/TwoStage flow does not call this four-domain path. |
| `nexus-moment-context-assembly -> nexus-knowledge` | **Wired.** | Full `assemble_moment` may include selected User-scoped knowledge slices; current CLI Stage-0/TwoStage flow does not call this four-domain path. |
| `nexus-daemon-runtime -> nexus-moment-context-assembly` | **Wired with default features only.** | No daemon `cloud-stage`; KCA-002 B2 retires the daemon context-assemble route. |
| `nexus-daemon-runtime -> nexus-narrative` | **Wired.** | No dedicated narrative HTTP routes yet. |
| `nexus-daemon-runtime -> nexus-knowledge` (World KB) | **Wired.** | `/v1/daemon/kb/*` is still work-scope file index, not World KB (`nexus-knowledge`) integration. |
| `nexus-daemon-runtime -> nexus-knowledge` | **Wired.** | `GET /v1/daemon/references` still uses `nexus-local-db`; user knowledge store is not daemon-product wired. |
| CLI `creator kb` -> World-scoped narrative KB semantics | **Not a Cargo gap.** | KCA-003 C2 keeps `/v1/daemon/kb/*` and `creator kb` as `scope=work` only; future World KB behavior must route to `nexus-knowledge` + `nexus-narrative` (DR-46). |

### 5.3 Edges that are already wired and should remain

| Edge | Current reality | Target note |
| --- | --- | --- |
| `nexus-narrative -> nexus-knowledge` (World KB) | Currently wired. | Remains the narrative aggregate's World KB dependency (`nexus-kb` merged into `nexus-knowledge` in V1.139). |
| `nexus-creator-memory -> nexus-creator` | Currently wired. | Remains Creator memory subdomain dependency. |
| `nexus42 -> nexus-cloud-sync` | Currently wired with `legacy-sync`. | Remains CLI/cloud-line only; not a daemon path. |
| `nexus42 -> nexus-moment-context-assembly` with `cloud-stage` | Currently wired for CLI/platform flows. | Allowed only outside daemon default build; daemon target uses default features. |

---

## 6. Daemon API (principles)

> **Historical router authority.** `crates/nexus-daemon-runtime/src/api/mod.rs` below was deleted with its crate in v1.193 P2. Current retained route composition is [`apps/nexus-service/src/routes.ts`](../../../apps/nexus-service/src/routes.ts) and its family modules; domain/effect ownership is Rust core. The V1.24 gap list below remains a historical audit record, not a list of today's missing routes.

Authoritative route list for a given release lives in **`crates/nexus-daemon-runtime/src/api/mod.rs`** and the active **iteration compass**.

**Always allowed (local product):** runtime health/status, workspace, local creator listing/active/logout, local references, work-scope KB file-index APIs, memory pending-review, presets, orchestration, and agent-host (+ internal tool execution). Future World KB / User knowledge / Moment context surfaces may be local-only, but must be explicitly registered and documented (DR-46); after KCA-002 B2, daemon context assembly is not an active Daemon API route.

**Always forbidden on daemon:** `/sync/*`, `/creators/registrations*`, platform world/explore proxies, public `/acp/*` (use agent-host namespace), `nexus-cloud-sync`, `nexus-cloud-domain`, and platform HTTP paths.

Auth model: see V1.20 delivery compass (`X-API-Key`, keyless-localhost).

### 6.1 V1.24 product-integration gap cross-links

These are runtime/product gaps after Cargo alignment, not missing dependency edges:

| Gap | Boundary impact | V1.24 audit cross-link |
| --- | --- | --- |
| Daemon context assembly route retired | Historical `POST /v1/local/context/assemble` is not registered and was retired by KCA-002 B2; context assembly stays CLI in-process. | KCA-002 |
| Work KB path remains work-scoped | `/v1/daemon/kb/*` and `creator kb` are `scope=work` local file-index APIs only, not World KB (`nexus-knowledge`) APIs. | KCA-003 |
| Domain crates are only partially product-wired | `nexus-narrative`, `nexus-knowledge`, and moment assembly are linked in Cargo but not fully surfaced through daemon HTTP/product workflows. | KCA-004/KCA-005 |

---

## 7. CLI integration (principles)

> **Historical CLI assignment.** The daemon-control row below records the deleted CLI/API integration; current CLI disposition is [rust-core-service-boundary.md §7.2](../architecture/rust-core-service-boundary.md#72-operator--service-lifecycle), with no TS-service launcher alias.

| Concern | Owner |
| --- | --- |
| Daemon control | Daemon API |
| Creator register/verify | `nexus-cloud-sync` (+ persist via Creator local state and `nexus-cloud-domain` target invariants) |
| Bundle sync | `nexus-cloud-sync` (`legacy-sync` until redesigned) |
| `local_only` context | `nexus-moment-context-assembly` Stage-0 |
| World-scoped narrative KB | `nexus-knowledge` (World KB) + `nexus-narrative` |
| User-scoped global knowledge | `nexus-knowledge` |

---

## 8. Orchestration

> **Historical daemon-build policy.** No daemon build survives v1.193 P2; current execution/transport ownership is defined in [rust-core-service-boundary.md](../architecture/rust-core-service-boundary.md).

Built-in `sync.*` / `outbox.flush` capabilities on **daemon builds** MUST NOT call cloud-sync; stubs or explicit “cloud line disabled” results are acceptable until cloud orchestration is redesigned.

Workspace file writes remain agent-mediated (agent-host internal tool execution); unchanged principle from preset-driven architecture.

---

## 9. Cloud runtime policy

> **Historical host wording.** “Daemon hot path” below refers to the retired host, not a surviving process mode.

`runtime_mode`, `degradation`, and platform health probing belong to the **cloud line** (CLI / `cloud-stage` builds), not the daemon hot path.

---

*Historical local/cloud crate and Daemon API boundary record; current topology SSOT: [rust-core-service-boundary.md](../architecture/rust-core-service-boundary.md).*
