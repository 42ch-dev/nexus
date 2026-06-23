# `schemas/` Directory Layout (External-Consumer Contracts)

## 0. Document position

| Attribute | Value |
| --- | --- |
| **Status** | Normative — V1.62 Shipped (consumer-scope reorganization) |
| **Document class** | Master |
| **Scope** | Folder names, consumer-scope mapping, README rules, rename policy; **not** field-level DTO definitions (those stay in platform `v1-spec` + `data-model-v1`) |
| **Last updated** | 2026-06-23 — V1.62 P2 (spec-seal polish) |
| **Related** | [schemas-external-consumer-boundary.md](../schemas-external-consumer-boundary.md), [local-cloud-crate-architecture.md](./local-cloud-crate-architecture.md), [compute-module-abi.md](./compute-module-abi.md) §4–§5, [wasm-host.md](./wasm-host.md) §6–§7, [schemas/AGENTS.md](../../../schemas/AGENTS.md), [tooling/AGENTS.md](../../../tooling/AGENTS.md) |

**Do not confuse:**

- **`schemas/domain/`** — wire **entity** shapes (Creator, World, …) used on platform-observed boundaries.
- **`nexus-domain` / `nexus-cloud-domain` crates** — Rust **logic** crates; the monolith `nexus-domain` name is **retired** (platform slice → `nexus-cloud-domain`).

---

## 1. Normative tree (2026-06, post-V1.62 P0)

All paths are under repository root `schemas/`. Only **external-consumer** files belong here (see boundary doc): platform wire OR Local API cross-language contracts.

```text
schemas/
├── AGENTS.md              # codegen + drift rules (mandatory read)
├── README.md              # index (this layout + file counts)
├── common/                # shared identifiers, enums, value objects ($ref'd by wire + local-api)
├── domain/                # wire domain entities (Creator, World, KeyBlock, …)
├── platform/              # platform consumer-only
│   ├── http-bff/          # platform HTTP request/response bodies (BFF contracts)
│   └── sync/              # CLI ↔ platform sync protocol (bundle, delta, pull, conflict)
└── local-api/             # external Local API clients (e.g. future WebApp/Web-UI, WASM modules)
    └── compute/           # compute module ABI envelopes (ComputeInput / ComputeOutput)
```

**Removed paths (do not recreate):**

- `schemas/acp-runtime/` — → `crates/nexus-contracts/src/local/acp_runtime/`
- `schemas/meta/` — → `crates/nexus-contracts/src/local/meta.rs`
- `schemas/cli-sync/` — renamed `cloud-sync/` (2026-05-20); `cloud-sync/` folded into `platform/sync/` (2026-06-23, V1.62 P0)
- `schemas/cloud-sync/` — → `platform/sync/` (2026-06-23, V1.62 P0)
- `schemas/compute/` — compute envelopes → `local-api/compute/`; entity-attributes/entity-state **deleted** (per-module shapes → `modules/<id>/manifest.json`, V1.62 P1) (2026-06-23, V1.62 P0)
- Any daemon `/v1/local/*` DTO as JSON Schema — use `src/local/` (orchestration, schedule, daemon status, registry manifest)

---

## 2. Folder ↔ product line ↔ consumers

