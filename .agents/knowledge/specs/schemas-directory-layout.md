# `schemas/` Directory Layout (Wire Contracts)

## 0. Document position

| Attribute | Value |
| --- | --- |
| **Status** | Active — normative layout for JSON Schema wire tree |
| **Scope** | Folder names, product-line mapping, README rules, rename policy; **not** field-level DTO definitions (those stay in platform `v1-spec` + `data-model-v1`) |
| **Boundary rule** | [schemas-wire-platform-sync-boundary.md](../schemas-wire-platform-sync-boundary.md) — what may appear under `schemas/` at all |
| **Crate alignment** | [local-cloud-crate-architecture.md](./local-cloud-crate-architecture.md) — `nexus-contracts` generated vs `src/local/` |
| **Codegen** | [schemas/AGENTS.md](../../../schemas/AGENTS.md), [tooling/AGENTS.md](../../../tooling/AGENTS.md) |

**Do not confuse:**

- **`schemas/domain/`** — wire **entity** shapes (Creator, World, Bundle, …) used on platform-observed boundaries.
- **`nexus-domain` / `nexus-cloud-domain` crates** — Rust **logic** crates; the monolith `nexus-domain` name is **retired** (platform slice → `nexus-cloud-domain`).

---

## 1. Normative tree (2026-05)

All paths are under repository root `schemas/`. Only **wire** files belong here (see boundary doc).

```text
schemas/
├── AGENTS.md              # codegen + drift rules (mandatory read)
├── README.md              # index (this layout + file counts)
├── common/                # shared identifiers, enums, timestamps ($ref'd by wire)
├── domain/                # wire domain entities + bundle/delta/sync-command
├── platform/              # platform HTTP request/response bodies (BFF contracts)
├── cli-sync/              # CLI ↔ platform sync protocol envelopes (cloud line)
└── meta/                  # POINTER ONLY — meta-schema moved to local Rust (see §4)
```

**Removed (do not recreate):**

- `schemas/acp-runtime/` — local-only; types in `crates/nexus-contracts/src/local/acp_runtime/`.
- Any daemon `/v1/local/*` DTO as JSON Schema — use `src/local/` (orchestration, schedule, daemon status, registry manifest).

---

## 2. Folder ↔ product line ↔ consumers

| Directory | Product line | Platform observes? | Primary Rust consumer | npm `@42ch/nexus-contracts` |
| --- | --- | --- | --- | --- |
| **`platform/`** | Cloud enhancement (platform HTTP) | **Yes** — BFF bodies | `nexus-cloud-sync` (HTTP client), platform TS | **Yes** |
| **`cli-sync/`** | Cloud enhancement (bundle / pull / conflict) | **Yes** — sync wire | `nexus-cloud-sync` (`legacy-sync`) | **Yes** |
| **`domain/`** | Wire entities embedded in bundles & platform bodies | **Yes** (transitive via `$ref`) | All cloud-line crates + generated imports | **Yes** |
| **`common/`** | Shared wire value objects | **Yes** (when `$ref`'d) | Generated | **Yes** |
| **`meta/`** | *(none — see §4)* | **No** | — | **No** |

**Local product line** (daemon, orchestration, agent-host) MUST NOT add new subtrees under `schemas/`. Add types under `crates/nexus-contracts/src/local/{acp_runtime,domain,orchestration,schedule}/`.

---

## 3. Subdirectory contracts

### 3.1 `platform/`

- One schema file per **platform HTTP** request or response shape (or shared response fragment), kebab-case basename.
- **Not** daemon Local API proxies (V1.20 removed world/explore **daemon** routes; platform HTTP contracts **remain** wire here).
- Grouping is **flat** (no `platform/explore/` subfolders in V1.21) — use filename prefix: `explore-*`, `world-*`, `publish-*`, `notifications-*`, `context-assembly-v1`, etc.
- Maintain [`platform/README.md`](../../../schemas/platform/README.md) index when adding files.

### 3.2 `cli-sync/` (target rename: `cloud-sync/`)

- Sync protocol: bundle envelope view, pull request/response, conflict response.
- **`domain/bundle.schema.json`** is the **codegen canonical** `Bundle` type; `cli-sync/bundle.schema.json` is a **validation refinement** (see `tooling/codegen/src/schema-loader.ts` `SKIP_STRUCT_GENERATION_REL_PATHS`).
- **Target folder name (post–V1.21):** `schemas/cloud-sync/` to align with crate `nexus-cloud-sync` and [local-cloud-crate-architecture.md](./local-cloud-crate-architecture.md). Rename requires updating every `$id` / `$ref` URI and coordinated `@42ch/nexus-contracts` release (see §5).

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
| `bundle.schema.json`, `delta.schema.json`, `sync-command.schema.json` | Sync payloads | `nexus-cloud-sync` |

[`domain/README.md`](../../../schemas/domain/README.md) MUST list only files that exist under `schemas/domain/*.json`.

### 3.4 `common/`

- `common.schema.json` — identifiers and enums (data-model §7).
- `source-anchor.schema.json`, `version-ref.schema.json` — value objects §6.
- Do not add local-only enums here; if platform never observes a enum, put it in `src/local/`.

### 3.5 `meta/` (deprecated path)

- **`meta.schema.json` is not in `schemas/`** — moved to hand-written `crates/nexus-contracts/src/local/meta.rs` (V1.4 WS5).
- Keep `schemas/meta/README.md` as a **pointer** only; do not restore JSON unless CI validation requires a committed meta-schema file (then document as repo-internal, still **not** wire).

---

## 4. Content hygiene (V1.21 program)

| Check | Action |
| --- | --- |
| README vs disk | Every `schemas/*/README.md` matches `*.json` in that folder |
| Stale `acp-runtime` references | Remove from plans/docs; types are under `src/local/acp_runtime/` |
| `OutboxEntry` | **Local only** — must not reappear in `schemas/domain/` |
| `key-block` on wire | Stays in `schemas/domain/` if platform/sync bundles carry KeyBlocks; narrative **logic** is `nexus-kb` |
| Platform grep before delete | `rg <TypeName>` on `nexus-platform` before removing any schema file |

**Re-audit:** Refresh the file-level table in [archived schemas-boundary §5.2](../archived/knowledge/schemas-boundary.md) or add a dated appendix under this doc when moving or renaming folders.

---

## 5. Rename policy: `cli-sync/` → `cloud-sync/`

| Phase | Action |
| --- | --- |
| **V1.21 (docs)** | Specs and READMEs may say **target** name `cloud-sync/` while disk remains `cli-sync/` |
| **V1.21 (optional code)** | Mechanical rename + `$id`/`$ref` URI update + `pnpm run codegen` + platform contracts bump |
| **If deferred** | Document in plan residual; physical path stays `cli-sync/` until next semver-coordinated change |

**Do not rename** `platform/` → `cloud-platform/` (platform HTTP naming is stable in v1-spec).

---

## 6. Related platform paths

Platform prose may still say `v1-spec/cli-sync/` for sync **protocol** documents. That is the **platform repo folder name**, independent of OSS `schemas/cli-sync/` directory name. When renaming OSS folders, add a one-line cross-reference in platform ADR or sync-contract doc in the **same release train**.

---

*Layout SSOT. Implementation: V1.21 plan Batch G; wire/local rule: [schemas-wire-platform-sync-boundary.md](../schemas-wire-platform-sync-boundary.md).*
