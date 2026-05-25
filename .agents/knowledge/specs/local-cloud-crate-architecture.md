# Local–Cloud Crate Architecture (OSS)

## 0. Document position

| Attribute | Value |
| --- | --- |
| **Status** | Active — long-term normative SSOT, reconciled with the V1.23 entity scope model |
| **Scope** | Stable rules: local vs cloud product lines, crate responsibilities, contracts usage, dependency forbidden edges, current-vs-target wiring, Local API *classes* allowed/forbidden |
| **Scope model SSOT** | [entity-scope-model.md](./entity-scope-model.md) — authoritative for scope hierarchy, crate ownership, and `kb`/`knowledge` naming boundaries |
| **Delivery compass** | Iteration-scoped milestones, phases, acceptance tests → [v1.21-local-platform-isolation-delivery-compass-v1.md](../../iterations/v1.21-local-platform-isolation-delivery-compass-v1.md) |
| **Related** | [entity-scope-model.md](./entity-scope-model.md), [local-runtime-boundary.md](./local-runtime-boundary.md), [daemon-runtime.md](./daemon-runtime.md), [cli-spec.md](./cli-spec.md), [schemas-directory-layout.md](./schemas-directory-layout.md), [../schemas-wire-platform-sync-boundary.md](../schemas-wire-platform-sync-boundary.md) |

**This file is not an implementation checklist.** Do not add migration batches, branch names, or “done by V1.21” task tables here — put those in the matching **iteration compass** and `.agents/plans/`.

**Current-vs-target rule:** sections below explicitly separate **Cargo dependency wiring** from **product integration**. A Cargo edge is not a claim that CLI commands or daemon HTTP handlers use that crate at runtime.

---

## 1. Two product lines (frozen)

| Line | Purpose | Integration surface |
| --- | --- | --- |
| **Local product** | Orchestration, agent-host, workspace, Creator + Creator memory, World-scoped narrative KB, User-scoped knowledge, narrative graph, Moment context assembly | `nexus42 daemon` → `/v1/local/*` |
| **Cloud enhancement** | Platform HTTP, bundle sync, registration, optional context Stage-1, User/Pairing persistence | `nexus-cloud-sync` + CLI cloud subcommands — **never** daemon Local API |

**Hard isolation:** `nexus-daemon-runtime` MUST NOT depend on `nexus-cloud-sync` and MUST NOT register HTTP handlers that perform platform HTTP or proxy sync.

**Verified current Cargo reality (2026-05-22):** `cargo tree -p nexus-daemon-runtime --edges normal --depth 1` shows direct local-domain edges to `nexus-creator-memory`, `nexus-narrative`, `nexus-kb`, `nexus-knowledge`, and `nexus-moment-context-assembly`, and no `nexus-cloud-sync` or `nexus-cloud-domain` edge. The daemon/cloud forbidden-edge boundary remains satisfied.

**Agent identity:** Operational actor for agents and orchestration is **`Creator`** (`creator_id`). `User` / `Pairing` are platform-bridge concepts only.

---

## 2. Contracts boundary (frozen)

All **wire shapes** and **platform-aligned DTOs** come from the **`nexus-contracts`** crate (§3.1): generated from `schemas/` (layout: [schemas-directory-layout.md](./schemas-directory-layout.md)) or hand-written under `src/local/` per [schemas-wire-platform-sync-boundary.md](../schemas-wire-platform-sync-boundary.md).

| Rule | Detail |
| --- | --- |
| **No second DTO set** | Crates MUST NOT redefine structs that duplicate generated wire types (e.g. a hand-rolled `Creator` with the same fields as `nexus_contracts::Creator`). |
| **`nexus-creator`** | Imports wire/local contract types for Creator and related IDs; adds **domain logic** (validation, conversions, local cache records, path helpers). Any field that must match platform JSON MUST use contract types verbatim (`snake_case` on wire). |
| **`nexus-cloud-domain`** | Imports contract types for `User`, `Pairing`, and platform-bridge enums; adds **domain logic** only (invariants, mapping from platform responses). |
| **`nexus-cloud-sync`** | Uses contracts for request/response bodies on HTTP; no parallel HTTP DTOs in application crates. User/Pairing invariants MUST route through `nexus-cloud-domain`. |
| **Local-only types** | Schedule, daemon status, orchestration HTTP, assembly DTOs → `nexus-contracts/src/local/` (not `schemas/`) unless platform later observes them. |