| Directory | Product line | External consumer | Primary Rust consumer | npm `@42ch/nexus-contracts` |
| --- | --- | --- | --- | --- |
| **`platform/http-bff/`** | Cloud enhancement (platform HTTP) | `nexus-platform` | `nexus-cloud-sync` (HTTP client), platform TS | **Yes** |
| **`platform/sync/`** | Cloud enhancement (bundle / pull / conflict) | `nexus-platform` | `nexus-cloud-sync` (`legacy-sync`) | **Yes** |
| **`domain/`** | Wire entities embedded in bundles & platform bodies | `nexus-platform` (transitive via `$ref`) | All cloud-line crates + generated imports | **Yes** |
| **`common/`** | Shared wire value objects | `nexus-platform` (when `$ref`'d) | Generated | **Yes** |
| **`local-api/compute/`** | Local API — compute module ABI | External WASM modules + future WebApp | `nexus-wasm-host` (re-exports), compute modules | **Yes** |

**Local product line** (daemon, orchestration, agent-host internal DTOs) MUST NOT add new subtrees under `schemas/` unless an **external** client (separate process / language boundary) consumes them. Add types under `crates/nexus-contracts/src/local/{acp_runtime,domain,orchestration,schedule}/`. The `local-api/` tree is reserved for cross-language Local API contracts (one subfolder per concern; V1.62 seeded `compute/`).

---

## 3. Subdirectory contracts

### 3.1 `platform/http-bff/`

- One schema file per **platform HTTP** request or response shape (or shared response fragment), kebab-case basename.
- **Not** daemon Local API proxies (V1.20 removed world/explore **daemon** routes; platform HTTP contracts **remain** wire here).
- Grouping is **flat** (no `http-bff/explore/` subfolders) — use filename prefix: `explore-*`, `world-*`, `publish-*`, `notifications-*`, `context-assembly-v1`, etc.
- `$id` / `$ref` URIs use `https://nexus42.invalid/schemas/platform/http-bff/...`.
- Maintain [`platform/http-bff/README.md`](../../../schemas/platform/http-bff/README.md) index when adding files.

### 3.2 `platform/sync/`

- CLI ↔ platform sync protocol: bundle envelope (codegen canonical), delta, sync-command, pull request/response, conflict response.
- **`bundle.schema.json`** is the **codegen canonical** `Bundle` type. **`bundle-refinement.schema.json`** is a **validation refinement** (allOf of the canonical bundle with CLI-specific constraints) — codegen skips it (see `tooling/codegen/src/schema-loader.ts` `SKIP_STRUCT_GENERATION_REL_PATHS`).
- `delta.schema.json` and `sync-command.schema.json` moved here from `domain/` (V1.62 P0) because they are sync-protocol payloads, not wire entities.
- `$id` / `$ref` URIs use `https://nexus42.invalid/schemas/platform/sync/...`.
- Maintain [`platform/sync/README.md`](../../../schemas/platform/sync/README.md).

### 3.3 `domain/`

Wire entities aligned with platform `data-model-v1` §5–§10. Current inventory (verify on disk):

| File | Role | Typical `nexus-cloud-domain` / app crate |
| --- | --- | --- |
| `creator.schema.json` | Creator wire shape | `nexus-creator` (logic), not duplicated in app |
| `user.schema.json`, `pairing.schema.json` | Account bridge | `nexus-cloud-domain` (logic) |
| `world.schema.json`, `world-membership.schema.json`, `fork-branch.schema.json` | Narrative graph | `nexus-narrative` / bundles |
| `key-block.schema.json`, `timeline-event.schema.json` | Narrative KB on wire | `nexus-kb` + sync bundles |
| `memory.schema.json` | Memory on wire | `nexus-creator-memory` |
| `story-manifest.schema.json` | Story summary on wire | `nexus-narrative`, novel-writing sync |

(bundle/delta/sync-command moved to `platform/sync/` in V1.62 P0 — they are sync payloads, not wire entities.)

[`domain/README.md`](../../../schemas/domain/README.md) MUST list only files that exist under `schemas/domain/*.json`.

### 3.4 `common/`

- `common.schema.json` — identifiers and enums (data-model §7). Definitions-only; codegen emits `generated/common/common_types.rs` + `CommonTypes.ts`.
- `source-anchor.schema.json`, `version-ref.schema.json` — value objects §6. `SourceAnchor` is emitted into `common_types`.
- Do not add local-only enums here; if no external client observes an enum, put it in `src/local/`.

**Meta schema (local, not a `schemas/` folder):** `crates/nexus-contracts/src/local/meta.rs`. Removed `schemas/meta/` (V1.4 WS5 + V1.21 cleanup).

### 3.5 `local-api/compute/`

- Compute module ABI envelopes consumed by **external** WASM compute modules (and, in future, the WebApp/Web-UI): `compute-input.schema.json`, `compute-output.schema.json`.
- These are cross-language contracts (Rust host ↔ wasm32 module), so they live under `schemas/` and run through codegen, not as hand-written local types.
- Per-module shape declarations (per-BlockType attributes/state) do **not** live here — they live in each module's `manifest.json` `schemas` block (V1.62 P1). See [modules/README.md](../../../modules/README.md).
- `$id` / `$ref` URIs use `https://nexus42.invalid/schemas/local-api/compute/...`.
- Maintain [`local-api/compute/README.md`](../../../schemas/local-api/compute/README.md). Compute ABI normative detail: [compute-module-abi.md](./compute-module-abi.md). Host-side runtime detail: [wasm-host.md](./wasm-host.md).

---

## 4. Content hygiene

| Check | Action |
| --- | --- |
| README vs disk | Every `schemas/**/README.md` matches `*.json` in that folder |
| Stale `acp-runtime` / `cloud-sync` / `compute` references | Remove from active plans/docs; types moved/deleted (see §1 + §5) |
| `OutboxEntry` | **Local only** — must not reappear in `schemas/domain/` |
| `key-block` on wire | Stays in `schemas/domain/` if platform/sync bundles carry KeyBlocks; narrative **logic** is `nexus-kb` |
| Per-module entity shapes | **Not** in `schemas/` — declare in `modules/<id>/manifest.json` (V1.62 P1) |
| Platform grep before delete | `rg <TypeName>` on `nexus-platform` before removing any schema file |

**Re-audit:** add a dated appendix under §5 when moving or renaming folders.

---

## 5. Historical renames

| Old path | Current | Done |
| --- | --- | --- |
| `schemas/cli-sync/` | `schemas/cloud-sync/` | 2026-05-20 — `$id` URIs updated |
| `schemas/acp-runtime/` | `src/local/acp_runtime/` | V1.4 WS5 |
| `schemas/meta/` | `src/local/meta.rs` | V1.4 WS5; directory removed V1.21 |
| `schemas/cloud-sync/` | `schemas/platform/sync/` | 2026-06-23 (V1.62 P0) — folded into consumer-scope `platform/sync/`; `bundle.schema.json` renamed `bundle-refinement.schema.json` |
| `schemas/compute/` | `schemas/local-api/compute/` (+ entity-* deleted) | 2026-06-23 (V1.62 P0) — compute envelopes → `local-api/compute/`; per-module entity-attributes/entity-state **deleted** (→ `manifest.json`, P1) |
| `schemas/domain/{bundle,delta,sync-command}` | `schemas/platform/sync/` | 2026-06-23 (V1.62 P0) — sync payloads, not wire entities |
| `schemas/platform/*.schema.json` (flat) | `schemas/platform/http-bff/*.schema.json` | 2026-06-23 (V1.62 P0) — consumer-scope split into `http-bff/` + `sync/` |

**Do not rename** `platform/` → `cloud-platform/` (platform HTTP naming is stable in v1-spec).

---

## 6. Related platform paths

Platform prose may still say `v1-spec/cli-sync/` for sync **protocol** documents. That is the **platform repo folder name**, independent of OSS `schemas/platform/sync/`. Coordinate `@42ch/nexus-contracts` semver when platform consumes regenerated types after URI path changes.

---

## 7. Wire file inventory (2026-06, post-V1.62 P0)

Authoritative count: run `pnpm run validate-schemas` (currently **56** `*.schema.json`).

| Directory | Files | Notes |
| --- | --- | --- |
| `common/` | 3 | `common`, `source-anchor`, `version-ref` |
| `domain/` | 10 | Wire entities (see §3.3 table) |
| `platform/http-bff/` | 34 | Platform HTTP bodies (flat; prefix grouping in [http-bff/README.md](../../../schemas/platform/http-bff/README.md)) |
| `platform/sync/` | 7 | `bundle`, `bundle-refinement` (codegen-skipped), `delta`, `sync-command`, `sync-pull-request`, `sync-pull-response`, `conflict-response` |
| `local-api/compute/` | 2 | `compute-input`, `compute-output` |

**Not in tree:** `acp-runtime/`, `meta/`, `cli-sync/`, `cloud-sync/`, `compute/` (all removed/renamed).

Historical audit (pre-rename paths): [archived schemas-boundary §5.2](../archived/knowledge/schemas-boundary.md) — use this section for current paths.

---

*Normative Master. V1.62 P0 consumer-scope reorganization (2026-06-23); V1.62 P2 spec-seal polish. Boundary rule: [schemas-external-consumer-boundary.md](../schemas-external-consumer-boundary.md).*
