# Compute Module Foundation & Visibility — Iteration Spec

| Attribute | Value |
| --- | --- |
| **Status** | Draft (V1.114) |
| **Document class** | Iteration-scoped spec draft |
| **Scope** | Compute-module registry Daemon API + computable KeyBlock state read endpoint + Control Room Modules panel + compute onboarding doc consolidation |
| **Coordinates with** | [compute-module-abi.md](../../../specs/compute-module-abi.md), [wasm-host.md](../../../specs/wasm-host.md), [entity-scope-model.md](../../../specs/entity-scope-model.md) §5.5.9, [daemon-api-surface-conventions.md](../../../specs/daemon-api-surface-conventions.md) |
| **Parent plan** | `2026-07-13-v1.114-compute-module-foundation` |

## Product framing (why authors care)

**Compute Modules are the deterministic engine** of Nexus — WASM units that apply
world rules (combat resolution, dice, relationship math) to **computable
KeyBlocks**. Unlike agents (which assist and suggest), compute is meant to be
*predictable*: same inputs → same state mutation and battle report.

Today that engine runs, but authors cannot participate as informed operators:

| Author question | Today | After V1.114 |
| --- | --- | --- |
| What compute modules does my install have? | Unknown — only orchestration "knows" | Modules panel + registry API |
| What does a module need from my world? | Buried in `manifest.json` / docs | Panel shows required KeyBlock types |
| What state did combat leave on my character? | `body.state` is write-only to the UI | State is readable for computable blocks |
| Can I trust compute as a product pillar? | Invisible = not a selling point | Visible + inspectable foundation |

This plan is **comprehensibility first**: discover modules, read what they
declare, inspect the state they own. It deliberately does **not** add "Run
compute" buttons — orchestration remains the runner. Visibility is the missing
floor under marketplace, state editors, and canvas intersection later.

## User stories

1. **As a game-bible / novel author with combat in my world**, I open Control
   Room → Modules and see `basic-combat` listed with a short description, so I
   know my install actually includes the deterministic combat engine.
2. **As an author inspecting a character KeyBlock**, I can see the dynamic
   compute `state` (e.g. `current_hp`) that modules mutate — not only static
   attributes — so world state is not a black box after a session.
3. **As an author deciding whether to lean on compute**, I can read which
   KeyBlock types a module requires and match that against my World KB before
   I invest structure in computable blocks.
4. **As a module author / contributor**, I have a single onboarding path
   (consolidated docs + registry that reflects real `manifest.json` shapes)
   instead of hunting across README + ABI + wasm-host notes.
5. **As a future product iteration**, I can build state editors, canvas badges,
   or marketplace UX on top of registry + state-read APIs without inventing
   discovery from scratch.

## Problem

The V1 compute ABI is normative and `nexus-wasm-host` runs modules — but compute
is **completely invisible** to authors:

1. **No module discovery surface**: there is no Daemon API endpoint to list
   installed compute modules or inspect their manifests. The only module
   (`basic-combat`) is known to orchestration but not to the UI. Authors cannot
   answer "what compute modules does my Nexus install have?"
2. **Computable state is write-only**: compute modules mutate KeyBlock `body.state`
   (e.g., `character.current_hp`), but the Daemon API World KB graph response
   does not surface `state`. Authors see the KeyBlock exists but not the
   deterministic state the compute engine tracks. This blocks all future
   compute-visualization work (the V2.0+ "KB state → human-readable UI editor"
   deferred item needs a read API first).
3. **No UI**: the Control Room has no Modules page, no compute panel, no
   indication that compute exists. For a pillar that is supposed to be a major
   selling point, it is hidden.

This is the foundational gap: before future compute iterations (module
marketplace, multi-module composition, canvas integration), authors need to
**see and understand** what compute does today.

## Goals

