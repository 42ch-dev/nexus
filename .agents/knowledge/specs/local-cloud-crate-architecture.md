# Local–Cloud Crate Architecture (OSS)

## 0. Document position

| Attribute | Value |
| --- | --- |
| **Status** | Active — long-term normative SSOT |
| **Scope** | Stable rules: local vs cloud product lines, crate responsibilities, contracts usage, dependency forbidden edges, Local API *classes* allowed/forbidden |
| **Delivery compass** | Iteration-scoped milestones, phases, acceptance tests → [v1.21-local-platform-isolation-delivery-compass-v1.md](../../iterations/v1.21-local-platform-isolation-delivery-compass-v1.md) |
| **Related** | [local-runtime-boundary.md](./local-runtime-boundary.md), [daemon-runtime.md](./daemon-runtime.md), [cli-spec.md](./cli-spec.md), [schemas-directory-layout.md](./schemas-directory-layout.md), [../schemas-wire-platform-sync-boundary.md](../schemas-wire-platform-sync-boundary.md) |

**This file is not an implementation checklist.** Do not add migration batches, branch names, or “done by V1.21” task tables here — put those in the matching **iteration compass** and `.agents/plans/`.

---

## 1. Two product lines (frozen)

| Line | Purpose | Integration surface |
| --- | --- | --- |
| **Local product** | Orchestration, agent-host, workspace, creator + memory, narrative KB, global knowledge, narrative graph | `nexus42 daemon` → `/v1/local/*` |
| **Cloud enhancement** | Platform HTTP, bundle sync, registration, optional context Stage-1, User/Pairing persistence | `nexus-cloud-sync` + CLI cloud subcommands — **never** daemon Local API |

**Hard isolation:** `nexus-daemon-runtime` MUST NOT depend on `nexus-cloud-sync` and MUST NOT register HTTP handlers that perform platform HTTP or proxy sync.

**Agent identity:** Operational actor for agents and orchestration is **`Creator`** (`creator_id`). `User` / `Pairing` are platform-bridge concepts only.

---

## 2. Contracts boundary (frozen)

All **wire shapes** and **platform-aligned DTOs** come from the **`nexus-contracts`** crate (§3.1): generated from `schemas/` (layout: [schemas-directory-layout.md](./schemas-directory-layout.md)) or hand-written under `src/local/` per [schemas-wire-platform-sync-boundary.md](../schemas-wire-platform-sync-boundary.md).

| Rule | Detail |
| --- | --- |
| **No second DTO set** | Crates MUST NOT redefine structs that duplicate generated wire types (e.g. a hand-rolled `Creator` with the same fields as `nexus_contracts::Creator`). |
| **`nexus-creator`** | Imports wire/local contract types for Creator and related IDs; adds **domain logic** (validation, conversions, local cache records, path helpers). Any field that must match platform JSON MUST use contract types verbatim (`snake_case` on wire). |
| **`nexus-cloud-domain`** | Imports contract types for `User`, `Pairing`, and platform-bridge enums; adds **domain logic** only (invariants, mapping from platform responses). |
| **`nexus-cloud-sync`** | Uses contracts for request/response bodies on HTTP; no parallel HTTP DTOs in application crates. |
| **Local-only types** | Schedule, daemon status, orchestration HTTP, assembly DTOs → `nexus-contracts/src/local/` (not `schemas/`) unless platform later observes them. |

**Corollary:** If a type appears in a platform API or sync bundle, its definition lives in **`nexus-contracts`**, not in `nexus-creator` or `nexus-cloud-domain`.

---

## 3. Crate responsibilities (long-term)

### 3.1 Foundation (types & paths)

These crates are **not** split by the local/cloud program; they sit **under** all application crates. See §2 for how application code must use them.

| Crate | Responsibility | `nexus-cloud-sync` dep? |
| --- | --- | --- |
| **`nexus-contracts`** | **Wire and local type SSOT** for the OSS monorepo: `src/generated/` from `schemas/` (`pnpm run codegen`); hand-written `src/local/` for CLI↔daemon and orchestration types not observed by platform; `enum_conversions.rs` for shared enums. **No business logic**, no HTTP, no I/O. Published to npm as `@42ch/nexus-contracts` for `nexus-platform`; Rust crate is workspace-internal. | N/A (leaf type library) |
| **`nexus-home-layout`** | Frozen `~/.nexus42/` path resolution and safe path helpers (creators, workspaces, run dir). No domain rules. | N/A |

**Rules for `nexus-contracts`:** application crates (`nexus-creator`, `nexus-cloud-domain`, `nexus-cloud-sync`, …) **depend on** `nexus-contracts`; they MUST NOT duplicate types defined in `generated/` or `local/`. Schema changes start in `schemas/` → codegen → then domain logic updates.

### 3.2 Application inventory (local + cloud)

