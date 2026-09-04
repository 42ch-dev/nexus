# `schemas/` Directory Layout (External-Consumer Contracts)

## 0. Document position

| Attribute | Value |
| --- | --- |
| **Status** | Normative — current Daemon API contracts live under `schemas/daemon-api/`. **V1.139 architect §5.2**: §3.3 domain/ table — `key-block.schema.json` **deleted** (V1.139 SPOKE adoption; KB entry type now sourced from spoke `knowledge-entry.schema.json`). |
| **Document class** | Master |
| **Scope** | Folder names, consumer-scope mapping, README rules, rename policy; **not** field-level DTO definitions (those stay in platform `v1-spec` + `data-model-v1`) |
| **Last updated** | 2026-09-04 — reconciled current Daemon API schema and generated-module names through V1.183. |
| **Related** | [schemas-external-consumer-boundary.md](schemas-external-consumer-boundary.md), [local-cloud-crate-architecture.md](./local-cloud-crate-architecture.md), [compute-module-abi.md](./compute-module-abi.md) §4–§5, [wasm-host.md](./wasm-host.md) §6–§7, [spoke-adapter-architecture.md](./spoke-adapter-architecture.md), [schemas/AGENTS.md](../../schemas/AGENTS.md), [tooling/AGENTS.md](../../tooling/AGENTS.md) |

**Do not confuse:**

- **`schemas/domain/`** — wire **entity** shapes (Creator, World, …) used on platform-observed boundaries.
- **`nexus-domain` / `nexus-cloud-domain` crates** — Rust **logic** crates; the monolith `nexus-domain` name is **retired** (platform slice → `nexus-cloud-domain`).

---

## 1. Normative tree (2026-09, current through V1.183)

All paths are under repository root `schemas/`. Only **external-consumer** files belong here (see boundary doc): platform wire OR Daemon API cross-language contracts.

```text
schemas/
├── AGENTS.md              # codegen + drift rules (mandatory read)
├── README.md              # index (this layout + file counts)
├── common/                # shared identifiers, enums, value objects ($ref'd by wire + daemon-api)
├── domain/                # wire domain entities (Creator, World, TimelineEvent, …)
├── platform/              # platform consumer-only
│   ├── http-bff/          # platform HTTP request/response bodies (BFF contracts)
│   └── sync/              # CLI ↔ platform sync protocol (bundle, delta, pull, conflict)
└── daemon-api/            # external Daemon API clients (Web UI, desktop shell, WASM modules)
    ├── agent-host/         # provider scan and agent-host contracts
    ├── canvas/             # canvas/editor contracts
    ├── check/              # check-run contracts
    ├── common/             # shared Daemon API envelopes
    ├── compute/            # compute module ABI envelopes
    ├── creators/           # creator management contracts
    ├── findings/           # quality finding contracts
    ├── inspector/          # inspector packet contracts
    ├── kb/                 # work-scope KB entry contracts
    ├── memory/             # memory contracts
    ├── orchestration/      # orchestration session and capability contracts
    ├── preset-management/  # preset management contracts
    ├── reading/            # reading-surface contracts
    ├── runtime/            # runtime trust contracts (currently certificate fingerprint)
    ├── schedule/           # schedule and core-context contracts
    ├── timeline/           # timeline contracts
    ├── tools/              # tool bridge contracts
    ├── works/              # works CRUD and chapter-content contracts
    │   └── chapters/
    ├── workspace/          # workspace management contracts
    └── worlds/             # world contracts
```

**Removed paths (do not recreate):**

- `schemas/acp-runtime/` — → `crates/nexus-contracts/src/local/acp_runtime/`
- `schemas/meta/` — → `crates/nexus-contracts/src/local/meta.rs`
- `schemas/cli-sync/` — renamed `cloud-sync/` (2026-05-20); `cloud-sync/` folded into `platform/sync/` (2026-06-23, V1.62 P0)
- `schemas/cloud-sync/` — → `platform/sync/` (2026-06-23, V1.62 P0)
- `schemas/compute/` — compute envelopes now live under `daemon-api/compute/`; entity-attributes/entity-state **deleted** (per-module shapes → `modules/<id>/manifest.json`, V1.62 P1)
- Daemon `/v1/daemon/*` DTOs consumed across the TypeScript boundary live under `schemas/daemon-api/<concern>/`. Internal types (orchestration internals, ACP registry, daemon status) remain in `crates/nexus-contracts/src/local/`.

---

## 2. Folder ↔ product line ↔ consumers

