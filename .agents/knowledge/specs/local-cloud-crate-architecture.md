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

**Current-vs-target rule:** sections below explicitly separate **currently wired** Cargo reality from **V1.23 target wired** architecture. A target dependency is not a claim that the edge exists today.

---

## 1. Two product lines (frozen)

| Line | Purpose | Integration surface |
| --- | --- | --- |
| **Local product** | Orchestration, agent-host, workspace, Creator + Creator memory, World-scoped narrative KB, User-scoped knowledge, narrative graph, Moment context assembly | `nexus42 daemon` → `/v1/local/*` |
| **Cloud enhancement** | Platform HTTP, bundle sync, registration, optional context Stage-1, User/Pairing persistence | `nexus-cloud-sync` + CLI cloud subcommands — **never** daemon Local API |

**Hard isolation:** `nexus-daemon-runtime` MUST NOT depend on `nexus-cloud-sync` and MUST NOT register HTTP handlers that perform platform HTTP or proxy sync.

**Verified current Cargo reality (2026-05-21):** `crates/nexus-daemon-runtime/Cargo.toml` has no `nexus-cloud-sync` or `nexus-cloud-domain` dependency, and `cargo tree -p nexus-daemon-runtime --depth 1` shows no cloud crate edge. This boundary must remain true after V1.23 wiring.

**Agent identity:** Operational actor for agents and orchestration is **`Creator`** (`creator_id`). `User` / `Pairing` are platform-bridge concepts only.

---

## 2. Contracts boundary (frozen)

All **wire shapes** and **platform-aligned DTOs** come from the **`nexus-contracts`** crate (§3.1): generated from `schemas/` (layout: [schemas-directory-layout.md](./schemas-directory-layout.md)) or hand-written under `src/local/` per [schemas-wire-platform-sync-boundary.md](../schemas-wire-platform-sync-boundary.md).

| Rule | Detail |
| --- | --- |
| **No second DTO set** | Crates MUST NOT redefine structs that duplicate generated wire types (e.g. a hand-rolled `Creator` with the same fields as `nexus_contracts::Creator`). |
| **`nexus-creator`** | Imports wire/local contract types for Creator and related IDs; adds **domain logic** (validation, conversions, local cache records, path helpers). Any field that must match platform JSON MUST use contract types verbatim (`snake_case` on wire). |
| **`nexus-cloud-domain`** | Imports contract types for `User`, `Pairing`, and platform-bridge enums; adds **domain logic** only (invariants, mapping from platform responses). |
| **`nexus-cloud-sync`** | Uses contracts for request/response bodies on HTTP; no parallel HTTP DTOs in application crates. Target wiring MUST route User/Pairing invariants through `nexus-cloud-domain`. |
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
| **`nexus-cloud-domain`** | `User`, `Pairing` | Platform-bridge domain logic for User/Pairing invariants and mappings from contract types. No HTTP transport. | No HTTP; target dependency of **`nexus-cloud-sync`** |
| **`nexus-moment-context-assembly`** | `Moment` | Per-moment, pre-session context aggregation. Target Stage-0 aggregates Creator memory, narrative state, World KB assets, and User knowledge. Optional `cloud-stage` may merge platform context. | Only with `cloud-stage` |
| **`nexus-cloud-sync`** | Cloud transport for User/Pairing and sync bundles | Platform HTTP and sync transport. It MUST use `nexus-cloud-domain` for User/Pairing invariants in the V1.23 target. | N/A |
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

- **Currently wired Stage-0:** depends on `nexus-creator-memory` + `nexus-contracts` only.
- **Target wired Stage-0:** aggregates `nexus-creator-memory`, `nexus-narrative`, `nexus-kb`, and `nexus-knowledge`.
- **Stage-1 (optional):** `cloud-stage` feature calls cloud-sync for platform context; not used on daemon default build.

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

## 4. Currently wired Cargo graph (verified 2026-05-21)