| Crate | Responsibility | `nexus-cloud-sync` dep? |
| --- | --- | --- |
| **`nexus-creator`** | Creator aggregate **logic** + local state; **types** from `nexus-contracts`; credential/cache hooks (no HTTP) | No |
| **`nexus-creator-memory`** | Creator-scoped SOUL, LTM, review, personality IO | No — depends on **`nexus-creator`** |
| **`nexus-kb`** | Narrative KeyBlock + SourceAnchor (tied to world/timeline) | No |
| **`nexus-knowledge`** | Global references + future general KB (not narrative KeyBlocks) | No |
| **`nexus-narrative`** | World, timeline, fork, story, manuscript, narrative consistency | No — may use **`nexus-kb`** |
| **`nexus-cloud-domain`** | Platform-bridge **logic** for `User`, `Pairing` (**types** from `nexus-contracts`) | No HTTP |
| **`nexus-moment-context-assembly`** | Per-moment context assembly; `cloud-stage` feature → cloud-sync | Only with `cloud-stage` |
| **`nexus-cloud-sync`** | Platform HTTP + optional `legacy-sync` pipeline | N/A |
| **`nexus-daemon-runtime`** | Local API, lifecycle, DB handles, orchestration/agent-host host | **Forbidden** |
| **`nexus-orchestration`** | Presets, schedules, workers | No cloud-sync (sync capabilities stubbed locally) |
| **`nexus42`** | CLI; cloud commands use cloud-sync | CLI may use cloud-sync |

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
| **Typical callers** | daemon, orchestration, creator-memory, knowledge | cloud-sync, CLI after registration |
| **HTTP** | Never | Never (cloud-sync owns HTTP) |

### 3.5 `nexus-kb` vs `nexus-knowledge`

- **`nexus-kb`:** facts **inside** a narrative graph (KeyBlocks, anchors on timeline/world).
- **`nexus-knowledge`:** **global** reference corpora and future KB entries; indexed per creator/workspace but not narrative KeyBlocks.

### 3.6 `nexus-moment-context-assembly`

- **Stage-0:** local-only assembly from creator-memory, kb, knowledge, narrative.
- **Stage-1 (optional):** `cloud-stage` feature calls cloud-sync for platform `context/assemble`; not used on daemon default build.

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

## 4. Dependency DAG (normative)

```text
schemas/ ──codegen──► nexus-contracts (generated + local + enum_conversions)
                              ▲
                              │ (all application crates import types; no reverse dep)
nexus42 ──┬── nexus-daemon-runtime ──┬── orchestration, agent-host
          │                          ├── creator, creator-memory, kb, knowledge, narrative
          │                          ├── moment-context-assembly (default features)
          │                          └── ✗ cloud-sync, ✗ cloud-domain
          └── nexus-cloud-sync ── cloud-domain ── contracts

creator-memory ── creator ── contracts, home-layout
knowledge      ── creator ── contracts
narrative      ── kb ── contracts
kb             ── contracts, narrative (types)
cloud-domain   ── contracts
cloud-sync     ── contracts, cloud-domain, local-db
moment-context-assembly ── creator-memory, kb, knowledge, narrative, creator, contracts
                        └── [cloud-stage] ── cloud-sync
```

**Forbidden:** `nexus-daemon-runtime` → `nexus-cloud-sync` or `nexus-cloud-domain`.

---

## 5. Daemon Local API (principles)

Authoritative route list for a given release lives in **`crates/nexus-daemon-runtime/src/api/mod.rs`** and the active **iteration compass**.

**Always allowed (local product):** runtime health/status, workspace, local creator listing/active/logout, knowledge references (local index), narrative KB entries, memory pending-review, presets, orchestration, agent-host (+ internal tool execution).

**Always forbidden on daemon:** `/sync/*`, `/creators/registrations*`, platform world/explore proxies, public `/acp/*` (use agent-host namespace).

Auth model: see [V1.20 delivery compass](../../iterations/v1.20-delivery-compass-v1.md) (`X-API-Key`, keyless-localhost).

---

## 6. CLI integration (principles)

| Concern | Owner |
| --- | --- |
| Daemon control | Local API |
| Creator register/verify | `nexus-cloud-sync` (+ persist via creator + cloud-domain) |
| Bundle sync | `nexus-cloud-sync` (`legacy-sync` until redesigned) |
| `local_only` context | `nexus-moment-context-assembly` Stage-0 |

---

## 7. Orchestration

Built-in `sync.*` / `outbox.flush` capabilities on **daemon builds** MUST NOT call cloud-sync; stubs or explicit “cloud line disabled” results are acceptable until cloud orchestration is redesigned.

Workspace file writes remain agent-mediated (agent-host internal tool execution); unchanged principle from preset-driven architecture.

---

## 8. Cloud runtime policy

`runtime_mode`, `degradation`, and platform health probing belong to the **cloud line** (CLI / `cloud-stage` builds), not the daemon hot path.

---

*Long-term SSOT. Implementation tracking: [v1.21-local-platform-isolation-delivery-compass-v1.md](../../iterations/v1.21-local-platform-isolation-delivery-compass-v1.md).*