1. **Compute-module registry Daemon API** — add `GET /v1/daemon/compute/modules`
   (list installed modules with manifest summary) and `GET
   /v1/daemon/compute/modules/{module_id}` (full manifest). Reuse the existing
   `manifest.json` shape from `compute-module-abi.md` §7 (codegenerated). The
   endpoint reads from the module discovery path already used by
   `nexus-wasm-host` embedded/module loader.
2. **Computable KeyBlock state read** — add a dedicated
   `GET /v1/daemon/worlds/{world_id}/kb/key-blocks/{key_block_id}/state` to
   surface the `body.state` of computable KeyBlocks. Read-only; no mutation.
   This unblocks future compute-visualization UI without waiting for V2.0.
3. **Control Room Modules panel** — a new Control Room page
   (`/modules`) under a **"Compute" nav section** that lists installed compute
   modules (name, version, description, required KeyBlock types, battle-report
   kind) and links to the worlds/KeyBlocks they apply to. Minimal but real —
   authors can finally see and understand compute.
4. **Compute onboarding documentation consolidation** — consolidate
   `modules/README.md` + `compute-module-abi.md` authoring guidance into a
   clear module-author onboarding path. Update `wasm-host.md` to reference the
   new registry endpoints.

### Architect decisions (locked by @architect — Review chain Seat 2)

- **Computable-state read → Option B (dedicated endpoint).** The graph response
  is a *rendering projection* for canvas; it loads eagerly on surface open.
  Compute state is an *inspect* action (author clicks a character to see
  `current_hp`), not a *render* action. Adding `state` to every entity in the
  graph response bloats a response that canvas already loads on open, and
  conflates "projection summary" with "full state dump." A dedicated endpoint
  is focused, independently cacheable, and follows the daemon-api convention
  of focused read endpoints. The endpoint reads `body.state` directly from the
  KeyBlock (already stored in SQLite `kb_key_blocks.body`) — it does not invent
  a parallel read path.

  Note: the entity projection schema (`world-kb-entity-projection.schema.json`)
  already has an optional `body` field whose description mentions `state`.
  Whether the handler currently populates `body.state` in the graph projection
  is an implementation detail; the dedicated endpoint is the *authoritative*
  state-read surface regardless.

- **Modules nav → dedicated "Compute" section.** "Capabilities" is an
  orchestration concept (AI-assisted). "Compute modules" is a deterministic-engine
  concept. Putting Compute under Capabilities conflates two execution models.
  A dedicated "Compute" nav section with a "Modules" entry (author-facing label)
  keeps compute visible and distinct. If product later merges them, that is a
  future IA decision.

- **Manifest schema promotion.** The manifest is currently a hand-written Rust
  struct (`nexus-wasm-host/src/manifest.rs::ModuleManifest`), not a codegenerated
  schema. The registry endpoints return manifest-derived shapes, so the manifest
  must be promoted to JSON Schema to avoid a parallel hand-written DTO. Add:
  - `schemas/daemon-api/compute/module-summary.schema.json` — list endpoint
    (module_id, name, version, description?, required_key_block_types,
    battle_report_kind?).
  - `schemas/daemon-api/compute/module-detail.schema.json` — detail endpoint
    (full manifest shape from compute-module-abi.md §7).
  The Rust `ModuleManifest` should be reconciled with the generated type after
  codegen (or replaced by it) to eliminate drift.

- **Registry logic placement.** Module discovery (`embedded_module_ids()`,
  `embedded_module_manifest()`) already lives in `nexus-wasm-host`. Add
  `list_module_summaries()` + `module_detail(id)` functions to
  `nexus-wasm-host` that parse + project manifests. The daemon-runtime crate
  wires thin HTTP handlers around them. No new crate for V1.114 (YAGNI with
  one module); extract `nexus-compute-registry` later if CDN/marketplace grows
  the registry surface.

## Non-goals

- Invoking compute modules from the UI (compute runs from orchestration; UI
  invocation is a future intersection iteration)