This section describes the current `Cargo.toml` and `cargo tree` reality. It is intentionally separate from the V1.23 target in §5.

### 4.1 Current direct dependencies for alignment-sensitive crates

| Crate | Currently wired direct workspace dependencies | Current product reachability / notes |
| --- | --- | --- |
| `nexus42` | `nexus-acp-host`, `nexus-cloud-sync` with `legacy-sync`, `nexus-contracts`, `nexus-creator`, `nexus-creator-memory`, `nexus-daemon-runtime`, `nexus-home-layout`, `nexus-local-db`, `nexus-moment-context-assembly` with `cloud-stage`, `nexus-orchestration` | CLI currently reaches cloud-sync and moment assembly. Because the CLI enables `cloud-stage`, `cargo tree -p nexus42` shows `nexus-moment-context-assembly -> nexus-cloud-sync`; this is CLI/cloud-line reachability, not daemon reachability. |
| `nexus-daemon-runtime` | `nexus-agent-host`, `nexus-contracts`, `nexus-creator`, `nexus-home-layout`, `nexus-local-db`, `nexus-orchestration` | Currently wired to local runtime essentials only. Currently NOT wired to `nexus-creator-memory`, `nexus-moment-context-assembly`, `nexus-narrative`, `nexus-kb`, `nexus-knowledge`, `nexus-cloud-sync`, or `nexus-cloud-domain`. |
| `nexus-moment-context-assembly` | `nexus-contracts`, `nexus-creator-memory`; optional `nexus-cloud-sync` behind `cloud-stage` | Currently NOT wired to `nexus-narrative`, `nexus-kb`, or `nexus-knowledge`. |
| `nexus-narrative` | `nexus-contracts`, `nexus-kb` | Currently compile-wired to `nexus-kb`; currently has no product caller from `nexus42` or daemon. |
| `nexus-kb` | `nexus-contracts` | Currently reachable only through `nexus-narrative` compile dependency; currently not product-reachable from `nexus42` or daemon. |
| `nexus-knowledge` | `nexus-contracts` | Currently standalone library; currently has no product caller. |
| `nexus-cloud-domain` | `nexus-contracts` | Currently standalone library; currently NOT wired into `nexus-cloud-sync`. |
| `nexus-cloud-sync` | `nexus-contracts`, `nexus-home-layout`, `nexus-local-db` | Currently owns cloud HTTP/sync transport but does not depend on `nexus-cloud-domain`; this is a V1.23 alignment gap. |

### 4.2 Current wiring diagram

```text
schemas/ ──codegen──► nexus-contracts

nexus42 ──┬── nexus-daemon-runtime ──┬── nexus-agent-host
          │                          ├── nexus-creator
          │                          ├── nexus-local-db
          │                          ├── nexus-orchestration
          │                          ├── nexus-contracts
          │                          └── nexus-home-layout
          ├── nexus-cloud-sync (legacy-sync enabled by CLI)
          ├── nexus-moment-context-assembly (cloud-stage enabled by CLI)
          │   ├── nexus-creator-memory
          │   └── [cloud-stage] nexus-cloud-sync
          ├── nexus-creator-memory ── nexus-creator
          ├── nexus-creator
          ├── nexus-local-db
          └── nexus-orchestration

nexus-narrative ──► nexus-kb ──► nexus-contracts
nexus-knowledge ──► nexus-contracts
nexus-cloud-domain ──► nexus-contracts
nexus-cloud-sync ──► nexus-contracts, nexus-home-layout, nexus-local-db
```

**Current daemon/cloud boundary:** `nexus-daemon-runtime` currently has no `nexus-cloud-sync` or `nexus-cloud-domain` edge. This matches the forbidden-edge policy.

---

## 5. V1.23 target wiring

The following target graph is normative for V1.23 planning. It does **not** describe current wiring unless an edge is also listed in §4.

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

nexus-moment-context-assembly (default Stage-0 target)
  ├── nexus-creator-memory
  ├── nexus-narrative
  ├── nexus-kb
  ├── nexus-knowledge
  └── nexus-contracts