| Directory | Product line | External consumer | Primary Rust consumer | npm `@42ch/nexus-contracts` |
| --- | --- | --- | --- | --- |
| **`platform/http-bff/`** | Cloud enhancement (platform HTTP) | `nexus-platform` | `nexus-cloud-sync` (HTTP client), platform TS | **Yes** |
| **`platform/sync/`** | Cloud enhancement (bundle / pull / conflict) | `nexus-platform` | `nexus-cloud-sync` (`legacy-sync`) | **Yes** |
| **`domain/`** | Wire entities embedded in bundles & platform bodies | `nexus-platform` (transitive via `$ref`) | All cloud-line crates + generated imports | **Yes** |
| **`common/`** | Shared wire value objects | `nexus-platform` (when `$ref`'d) | Generated | **Yes** |
| **`daemon-api/compute/`** | Daemon API — compute module ABI | External WASM modules + Web UI | `nexus-wasm-host` (re-exports), compute modules | **Yes** |
| **`daemon-api/common/`** | Daemon API — shared contract detail | Bundled Web UI + external Daemon API clients | `nexus-daemon-runtime` handlers via generated contracts | **Yes** |
| **`daemon-api/works/`** | Daemon API — works CRUD | Web UI / desktop shell | `nexus-daemon-runtime` | **Yes** |
| **`daemon-api/works/chapters/`** | Daemon API — chapter content and structure | Web UI / desktop shell | `nexus-daemon-runtime` chapter handlers | **Yes** |
| **`daemon-api/kb/`** | Daemon API — KB entries CRUD | Web UI / desktop shell | `nexus-daemon-runtime` | **Yes** |
| **`daemon-api/findings/`** | Daemon API — quality findings CRUD | Web UI / desktop shell | `nexus-daemon-runtime` | **Yes** |
| **`daemon-api/schedule/`** | Daemon API — schedule + core-context CRUD | Web UI / desktop shell | `nexus-daemon-runtime` | **Yes** |
| **`daemon-api/workspace/`** | Daemon API — workspace management CRUD | Web UI / desktop shell | `nexus-daemon-runtime` | **Yes** |
| **`daemon-api/creators/`** | Daemon API — creator management CRUD | Web UI / desktop shell | `nexus-daemon-runtime` | **Yes** |
| **`daemon-api/orchestration/`** | Daemon API — orchestration engine READ | Web UI / desktop shell | `nexus-daemon-runtime` | **Yes** |
| **`daemon-api/preset-management/`** | Daemon API — preset management full surface | Web UI / desktop shell | `nexus-daemon-runtime` | **Yes** |
| **`daemon-api/agent-host/`** | Daemon API — provider discovery and sessions | Web UI / desktop shell | `nexus-daemon-runtime` agent-host handlers | **Yes** |
| **`daemon-api/{canvas,check,inspector,reading}/`** | Daemon API — authoring and diagnostics | Web UI / desktop shell | `nexus-daemon-runtime` | **Yes** |
| **`daemon-api/{memory,timeline,worlds}/`** | Daemon API — narrative resources | Web UI / desktop shell | `nexus-daemon-runtime` | **Yes** |
| **`daemon-api/{runtime,tools}/`** | Daemon API — runtime and tool bridge | Web UI / desktop shell | `nexus-daemon-runtime` | **Yes** |

**Local product line** (daemon, orchestration, agent-host internal DTOs) MUST NOT add new subtrees under `schemas/` unless an **external** client (separate process or language boundary) consumes them. Add internal types under `crates/nexus-contracts/src/local/`. The `daemon-api/` tree is reserved for cross-language Daemon API contracts.

---

## 3. Subdirectory contracts

### 3.1 `platform/http-bff/`

- One schema file per **platform HTTP** request or response shape (or shared response fragment), kebab-case basename.
- **Not** Daemon API proxies (V1.20 removed world/explore **daemon** routes; platform HTTP contracts **remain** wire here).
- Grouping is **flat** (no `http-bff/explore/` subfolders) — use filename prefix: `explore-*`, `world-*`, `publish-*`, `notifications-*`, `context-assembly-v1`, etc.
- `$id` / `$ref` URIs use `https://nexus42.invalid/schemas/platform/http-bff/...`.
- Maintain [`platform/http-bff/README.md`](../../schemas/platform/http-bff/README.md) index when adding files.

### 3.2 `platform/sync/`

- CLI ↔ platform sync protocol: bundle envelope (codegen canonical), delta, sync-command, pull request/response, conflict response.
- **`bundle.schema.json`** is the **codegen canonical** `Bundle` type. **`bundle-refinement.schema.json`** is a **validation refinement** (allOf of the canonical bundle with CLI-specific constraints) — codegen skips it (see `tooling/codegen/src/ts-gen.ts` `SKIP_LIST` / `tooling/codegen/rust-gen/src/main.rs` `SKIP_SCHEMAS`).
- `delta.schema.json` and `sync-command.schema.json` moved here from `domain/` (V1.62 P0) because they are sync-protocol payloads, not wire entities.
- `$id` / `$ref` URIs use `https://nexus42.invalid/schemas/platform/sync/...`.
- Maintain [`platform/sync/README.md`](../../schemas/platform/sync/README.md).

### 3.3 `domain/`

Wire entities aligned with platform `data-model-v1` §5–§10. Current inventory (verify on disk):

| File | Role | Typical `nexus-cloud-domain` / app crate |
| --- | --- | --- |
| `creator.schema.json` | Creator wire shape | `nexus-creator` (logic), not duplicated in app |
| `user.schema.json`, `pairing.schema.json` | Account bridge | `nexus-cloud-domain` (logic) |
| `world.schema.json`, `world-membership.schema.json`, `fork-branch.schema.json` | Narrative graph | `nexus-narrative` / bundles |
| `timeline-event.schema.json` | Narrative Timeline on wire | `nexus-narrative` + sync bundles |
| `key-block.schema.json` | **DELETED (V1.139).** Replaced by spoke `knowledge-entry.schema.json` — nexus consumes `KnowledgeEntry` from `@42ch/spoke-schemas` / `spoke-schemas`. See [`spoke-adapter-architecture.md`](spoke-adapter-architecture.md) §3. | Nexus KB type now sourced from spoke |
| `memory.schema.json` | Memory on wire | `nexus-creator-memory` |
| `story-manifest.schema.json` | Story summary on wire | `nexus-narrative`, novel-writing sync |

(bundle/delta/sync-command moved to `platform/sync/` in V1.62 P0 — they are sync payloads, not wire entities.)

[`domain/README.md`](../../schemas/domain/README.md) MUST list only files that exist under `schemas/domain/*.json`.

### 3.4 `common/`

- `common.schema.json` — identifiers and enums (data-model §7). Definitions-only; codegen emits `generated/common/common_types.rs` + `CommonTypes.ts`.
- `source-anchor.schema.json`, `version-ref.schema.json` — value objects §6. `SourceAnchor` is emitted into `common_types`.
- Do not add local-only enums here; if no external client observes an enum, put it in `src/local/`.

**Meta schema (local, not a `schemas/` folder):** `crates/nexus-contracts/src/local/meta.rs`. Removed `schemas/meta/` (V1.4 WS5 + V1.21 cleanup).

### 3.5 `daemon-api/compute/`

- Compute module ABI envelopes consumed by **external** WASM compute modules and generated clients: `compute-input.schema.json`, `compute-output.schema.json`.
- These are cross-language contracts (Rust host ↔ wasm32 module), so they live under `schemas/` and run through codegen, not as hand-written local types.
- Per-module shape declarations do **not** live here — they live in each module's `manifest.json` `schemas` block. See [modules/README.md](../../modules/README.md).
- `$id` / `$ref` URIs use `https://nexus42.invalid/schemas/daemon-api/compute/...`.
- Maintain [`daemon-api/compute/README.md`](../../schemas/daemon-api/compute/README.md). Compute ABI normative detail: [compute-module-abi.md](./compute-module-abi.md). Host-side runtime detail: [wasm-host.md](./wasm-host.md).

### 3.5A `daemon-api/common/`

- Shared Daemon API contract details consumed across resource handlers and generated clients.
- `error-response.schema.json` defines the canonical `ErrorResponse { code, message, details? }` error detail. On the wire, the daemon wraps it as `{ success: false, error: { code, message, details?, request_id? } }`; `request_id`, when present, is nested inside `error`.
- `$id` / `$ref` URIs use `https://nexus42.invalid/schemas/daemon-api/common/...`.

### 3.6 `daemon-api/<concern>/`

- Cross-language contracts for the daemon's `/v1/daemon/*` endpoints.
- Current concerns are the directories enumerated in §1. Add one subfolder per
  externally consumed daemon concern; do not mirror internal Rust module
  structure into this tree.
- `$id` / `$ref` URIs use
  `https://nexus42.invalid/schemas/daemon-api/<concern>/...`.
- **Codegen**: generated Rust modules live under `generated::daemon_api`;
  TypeScript modules live under
  `packages/nexus-contracts/src/generated/daemon-api/`.
- **Drift detection**: promoted schemas are registered with
  `CheckMode::Strict`.

Chapter contracts live in `daemon-api/works/chapters/` and use
`/v1/daemon/works/{work_id}/chapters/*`; preset contracts live in
`daemon-api/preset-management/`. Detailed chapter semantics:
[chapter-content-local-api.md](./chapter-content-local-api.md), whose filename
is historical.

---

## 4. Content hygiene

| Check | Action |
| --- | --- |
| README vs disk | Every `schemas/**/README.md` matches `*.json` in that folder |
| Stale `acp-runtime` / `cloud-sync` / `compute` references | Remove from active plans/docs; types moved/deleted (see §1 + §5) |
| `OutboxEntry` | **Local only** — must not reappear in `schemas/domain/` |
| `key-block` on wire | **Deleted from `schemas/domain/`.** Knowledge-entry contracts come from spoke `knowledge-entry.schema.json`; do not recreate `key-block.schema.json` |
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
| `schemas/compute/` | `schemas/daemon-api/compute/` (+ entity-* deleted) | 2026-06-23 (V1.62 P0) moved first to historical `schemas/local-api/compute/`; the later Daemon API rename produced the current path |
| `schemas/local-api/` | `schemas/daemon-api/` | Daemon API namespace rename; current generated modules are Rust `generated::daemon_api` and TypeScript `generated/daemon-api` |
| `schemas/domain/{bundle,delta,sync-command}` | `schemas/platform/sync/` | 2026-06-23 (V1.62 P0) — sync payloads, not wire entities |
| `schemas/platform/*.schema.json` (flat) | `schemas/platform/http-bff/*.schema.json` | 2026-06-23 (V1.62 P0) — consumer-scope split into `http-bff/` + `sync/` |

**Do not rename** `platform/` → `cloud-platform/` (platform HTTP naming is stable in v1-spec).

---

## 6. Related platform paths

Platform prose may still say `v1-spec/cli-sync/` for sync **protocol** documents. That is the **platform repo folder name**, independent of OSS `schemas/platform/sync/`. Coordinate `@42ch/nexus-contracts` semver when platform consumes regenerated types after URI path changes.

---

## 7. Wire file inventory (2026-09, current through V1.183)

The table records expected directory membership and deliberately avoids a
hand-maintained aggregate count. `schemas/README.md` and
`pnpm run validate-schemas` remain the count authorities.

| Directory | Files | Notes |
| --- | --- | --- |
| `common/` | 3 | `common`, `source-anchor`, `version-ref` |
| `domain/` | 10 → **9** (V1.139: `key-block.schema.json` deleted) | Wire entities (see §3.3 table) |
| `platform/http-bff/` | Current disk inventory | Platform HTTP bodies (flat; prefix grouping in [http-bff/README.md](../../schemas/platform/http-bff/README.md)) |
| `platform/sync/` | Current disk inventory | Bundle, pull, delta, command, and conflict contracts |
| `daemon-api/common/` | Current disk inventory | Shared error/envelope contracts |
| `daemon-api/compute/` | Current disk inventory | Compute input/output ABI |
| `daemon-api/works/` | Current disk inventory | CRUD plus chapter subtree and later works surfaces |
| `daemon-api/kb/` | Current disk inventory | Work-scope KB entry contracts |
| `daemon-api/findings/` | Current disk inventory | Quality findings contracts |
| `daemon-api/schedule/` | Current disk inventory | Schedule + core-context contracts |
| `daemon-api/workspace/` | Current disk inventory | Workspace management contracts |
| `daemon-api/creators/` | Current disk inventory | Creator management contracts |
| `daemon-api/orchestration/` | Current disk inventory | Sessions and capability contracts |
| `daemon-api/preset-management/` | Current disk inventory | Preset management contracts |
| `daemon-api/agent-host/` | Current disk inventory | Provider scan and agent-host contracts |
| `daemon-api/{canvas,check,inspector,reading}/` | Current disk inventory | Authoring and diagnostic surfaces |
| `daemon-api/{memory,timeline,worlds}/` | Current disk inventory | Narrative resource surfaces |
| `daemon-api/{runtime,tools}/` | Current disk inventory | Runtime and tool-bridge surfaces |

Do not hand-maintain an exact total here; `schemas/README.md` and `pnpm run validate-schemas` are the count authorities. This inventory records expected directory membership and notable V1.64 deltas.

**Not in tree:** `acp-runtime/`, `meta/`, `cli-sync/`, `cloud-sync/`, `compute/` (all removed/renamed).

Historical audit (pre-rename paths): [archived `schemas-boundary.md` §5.2](../archived/knowledge/schemas-boundary.md) — use this section for current paths.

---

*Normative Master. Current external Daemon API contracts live under `schemas/daemon-api/`; generated authorities are Rust `generated::daemon_api` and TypeScript `generated/daemon-api`. Boundary rule: [schemas-external-consumer-boundary.md](schemas-external-consumer-boundary.md).*