- Authoring new compute modules (basic-combat is the proof target)
- Multi-module composition (V2.0+ ABI)
- CDN distribution / signing (V2.0+)
- Module marketplace / public registry (V3.0+)
- Computable-state editor UI (read-only in V1.114; the V2.0+ "KB state →
  human-readable UI editor" remains deferred but now has the read foundation)
- Canvas integration of compute results / battle-report chrome on nodes
  (future intersection iteration — needs this registry + state read first)
- Module install/uninstall UX (list what is already installed only)

## Acceptance criteria (product-facing)

- [ ] An author can open a Modules panel in Control Room and see at least
  `basic-combat` with name, version, and description
- [ ] Module detail surfaces required KeyBlock types (and battle-report kind when
  declared) so authors can match modules to world structure
- [ ] For a computable KeyBlock, dynamic `state` is readable via the dedicated
  `GET /v1/daemon/worlds/{world_id}/kb/key-blocks/{id}/state` endpoint;
  non-computable blocks return `is_computable: false` (do not fake state)
- [ ] Registry list/detail endpoints exist and return shapes consistent with
  existing `manifest.json` (no parallel invented module DTO)
- [ ] Module-author docs point to one onboarding path + the new registry routes

## Interface sketch (locked by @architect — Review chain Seat 2)

### Registry endpoints (additive)

```
GET /v1/daemon/compute/modules
  → 200 { items: ComputeModuleSummary[], has_more: false }
     ComputeModuleSummary = { module_id, name, version, description?,
                              required_key_block_types, battle_report_kind? }

GET /v1/daemon/compute/modules/{module_id}
  → 200 ComputeModuleDetail   (full manifest.json shape per abi §7)
  → 404 if unknown module_id
```

Schemas: `schemas/daemon-api/compute/module-summary.schema.json`,
`schemas/daemon-api/compute/module-detail.schema.json` (promotes the manifest
to a wire contract — see Architect decisions above).

### Computable-state read — Option B: dedicated endpoint (locked)

```
GET /v1/daemon/worlds/{world_id}/kb/key-blocks/{key_block_id}/state
  → 200 { state: Record<string, unknown>, is_computable: true, version: integer }
  → 200 { state: null, is_computable: false, version: integer }   # non-computable block
  → 404 if key_block_id unknown
```

Rationale: see Architect decisions above. The graph response is not extended;
state is a focused inspect read. `version` mirrors the per-row OCC revision
(`kb_key_blocks.revision`) so callers can detect staleness without a separate
fetch.

Schema: `schemas/daemon-api/canvas/world-kb/key-block-state-response.schema.json`.

### Modules panel UI

- New route `/modules` in the Control Room.
- Nav entry under a dedicated **"Compute" section** (locked by @architect).
  Label: "Modules" (author-understandable, not internal crate names).
- Lists modules from the registry endpoint.
- Per-module detail: manifest fields + "Required KeyBlock types" + link to
  worlds that have those block types (when feasible without new heavy queries).
- Read-only in V1.114 — no Run / Install actions.

## Verification

- New Daemon API tests: registry list/detail happy path + 404; computable-state
  read returns `state` + `is_computable: true` for computable blocks,
  `state: null` + `is_computable: false` for non-computable.
- `basic-combat` module appears in the registry.
- New web tests: Modules panel renders module list; detail shows manifest fields.
- Codegen: new schemas under `schemas/daemon-api/compute/` +
  `schemas/daemon-api/canvas/world-kb/` produce Rust + TypeScript types;
  `@42ch/nexus-contracts` minor bump. The hand-written `ModuleManifest` in
  `nexus-wasm-host` is reconciled with the generated type (no parallel DTO).
- Existing compute tests (`nexus-wasm-host`) all pass.
- Human smoke: open Modules panel on a fresh install → see `basic-combat` →
  open a computable KeyBlock path and confirm state is readable.