```

**Daemon target constraint:** if `nexus-moment-context-assembly` keeps optional `cloud-stage`, daemon wiring MUST use default features only. The daemon target MUST still have no `nexus-cloud-sync`, no `nexus-cloud-domain`, and no platform HTTP path.

### 5.2 Explicit V1.23 alignment gaps

| Crate pair / path | Current reality | V1.23 target |
| --- | --- | --- |
| `nexus-cloud-sync -> nexus-cloud-domain` | **Currently NOT wired.** `nexus-cloud-sync` depends on `nexus-contracts`, `nexus-home-layout`, and `nexus-local-db`; it does not depend on `nexus-cloud-domain`. | **Target wired.** Cloud transport uses `nexus-cloud-domain` for User/Pairing invariants. |
| `nexus-moment-context-assembly -> nexus-narrative` | **Currently NOT wired.** Stage-0 only depends on `nexus-creator-memory` + contracts. | **Target wired.** Moment assembly reads narrative World/Timeline/Event context through `nexus-narrative`. |
| `nexus-moment-context-assembly -> nexus-kb` | **Currently NOT wired.** Stage-0 does not read World KB assets. | **Target wired.** Moment assembly can include World-scoped narrative KB slices. |
| `nexus-moment-context-assembly -> nexus-knowledge` | **Currently NOT wired.** Stage-0 does not read User knowledge. | **Target wired.** Moment assembly can include selected User-scoped knowledge slices. |
| `nexus-daemon-runtime -> nexus-moment-context-assembly` | **Currently NOT wired.** Daemon direct deps do not include moment assembly. | **Target wired with default features only.** Daemon local APIs may call Stage-0 assembly without cloud-stage. |
| `nexus-daemon-runtime -> nexus-narrative` | **Currently NOT wired.** Daemon direct deps do not include narrative. | **Target wired.** Daemon local product graph reaches narrative state where product paths require it. |
| `nexus-daemon-runtime -> nexus-kb` | **Currently NOT wired.** Daemon direct deps do not include `nexus-kb`; `nexus-kb` is only compile-reachable through `nexus-narrative`. | **Target wired.** Daemon local product graph reaches World-scoped KB paths where product paths require them. |
| `nexus-daemon-runtime -> nexus-knowledge` | **Currently NOT wired.** Daemon direct deps do not include `nexus-knowledge`. | **Target wired.** Daemon local product graph reaches User-scoped knowledge where product paths require it. |
| `nexus42` product reachability for `nexus-narrative` / `nexus-kb` / `nexus-knowledge` | **Currently NOT wired as direct product domains.** CLI reaches neither `nexus-narrative` nor `nexus-knowledge`; `nexus-kb` is not direct and only tied to the compile-only narrative island. | **Target wired.** CLI/daemon product paths route to the owning domain crates instead of duplicating storage or semantics. |
| CLI `creator kb` -> World-scoped `nexus-kb` semantics | **Currently NOT equivalent.** `creator kb` is a local work-scope file/index workflow under active Creator/workspace. | **Target clarified/routed.** Future World KB behavior routes to `nexus-kb` + `nexus-narrative`; future User knowledge behavior routes to `nexus-knowledge`. |

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

**Always allowed (local product):** runtime health/status, workspace, local creator listing/active/logout, User knowledge references (local index), World-scoped narrative KB entries, memory pending-review, presets, orchestration, agent-host (+ internal tool execution), and default-feature Moment context assembly when wired.

**Always forbidden on daemon:** `/sync/*`, `/creators/registrations*`, platform world/explore proxies, public `/acp/*` (use agent-host namespace), `nexus-cloud-sync`, `nexus-cloud-domain`, and platform HTTP paths.

Auth model: see [V1.20 delivery compass](../../iterations/v1.20-delivery-compass-v1.md) (`X-API-Key`, keyless-localhost).

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