**Corollary:** If a type appears in a platform API or sync bundle, its definition lives in **`nexus-contracts`**, not in `nexus-creator` or `nexus-cloud-domain`.

---

## 3. Crate responsibility and scope ownership

This table follows [entity-scope-model.md §4](./entity-scope-model.md#4-crate-ownership-map). If older wording conflicts with that model, the entity scope model is authoritative.

### 3.1 Foundation (types & paths)

These crates are **not** split by the local/cloud program; they sit **under** all application crates. See §2 for how application code must use them.

| Crate | Scope ownership | Responsibility boundary | `nexus-cloud-sync` dep? |
| --- | --- | --- | --- |
| **`nexus-contracts`** | Cross-scope type foundation | **Wire and local type SSOT** for the OSS monorepo: `src/generated/` from `schemas/` (`pnpm run codegen`); hand-written `src/local/` for CLI↔daemon and orchestration types not observed by platform; `enum_conversions.rs` for shared enums. **No business logic**, no HTTP, no I/O. Published to npm as `@42ch/nexus-contracts` for `nexus-platform`; Rust crate is workspace-internal. | N/A (leaf type library) |
| **`nexus-home-layout`** | Storage paths for `User`/`Creator` local material | Frozen `~/.nexus42/` path resolution and safe path helpers only. No entity invariants. | N/A |
| **`nexus-local-db`** | Storage mechanics across Creator/workspace working copies | SQLite initialization, migration, versioning, and shared local persistence APIs. Does not own narrative or User semantics. | No |

**Rules for `nexus-contracts`:** application crates (`nexus-creator`, `nexus-cloud-domain`, `nexus-cloud-sync`, …) **depend on** `nexus-contracts`; they MUST NOT duplicate types defined in `generated/` or `local/`. Schema changes start in `schemas/` → codegen → then domain logic updates.

### 3.2 Application inventory (local + cloud)

| Crate | Scope ownership | Responsibility boundary | `nexus-cloud-sync` dep? |
| --- | --- | --- | --- |
| **`nexus-creator`** | `Creator` | Creator aggregate logic, credential/cache hooks, active Creator local state, and conversions over contract types. No platform HTTP. | No |
| **`nexus-creator-memory`** | `Creator` memory subdomain | Creator-scoped SOUL, long-term memory, review, personality, and experience I/O. | No — depends on **`nexus-creator`** |
| **`nexus-kb`** | World-scoped narrative KB graph | Narrative knowledge assets under `World`: KeyBlocks, SourceAnchors, graph insertion/query, and narrative KB lifecycle. It is not generic Creator knowledge or User knowledge. | No |
| **`nexus-knowledge`** | `User` knowledge | User-scoped global knowledge/reference indexing and storage. It may feed Moment context assembly. It does not own narrative KeyBlocks and is not Creator-scoped. | No |
| **`nexus-narrative`** | `World`, `Timeline`, `Event` | Creative-work narrative state: current work background, world state, forks, timelines, events, story/manuscript projections, and narrative consistency. | No — currently depends on **`nexus-kb`** |
| **`nexus-cloud-domain`** | `User`, `Pairing` | Platform-bridge domain logic for User/Pairing invariants and mappings from contract types. No HTTP transport. | No HTTP; dependency of **`nexus-cloud-sync`** |
| **`nexus-moment-context-assembly`** | `Moment` | Per-moment, pre-session context aggregation. **`assemble_moment` is the single local CLI SSOT** (V1.28+): aggregates Creator memory, narrative state, World KB assets, and User knowledge via `nexus42 platform context assemble-moment`. Stage0 / degradation / optional two-stage behavior are flags on that command (`assemble-local` **removed** pre-release). User knowledge reads from **SQLite** (V1.27+). Optional `cloud-stage` may merge future platform context; direct platform cloud assembly remains deferred. | Only with `cloud-stage` |
| **`nexus-cloud-sync`** | Cloud transport for User/Pairing and sync bundles | Platform HTTP and sync transport. It MUST use `nexus-cloud-domain` for User/Pairing invariants. | N/A |
| **`nexus-daemon-runtime`** | Runtime host, not entity owner | Local API, lifecycle, DB handles, orchestration, and agent-host. It MUST NOT own cloud transport or platform User/Pairing invariants. | **Forbidden** |
| **`nexus-orchestration`** | Execution sessions/schedules, not hierarchy owner | Presets, schedules, workers, and capability registry. Carries `creator_id`/workspace/world references as execution context; does not redefine entity ownership. | No cloud-sync (sync capabilities stubbed locally) |
| **`nexus42`** | CLI surface | User-facing command routing and wording. It invokes owning crates; it MUST NOT become a second domain implementation for scope rules. | CLI may use cloud-sync for cloud commands |

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

### 3.5 `nexus-kb` vs `nexus-knowledge`

- **`nexus-kb`:** World-scoped narrative KB graph assets (KeyBlocks, SourceAnchors, graph insertion/query) coordinated with `nexus-narrative`.
- **`nexus-knowledge`:** User-scoped global knowledge/reference material. It is tag-driven and may be pulled into Moment context assembly. It is not Creator-scoped and does not own World KeyBlocks.
- **CLI `creator kb`:** today is a local work-scope file/index workflow under the active Creator/workspace. It is not equivalent to `nexus-kb` or `nexus-knowledge` until later tasks route or rename it.

### 3.6 `nexus-moment-context-assembly`

- **Shipped local four-domain Moment path (V1.26+, SSOT V1.28):** `assemble_moment` depends on `nexus-creator-memory`, `nexus-narrative`, `nexus-kb`, `nexus-knowledge`, and `nexus-contracts`. `nexus42 platform context assemble-moment` is the **single** local assembly command; it calls `assemble_moment` in-process with persistent narrative / World KB / User knowledge stores (SQLite User knowledge since V1.27).
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

## 4. Currently wired Cargo graph (verified 2026-05-22)

This section describes the current `Cargo.toml` and `cargo tree` reality. It is intentionally separate from product integration gaps in §6 and the V1.24 audit compass.

### 4.1 Current direct dependencies for alignment-sensitive crates

| Crate | Currently wired direct workspace dependencies | Current product reachability / notes |
| --- | --- | --- |
| `nexus42` | `nexus-acp-host`, `nexus-cloud-sync` with `legacy-sync`, `nexus-contracts`, `nexus-creator`, `nexus-creator-memory`, `nexus-daemon-runtime`, `nexus-home-layout`, `nexus-local-db`, `nexus-moment-context-assembly` with `cloud-stage`, `nexus-orchestration` | CLI currently reaches cloud-sync and moment assembly. Because the CLI enables `cloud-stage`, `cargo tree -p nexus42` shows `nexus-moment-context-assembly -> nexus-cloud-sync`; this is CLI/cloud-line reachability, not daemon reachability. |
| `nexus-daemon-runtime` | `nexus-agent-host`, `nexus-contracts`, `nexus-creator`, `nexus-creator-memory`, `nexus-home-layout`, `nexus-kb`, `nexus-knowledge`, `nexus-local-db`, `nexus-moment-context-assembly`, `nexus-narrative`, `nexus-orchestration` | Cargo-wired to the local domain graph with `nexus-moment-context-assembly` default features only. No daemon edge to `nexus-cloud-sync` or `nexus-cloud-domain`. Product wiring remains partial: daemon handlers do not expose moment assembly or narrative/user-knowledge domain HTTP yet. |
| `nexus-moment-context-assembly` | `nexus-contracts`, `nexus-creator-memory`, `nexus-kb`, `nexus-knowledge`, `nexus-narrative`; optional `nexus-cloud-sync` behind `cloud-stage` | Four-domain Moment library dependencies are wired. Current CLI Stage-0/TwoStage product flow remains narrower and does not call `assemble_moment`; CLI can enable `cloud-stage`, daemon default build does not. |
| `nexus-narrative` | `nexus-contracts`, `nexus-kb` | World/Timeline/Event domain library wired to World KB. No dedicated daemon narrative routes yet. |
| `nexus-kb` | `nexus-contracts` | World KB graph library; reachable from narrative, moment assembly, and daemon Cargo graph. Daemon `/v1/local/kb/*` remains the CLI local work KB file index, not `nexus-kb`. |
| `nexus-knowledge` | `nexus-contracts` | User knowledge/reference-source library; reachable from moment assembly and daemon Cargo graph. `GET /v1/local/references` still uses `nexus-local-db`, not this crate. |
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
          │                          ├── nexus-narrative ──► nexus-kb
          │                          ├── nexus-kb
          │                          ├── nexus-knowledge
          │                          ├── nexus-moment-context-assembly
          │                          ├── nexus-contracts
          │                          └── nexus-home-layout
          ├── nexus-cloud-sync (legacy-sync enabled by CLI)
          │   └── nexus-cloud-domain
          ├── nexus-moment-context-assembly (cloud-stage enabled by CLI)
          │   ├── nexus-creator-memory
          │   ├── nexus-narrative ──► nexus-kb
          │   ├── nexus-kb
          │   ├── nexus-knowledge
          │   └── [cloud-stage] nexus-cloud-sync
          ├── nexus-creator-memory ── nexus-creator
          ├── nexus-creator
          ├── nexus-local-db
          └── nexus-orchestration

nexus-narrative ──► nexus-kb ──► nexus-contracts
nexus-knowledge ──► nexus-contracts
nexus-cloud-domain ──► nexus-contracts
nexus-cloud-sync ──► nexus-cloud-domain, nexus-contracts, nexus-home-layout, nexus-local-db
```

**Current daemon/cloud boundary:** `nexus-daemon-runtime` has no `nexus-cloud-sync` or `nexus-cloud-domain` edge. This matches the forbidden-edge policy.

---

## 5. V1.23 dependency wiring target (achieved for Cargo edges)

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
  │   ├── nexus-kb
  │   ├── nexus-knowledge
  │   ├── nexus-moment-context-assembly (default features only)
  │   └── nexus-local-db
  ├── nexus-cloud-sync
  │   └── nexus-cloud-domain
  └── nexus-moment-context-assembly (cloud-stage only for CLI/platform flows)

nexus-moment-context-assembly (default four-domain library target)
  ├── nexus-creator-memory
  ├── nexus-narrative
  ├── nexus-kb
  ├── nexus-knowledge
  └── nexus-contracts
```

**Daemon target constraint:** if `nexus-moment-context-assembly` keeps optional `cloud-stage`, daemon wiring MUST use default features only. The daemon target MUST still have no `nexus-cloud-sync`, no `nexus-cloud-domain`, and no platform HTTP path.

### 5.2 V1.23 alignment results

| Crate pair / path | Cargo status (2026-05-22) | Product note |
| --- | --- | --- |
| `nexus-cloud-sync -> nexus-cloud-domain` | **Wired.** | Cloud transport must route User/Pairing invariants through `nexus-cloud-domain`. |
| `nexus-moment-context-assembly -> nexus-narrative` | **Wired.** | Full `assemble_moment` may read narrative World/Timeline/Event context through `nexus-narrative`; current CLI Stage-0/TwoStage flow does not call this four-domain path. |
| `nexus-moment-context-assembly -> nexus-kb` | **Wired.** | Full `assemble_moment` may include World-scoped narrative KB slices; current CLI Stage-0/TwoStage flow does not call this four-domain path. |
| `nexus-moment-context-assembly -> nexus-knowledge` | **Wired.** | Full `assemble_moment` may include selected User-scoped knowledge slices; current CLI Stage-0/TwoStage flow does not call this four-domain path. |
| `nexus-daemon-runtime -> nexus-moment-context-assembly` | **Wired with default features only.** | No daemon `cloud-stage`; KCA-002 B2 retires the daemon context-assemble route. |
| `nexus-daemon-runtime -> nexus-narrative` | **Wired.** | No dedicated narrative HTTP routes yet. |
| `nexus-daemon-runtime -> nexus-kb` | **Wired.** | `/v1/local/kb/*` is still work-scope file index, not World KB (`nexus-kb`) integration. |
| `nexus-daemon-runtime -> nexus-knowledge` | **Wired.** | `GET /v1/local/references` still uses `nexus-local-db`; user knowledge store is not daemon-product wired. |
| CLI `creator kb` -> World-scoped `nexus-kb` semantics | **Not a Cargo gap.** | KCA-003 C2 keeps `/v1/local/kb/*` and `creator kb` as `scope=work` only; future World KB behavior must route to `nexus-kb` + `nexus-narrative`. |

### 5.3 Edges that are already wired and should remain

| Edge | Current reality | Target note |
| --- | --- | --- |
| `nexus-narrative -> nexus-kb` | Currently wired. | Remains the narrative aggregate’s World KB dependency. |
| `nexus-creator-memory -> nexus-creator` | Currently wired. | Remains Creator memory subdomain dependency. |
| `nexus42 -> nexus-cloud-sync` | Currently wired with `legacy-sync`. | Remains CLI/cloud-line only; not a daemon path. |
| `nexus42 -> nexus-moment-context-assembly` with `cloud-stage` | Currently wired for CLI/platform flows. | Allowed only outside daemon default build; daemon target uses default features. |

---

## 6. Daemon Local API (principles)

Authoritative route list for a given release lives in **`crates/nexus-daemon-runtime/src/api/mod.rs`** and the active **iteration compass**.

**Always allowed (local product):** runtime health/status, workspace, local creator listing/active/logout, local references, work-scope KB file-index APIs, memory pending-review, presets, orchestration, and agent-host (+ internal tool execution). Future World KB / User knowledge / Moment context surfaces may be local-only, but must be explicitly registered and documented; after KCA-002 B2, daemon context assembly is not an active Local API route.

**Always forbidden on daemon:** `/sync/*`, `/creators/registrations*`, platform world/explore proxies, public `/acp/*` (use agent-host namespace), `nexus-cloud-sync`, `nexus-cloud-domain`, and platform HTTP paths.

Auth model: see [V1.20 delivery compass](../../iterations/v1.20-delivery-compass-v1.md) (`X-API-Key`, keyless-localhost).

### 6.1 V1.24 product-integration gap cross-links

These are runtime/product gaps after Cargo alignment, not missing dependency edges:

| Gap | Boundary impact | V1.24 audit cross-link |
| --- | --- | --- |
| Daemon context assembly route retired | `POST /v1/local/context/assemble` is not registered and is retired by KCA-002 B2; context assembly stays CLI in-process. | [KCA-002](../../iterations/v1.24-knowledge-crates-alignment-audit-compass-v1.md#42-missing-local-api-context-assemble-kca-002) |
| Work KB path remains work-scoped | `/v1/local/kb/*` and `creator kb` are `scope=work` local file-index APIs only, not World KB (`nexus-kb`) APIs. | [KCA-003](../../iterations/v1.24-knowledge-crates-alignment-audit-compass-v1.md#41-dual-kb-semantics-without-route-qualification-kca-003) |
| Domain crates are only partially product-wired | `nexus-narrative`, `nexus-kb`, `nexus-knowledge`, and moment assembly are linked in Cargo but not fully surfaced through daemon HTTP/product workflows. | [KCA-004/KCA-005](../../iterations/v1.24-knowledge-crates-alignment-audit-compass-v1.md#43-compile-time-only-domain-linkage-kca-005) |

---

## 7. CLI integration (principles)

| Concern | Owner |
| --- | --- |
| Daemon control | Local API |
| Creator register/verify | `nexus-cloud-sync` (+ persist via Creator local state and `nexus-cloud-domain` target invariants) |
| Bundle sync | `nexus-cloud-sync` (`legacy-sync` until redesigned) |
| `local_only` context | `nexus-moment-context-assembly` Stage-0 |
| World-scoped narrative KB | `nexus-kb` + `nexus-narrative` |
| User-scoped global knowledge | `nexus-knowledge` |

---

## 8. Orchestration

Built-in `sync.*` / `outbox.flush` capabilities on **daemon builds** MUST NOT call cloud-sync; stubs or explicit “cloud line disabled” results are acceptable until cloud orchestration is redesigned.

Workspace file writes remain agent-mediated (agent-host internal tool execution); unchanged principle from preset-driven architecture.

---

## 9. Cloud runtime policy

`runtime_mode`, `degradation`, and platform health probing belong to the **cloud line** (CLI / `cloud-stage` builds), not the daemon hot path.

---

*Long-term SSOT. Implementation tracking: [v1.21-local-platform-isolation-delivery-compass-v1.md](../../iterations/v1.21-local-platform-isolation-delivery-compass-v1.md) and V1.23 plan `.agents/plans/2026-05-21-v1.23-architecture-crate-wiring-alignment.md`.*
