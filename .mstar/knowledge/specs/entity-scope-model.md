# Nexus Entity Scope Model

## 0. Document position

| Attribute | Value |
| --- | --- |
| **Status** | Normative — entity scope hierarchy, uniqueness, crate ownership. **V1.40 Shipped**: §5.1.1 narrative taxonomy (`BlockType` + `novel_category` + `canonical_name` grammar). **V1.50 Shipped**: §5.5 World KB promotion state machine. **V1.51 Shipped**: §5.5.6 LLM pathway subsection. **V1.54 Shipped**: §5.1.1 game-bible taxonomy. **V1.55 P3**: §5.1.1 script taxonomy. **V1.62 Shipped**: §5.5.9 computable-flag semantics + structured validation mode (closes `R-V161P1-LOW-001`). **V1.74 Shipped β**: §5.6 World KB relationship semantics. |
| **Document class** | Master |
| **Scope** | Global/User/Creator/World/Timeline/Event/Moment hierarchy; entity ownership; `kb`/`knowledge` naming boundaries; scope transition rules |
| **Last updated** | 2026-06-23 — V1.62 P2 §5.5.9 computable-flag semantics + structured validation mode |
| **Related** | [local-cloud-crate-architecture.md](./local-cloud-crate-architecture.md), [cli-spec.md](./cli-spec.md), [daemon-runtime.md](./daemon-runtime.md), [orchestration-engine.md](./orchestration-engine.md), [compute-module-abi.md](./compute-module-abi.md), [wasm-host.md](./wasm-host.md), [local-db-schema.md](./local-db-schema.md), [`docs/ARCHITECTURE.md`](../../../docs/ARCHITECTURE.md) |

This file is normative for V1.23 crate wiring and naming alignment. When this file
overlaps older wording in prior specs, keep the locked decisions here and update the
older wording during the relevant follow-up task.

---

## 1. Scope hierarchy

### 1.1 Canonical tree

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

### 1.2 Scope definitions

| Scope | Meaning | Parent | Normative notes |
| --- | --- | --- | --- |
| `Global` | Product-wide global scope | None | Contains product-level contracts, schema/version conventions, and process-wide constants. It is not a tenant or actor scope. |
| `User` | Human user scope | `Global` | Owns human account/session concepts and user-level knowledge. User is the cloud account and pairing actor, not the operational agent identity. |
| `Creator` | Agent / creator persona scope | `User` | Owns Creator identity, local workspace partitioning, SOUL, long-term memory, and agent-facing operational context. A Creator may be created independently and later paired to a User, but once paired it is modeled under the User scope. |
| `World` | World / narrative universe scope | `Creator` | Owns creative-work narrative state: world state, story/manuscript projections, forks, memberships, and the narrative KB graph. |
| `Timeline` | Timeline scope inside a world | `World` | Owns ordered narrative history inside one World. Timeline history is append/fork oriented; rewriting prior history requires a Fork, not in-place mutation. |
| `Event` | Event scope inside a timeline | `Timeline` | Owns an immutable narrative occurrence or change point in a Timeline. |
| `Moment` | Moment / session-start context point | `Event` | Owns the session-start context assembly point: a read-only snapshot assembled before an agent session begins. |

### 1.3 Nesting rules

1. Every scoped entity MUST have exactly one canonical owning scope.
2. Scope containment is strict and directional: lower scopes may reference ancestor IDs, but they MUST NOT claim ownership outside their parent chain.
3. `World`, `Timeline`, `Event`, and `Moment` are narrative scopes. Their domain logic belongs to the local/narrative product line, not to cloud transport.
4. `User` and `Pairing` are cloud-domain concepts. Cloud transport MUST use `nexus-cloud-domain` for their invariants.
5. Local `workspace_slug` is a storage partition under the active Creator. It can bind or stage multiple Worlds, but it is not a canonical entity scope in this hierarchy.

---

## 2. Entity type catalog

| Scope | Entity types that live here | Primary owner crate(s) |
| --- | --- | --- |
| `Global` | Schema bundle identity, contract schema versions, product-wide command/capability names, global daemon/runtime constants | `nexus-contracts`; `nexus42` for CLI surface; `nexus-daemon-runtime` for local runtime process constants |
| `User` | User account/profile, platform session, Pairing records, user-level knowledge index, user-global reference corpus | `nexus-cloud-domain` for User/Pairing invariants; `nexus-cloud-sync` for HTTP transport; `nexus-knowledge` for user-scoped knowledge/reference index |
| `Creator` | Creator aggregate, Creator credentials/cache records, active Creator selection, workspace registrations, SOUL, long-term memory, review queue/personality I/O | `nexus-creator`; `nexus-creator-memory`; `nexus-home-layout`; `nexus-local-db` for local persistence mechanics |
| `World` | World aggregate, world membership, fork branches, story manifests, manuscript state/projections, narrative KB graph, KeyBlocks, SourceAnchors | `nexus-narrative`; `nexus-kb` for KB graph insertion/query and KeyBlock/SourceAnchor logic |
| `Timeline` | Timeline branches/ordered history, timeline events, fork positions, recent timeline query projections | `nexus-narrative` |
| `Event` | Timeline event records, event-level deltas, event source references, event-to-moment anchors | `nexus-narrative`; `nexus-kb` when an event produces or references narrative KB assets |
| `Moment` | Session-start context assembly request/response, assembled prompt context, selected memory/knowledge/narrative slices, token-budgeted snapshot | `nexus-moment-context-assembly` |

### 2.1 Local storage catalog

Local persistence is an implementation boundary and does not add new canonical scopes:

- `$HOME/.nexus42/creators/<creator_id>/workspaces/<workspace_slug>/state.db`
  is the per-Creator/per-workspace local working copy from [local-db-schema.md](./local-db-schema.md).
- `workspace_slug` is unique under `creator_id` and is managed by CLI/daemon local state.
- A workspace may stage or bind multiple `world_id` values; requests that touch a World MUST carry `world_id` explicitly.

---

## 3. Uniqueness constraints

### 3.1 Formal rule

For non-fungible unique entities, the uniqueness key is:

```text
(scope_kind, scope_id, entity_class_or_template_id)
```

Within the same `scope_id`, the same `entity_class_or_template_id` MUST have at
most one active instance. Different classes/templates do not share this constraint.

Examples:

- A `World` may have at most one active instance of a given unique template under the same `creator_id` if that template is declared unique at Creator scope.
- The same `canonical_name` may be unique for one `BlockType` under a `world_id`, while a different `BlockType` with the same display text remains a different class/template and is not blocked by that uniqueness rule.
- A unique Moment template may occur once for a given `event_id`; the same template may occur again under a different `event_id`.

### 3.2 Non-goals of this rule

This rule does not make all entities unique. Append-only or fungible records such as
timeline events, source observations, audit entries, outbox commands, and historical
versions may have multiple instances under the same scope unless their class/template
explicitly declares uniqueness.

### 3.3 Lifecycle semantics

1. The uniqueness check applies to active instances.
2. Deprecated, merged, deleted, or archived records MAY be retained for audit/history.
3. Replacing a unique entity MUST preserve traceability to the replaced instance when the domain supports history (`replacement_id`, merge target, fork relation, or equivalent).

---

## 4. Crate ownership map

| Crate | Scope ownership | Responsibility boundary |
| --- | --- | --- |
| `nexus-contracts` | Cross-scope type foundation | Owns generated wire types and local DTOs. No business logic, no I/O, no parallel duplicate DTOs in application crates. |
| `nexus-home-layout` | Storage paths for `User`/`Creator` local material | Owns stable `~/.nexus42/` path helpers only. It does not own entity invariants. |
| `nexus-local-db` | Storage mechanics across Creator/workspace working copies | Owns SQLite initialization, migration, versioning, and shared local persistence APIs. It does not own narrative or User semantics. |
| `nexus-cloud-domain` | `User`, `Pairing` | Owns User/Pairing domain invariants and mappings from contract types. No HTTP transport. |
| `nexus-cloud-sync` | Cloud transport for User/Pairing and sync bundles | Owns platform HTTP and sync transport. It MUST use `nexus-cloud-domain` for User/Pairing invariants instead of reimplementing or bypassing them; current Cargo wiring includes this dependency. |
| `nexus-creator` | `Creator` | Owns Creator aggregate logic, credential/cache hooks, active Creator local state, and conversions over contract types. No platform HTTP. |
| `nexus-creator-memory` | `Creator` memory subdomain | Owns SOUL, long-term memory, review, and personality/experience I/O under Creator scope. |
| `nexus-knowledge` | `User` knowledge | Owns user-scoped global knowledge/reference indexing and storage. It is not Creator-scoped and does not own narrative KeyBlocks. It provides material that moment assembly may pull into a Moment. |
| `nexus-narrative` | `World`, `Timeline`, `Event` | Owns creative-work narrative state: current work background, world state, forks, timelines, events, story/manuscript projections, and narrative consistency. |
| `nexus-kb` | World-scoped narrative KB graph | Owns narrative knowledge assets under `World`: KeyBlocks, SourceAnchors, graph insertion/query, and narrative KB lifecycle. KB scope belongs under a World/narrative entity, not directly under Creator. |
| `nexus-moment-context-assembly` | `Moment` | Owns session-start moment context aggregation. It runs before a session begins and aggregates relevant local domains: Creator memory, narrative state, World KB assets, and User knowledge. Optional `cloud-stage` may merge platform context, but daemon default remains local Stage-0. |
| `nexus-daemon-runtime` | Runtime host, not entity owner | Hosts local APIs, DB handles, orchestration, and agent-host. It MUST NOT own cloud transport or platform User/Pairing invariants. |
| `nexus-orchestration` | Execution sessions/schedules, not hierarchy owner | Owns presets, schedules, workers, and capability registry. It carries `creator_id`/workspace/world references as execution context, but it does not redefine entity ownership. |

#### 5.5.8 Conditional routing branch input visibility (V1.56 P3 amendment)

When the conditional routing engine (DF-56) evaluates a state's `next: { kind: conditional, branches: [...] }` expression, the engine may invoke capabilities and read workspace state to populate the expression context. Branch decision inputs are **read-only projections** of underlying entity state:

- **`_context.registry_refresh.*`** — fields projected from the `nexus.registry.refresh` capability output (which is itself an entity-scope snapshot of the capability registry). Fields include `source` (`synthetic` | `network` | `synthetic_fallback`), `snapshot_version`, `capability_count`, `fallback_reason` (CdnError variant stringified per V1.56 P1 fix-wave), `retry_count`. The branch input is read-only; the capability invocation does not mutate the registry.
- **`_context.workspace.*`** — fields projected from the active workspace session (V1.56 P0 OCC + persistent session). Fields include `session_id`, `conflict_detected` (bool; OCC outcome of last commit), `changes_applied` (count of paths committed), `workspace_root` (canonical path). The branch input is read-only; expression evaluation does not invoke `workspace.commit` itself.

Branch inputs do **not** redefine entity ownership. The expression evaluator reads through existing API surfaces:

- Registry refresh input: `nexus-orchestration::tasks::inject_registry_refresh_context()` → reads `nexus.registry.refresh` capability output via existing capability registry machinery.
- Workspace input: `nexus-orchestration::tasks::inject_workspace_context()` → reads active workspace session via existing `nexus-daemon-runtime` workspace handlers.

This amendment exists to clarify that DF-56 conditional routing branch inputs are **derived from**, not **owners of**, the entity scope. The entity ownership hierarchy (Creator > World > Timeline > Event > Moment) is unchanged; routing inputs are projections.

#### 5.5.9 Computable-flag semantics and structured validation mode (V1.62 P2)

**Status**: Normative — V1.62 Shipped (closes `R-V161P1-LOW-001`).

V1.61 added `computable: Option<bool>` and `state: Option<serde_json::Value>` to
`KeyBlockBody` (see `schemas/domain/key-block.schema.json`). V1.62 formalizes
the semantics of these fields and establishes the structured validation mode that
was deferred from V1.61 P1.

##### 5.5.9.1 `computable` flag

The `computable` boolean field on `KeyBlockBody` marks a KeyBlock as
participating in WASM compute. Its semantics are:

| Value | Meaning |
| --- | --- |
| `Some(true)` | The KeyBlock participates in WASM compute. Its `body` must carry `attributes` (immutable compute params, e.g., `max_hp`, `base_atk`) and may carry `state` (mutable runtime data, e.g., `current_hp`). The host includes computable KeyBlocks in `ComputeInput.key_blocks` when their `block_type` matches the module's `required_key_block_types`. |
| `None` or `Some(false)` | The KeyBlock does not participate in WASM compute. It may still appear in `ComputeInput.key_blocks` for read-only reference (e.g., a scene KeyBlock providing narrative context), but the host does not apply `state_delta` operations to it. |

The flag is a filterable marker — the KB query layer
(`KbQuery::with_computable(bool)`) can select only computable KeyBlocks when
building the invocation snapshot. This was implemented in V1.61 P1
(`crates/nexus-kb/src/query.rs`, `InMemoryKbStore`, `SqliteKbStore`).

##### 5.5.9.2 `state` field

The `state` field on `KeyBlockBody` holds **mutable runtime data** scoped to the
KeyBlock. Its shape is nested by `block_type` (compass V1.61 Q5):

```json
{
  "state": {
    "character": {
      "current_hp": 85,
      "status_effects": ["poisoned"],
      "is_alive": true
    }
  }
}
```

The nesting by `block_type` avoids field-name collisions when the same KeyBlock
is used by different module types. The `state` object is mutable — the host
applies `state_delta` operations (`+/-/set`) from `ComputeOutput.state_delta`
(see [compute-module-abi.md](./compute-module-abi.md) §5.1).

The `state` field is carried in the KeyBlock's `body_json` (TEXT column in
SQLite). No separate DB column is required — the JSON is transparent to the
storage layer (compass V1.61 design item #2). Growth is expected as modules
add per-block state, but individual state objects are small (character-scale
combat state is < 1 KiB).

##### 5.5.9.2.1 SQLite `body_json` growth expectations (R-V161P0-LOW-004)

The `body_json` TEXT column in `kb_key_blocks` stores the full `KeyBlockBody`
(including `state`) as serialized JSON. Computable KeyBlocks accumulate mutable
state across compute invocations — each `state_delta` apply writes back the
updated `body_json` via `SqliteKbStore::update_key_block`.

| Concern | Assessment | Mitigation |
| --- | --- | --- |
| Row size growth | Each computable KeyBlock's `body_json` grows proportionally to its state object. Typical character state (< 1 KiB) is negligible. Larger state shapes (e.g., 10 KiB terrain grids) would still be within SQLite's 1 GiB default `max_page_count`. | Per-module state schemas cap individual state objects (see `manifest.json` `schemas.key_block_state`). No per-row growth cap is needed at current scale. |
| DB file growth | The `kb_key_blocks` table is append-mostly (inserts on adopt/add; updates on state delta apply). Updates rewrite the row in-place via SQLite's B-tree (no append-only bloat beyond WAL). | SQLite WAL auto-checkpoints. `VACUUM` recovers space after bulk deletes — not needed for normal operations. |
| `json_extract` query cost | Computable query filters (`KbQuery::with_computable(true)`) use `computable` as a separate `WHERE` clause column (extracted from `body_json` during insert, not via `json_extract` at query time). State-path lookups (e.g. "all characters with `current_hp < 10`") are not a current query pattern. | No `json_extract` index is needed. If state-path queries become a hot path (≥ 10K computable KeyBlocks per world), add a per-`block_type` computed column or a `state_summary_json` denormalization column. Reassess at V2.0+ when user-authored compute modules introduce unbounded state shapes. |
| Migration risk | `body_json` is TEXT (no schema migration needed if the body shape evolves). Invalid JSON on read returns `None` for `body` — the KeyBlock is still returned but without a parsed body. | Serialization is validated at insert/update time. Existing rows with old shapes degrade gracefully (missing fields = `None` in the deserialized struct). |

The current design intentionally trades query flexibility for schema simplicity.
Post-1.0, if state-path queries emerge as a product requirement, a dedicated
`key_block_state` table (normalized, indexed) should be considered as a
replacement, not a supplement — avoiding two sources of truth for the same state.

##### 5.5.9.3 Structured validation mode

Per-module attribute and state shapes are **not** declared in product-level
schemas. V1.61 placed placeholder schemas under
`schemas/compute/compute-entity-attributes.schema.json` and
`schemas/compute/compute-entity-state.schema.json` — these are **deleted**
in V1.62 P0. Their content (per-`BlockType` shape declarations) is replaced
by per-module JSON Schema fragments declared in each compute module's
`manifest.json` `schemas` block (V1.62 P1; see
[compute-module-abi.md](./compute-module-abi.md) §7.3).

The structured validation mode is:

1. The `nexus-kb::validation` module provides `ValidationMode::Structured`
   (added in V1.61 P1, `crates/nexus-kb/src/validation.rs`). This mode
   requires `computable: true` on KeyBlocks that carry `state`.
2. Per-module attribute/state shapes are declared in the module's
   `manifest.json` `schemas.key_block_attributes[block_type]` and
   `schemas.key_block_state[block_type]` blocks.
3. At compute invocation time, the host (`nexus-wasm-host`) validates each
   KeyBlock in `ComputeInput.key_blocks` against the manifest-declared
   schemas (see [wasm-host.md](./wasm-host.md) §8.5).
4. Validation failures produce `ComputeError::ManifestValidationFailed`
   with a JSON path to the offending field.

The `ValidationMode::Structured` variant in `nexus-kb` validates the
computable flag and state field at the KB layer; the manifest-driven
validation in `nexus-wasm-host` validates the **shapes** of those fields
at compute time. Both layers are additive — KB validation ensures the
KeyBlock is structurally valid for compute; host validation ensures the
concrete values match the module's declared schemas.

##### 5.5.9.4 Relationship to deleted entity-* schemas

V1.61's `schemas/compute/compute-entity-attributes.schema.json` and
`schemas/compute/compute-entity-state.schema.json` are **deleted** in
V1.62 P0. Their content was per-`BlockType` placeholder shape declarations
(e.g., `character` attributes: `max_hp`, `base_atk`, `base_def`). V1.62
replaces this with:

- **Per-module shape declarations** in each module's `manifest.json`
  `schemas` block (V1.62 P1).
- **No product-level enumeration** of all possible BlockType attributes
  — the manifest is the module author's contract, not the platform's.

Specs and code MUST NOT reference the deleted paths
`schemas/compute/compute-entity-attributes.schema.json` or
`schemas/compute/compute-entity-state.schema.json`. The canonical
replacement is `manifest.json` `schemas` as documented in
[compute-module-abi.md](./compute-module-abi.md) §7.3.

| `nexus42` | CLI surface | Owns user-facing command routing and wording. It invokes the owning crates; it MUST NOT become a second domain implementation for scope rules. |

---

## 5. Naming clarifications

### 5.1 `nexus-kb` — World-scoped narrative KB

`nexus-kb` means narrative knowledge inside a `World`:

- KeyBlocks and SourceAnchors belong to the World/narrative graph.
- KB graph insertion/query is a `World` concern and is coordinated with `nexus-narrative`.
- `nexus-kb` MUST NOT be treated as generic Creator knowledge or User knowledge.

#### 5.1.1 Narrative World KB item taxonomy (V1.40 grill-me locked — **Shipped V1.40 P1**)

The generic `nexus-kb` persistence model stores World-scoped KeyBlocks with `block_type`, `canonical_name`, `body`, provenance anchors, and active uniqueness under `(world_id, block_type, canonical_name)` (see [local-db-schema.md](./local-db-schema.md) §4.1.2).

**SSOT for `block_type` (wire enum):** `schemas/common/common.schema.json` → `BlockType` → `@42ch/nexus-contracts` / `nexus-contracts`. Shipped values (snake_case on wire): `character`, `ability`, `scene`, `organization`, `item`, `conflict`, `info_point`, `event`. Implementations MUST NOT introduce a parallel `block_type` enum in `nexus-kb` or orchestration presets. `kb-extract`, `SqliteKbStore`, and `assemble_moment` / `fetch_world_kb` already use this vocabulary.

**Design decision: `environment` NOT in `BlockType` (R-V161P0-INFO-001).** The V1.61 compass initially named `environment` as a potential computable BlockType for environmental context (weather, terrain, lighting). After evaluation, `environment` was intentionally excluded from the wire enum:

1. `environment` is too broad for a single entity BlockType — it spans weather, terrain, celestial, ambient conditions, and multi-entity spatial state, all of which benefit from separate `KeyBlock` instances with different `block_type` values.
2. Environmental context is served by existing types: `scene` (spatial descriptions), `event` (ambient occurrences), `info_point` (environmental rules), and `location` (a novel category carried in `body.attributes`, not a wire enum variant).
3. Future profile-specific environmental modelling (e.g., game-bible `level` carrying terrain grid state, script `scene` carrying lighting cues) uses per-profile `body.attributes` shapes rather than a dedicated wire enum variant.

Module authors should use the `scene` + `info_point` BlockType combination to model environmental KeyBlocks, with per-module shape declarations in `manifest.json` `schemas` (see §5.5.9.3). If a future profile introduces a domain-specific environmental BlockType (e.g. `biome` for game-bible), it should follow the V1.54 game-bible precedent: extend `BlockType` as a new wire variant with a corresponding body-layer category.

**Novel profile semantics (body layer):** The V1.37 novel "seven categories" (`foundation`, `background`, `character`, `location`, `society`, `rules`, `economy`) are carried in `KeyBlock.body.attributes.novel_category` (string) plus type-specific fields in `body.attributes` / `body.summary`. They do **not** replace wire `block_type`.

**V1.40 P1 implementation:** `nexus-kb::validation` module provides `validate_body(block_type, body, ValidationMode)` that enforces `novel_category` presence and validity when `ValidationMode::Novel` is active. Both `InMemoryKbStore` and `SqliteKbStore` run validation on insert/update. Validation errors are structured (`ValidationKind` enum) so callers can produce precise diagnostics without string matching. `canonical_name` is validated for format/safety (no control chars, path separators, shell metacharacters, max 256 chars). Advisory warnings for `novel_category` ↔ `block_type` mismatch are emitted via `tracing::warn!`. See `crates/nexus-kb/src/validation.rs`.

**`canonical_name` grammar (V1.40 P1):** `[^\x00-\x1F\x7F/\\`$;&|><!*?"'(){}\[\]#]{1,256}` — non-empty, no control characters, no path separators, no shell metacharacters, max 256 chars.

Recommended default mapping when ingesting or authoring novel items (P1 validation may require `novel_category` when `profile_hint=novel`):

| `novel_category` (body) | Default wire `block_type` | Narrative use |
| --- | --- | --- |
| `foundation` | `info_point` | World axioms, cosmology, genre promises |
| `background` | `event` | Historical / legendary context |
| `character` | `character` | Named characters |
| `location` | `scene` | Places and settings |
| `society` | `organization` | Factions, cultures, institutions |
| `rules` | `conflict` | Magic/tech/legal constraints |
| `economy` | `item` | Trade, currency, resources |

`world_refs` in novel chapter frontmatter reference **stable `canonical_name`** values (optionally prefixed in prose as `char_lin_xia` — resolved against `(world_id, block_type, canonical_name)` via P0 validator). Iteration: [v1.40-novel-world-kb-delivery-compass-v1.md](../../iterations/v1.40-novel-world-kb-delivery-compass-v1.md); runtime layering: [world-kb-runtime-architecture.md](../world-kb-runtime-architecture.md).

Minimum common `body` shape for novel-profile items (V1.40 P1):

```json
{
  "summary": "One-line prompt descriptor",
  "attributes": {
    "novel_category": "character",
    "aliases": ["Xia"],
    "traits": ["..."]
  },
  "tags": ["novel"]
}
```

P1 adds validation helpers in `nexus-kb` for wire `BlockType` + optional `novel_category` / per-category `body.attributes` minimums. **Shipped in V1.40 P1** (`nexus-kb::validation`). No `schemas/` enum change in V1.40 unless a future ADR opts into a wire superset (out of V1.40 scope).

**Game-bible profile semantics (body layer — V1.54 P1):** Seven new `BlockType` wire enum variants are registered in `schemas/common/common.schema.json` for game-bible domain concepts. The corresponding body-layer category is `game_bible_category` (string, carried in `KeyBlock.body.attributes`). It does **not** replace wire `block_type`.

Shipped `BlockType` values (snake_case on wire) extended with:

| Wire `block_type` | UI label | `game_bible_category` | Design section |
| --- | --- | --- | --- |
| `species` | Species | `species` | `species.md` |
| `faction` | Faction | `faction` | `factions.md` |
| `magic_system` | Magic System | `magic_system` | `magic_system.md` |
| `technology` | Technology | `technology` | `technology.md` |
| `deity` | Deity | `deity` | `lore.md` |
| `level` | Level | `level` | `locations.md` |
| `economy_tier` | Economy Tier | `economy_tier` | `economy.md` |

Existing variants (`character`, `ability`, `scene`, `organization`, `item`, `conflict`, `info_point`, `event`) are reused for cross-domain concepts (e.g., a game character uses `BlockType::Character` with `game_bible_category: "character"`).

**V1.54 P1 implementation:** `nexus-kb::validation` adds `ValidationMode::GameBible` that requires `game_bible_category` in `body.attributes` when active and rejects `novel_category`. The game-bible category validation mirrors the novel pattern: seven valid categories, structured (`ValidationKind`) errors, advisory warnings on `game_bible_category` ↔ `block_type` mismatch. `canonical_name` validation is identical across all modes.

Valid `game_bible_category` values:

```
species, faction, magic_system, technology, deity, level, economy_tier
```

Default mapping from `game_bible_category` to `BlockType`:

| `game_bible_category` | Default wire `block_type` |
| --- | --- |
| `species` | `species` |
| `faction` | `faction` |
| `magic_system` | `magic_system` |
| `technology` | `technology` |
| `deity` | `deity` |
| `level` | `level` |
| `economy_tier` | `economy_tier` |

Minimum common `body` shape for game-bible items:

```json
{
  "summary": "One-line prompt descriptor",
  "attributes": {
    "game_bible_category": "faction",
    "aliases": ["The Iron Council"],
    "traits": ["..."]
  },
  "tags": ["game_bible"]
}
```

**Script profile semantics (body layer — V1.55 P3):** Three new `BlockType` wire enum variants are registered in `schemas/common/common.schema.json` for script domain concepts. The corresponding body-layer category is `script_category` (string, carried in `KeyBlock.body.attributes`). It does **not** replace wire `block_type`.

Shipped `BlockType` values (snake_case on wire) extended with:

| Wire `block_type` | UI label | `script_category` | Primary section |
| --- | --- | --- | --- |
| `dialogue` | Dialogue | `dialogue` | `Scripts/script.md` |
| `beat` | Beat | `beat` | `Beats/beat-sheet.md` |
| `act` | Act | `act` | `Beats/beat-sheet.md` |

Existing variants (`character`, `scene`, `organization`, `event`, etc.) are reused for cross-domain concepts (e.g., a script character uses `BlockType::Character` with `script_category: "dialogue"`).

**V1.55 P3 implementation:** `nexus-kb::validation` adds `ValidationMode::Script` that requires `script_category` in `body.attributes` when active and rejects `novel_category` and `game_bible_category`. The script category validation mirrors the novel/game-bible pattern: three valid categories, structured (`ValidationKind`) errors, advisory warnings on `script_category` ↔ `block_type` mismatch. `canonical_name` validation is identical across all modes.

Valid `script_category` values:

```
dialogue, beat, act
```

Default mapping from `script_category` to `BlockType`:

| `script_category` | Default wire `block_type` |
| --- | --- |
| `dialogue` | `dialogue` |
| `beat` | `beat` |
| `act` | `act` |

Minimum common `body` shape for script items:

```json
{
  "summary": "One-line prompt descriptor",
  "attributes": {
    "script_category": "dialogue",
    "characters": ["Alice", "Bob"],
    "traits": ["..."]
  },
  "tags": ["script"]
}
```

### 5.2 `nexus-knowledge` — User-scoped global knowledge

`nexus-knowledge` means user-level global knowledge/reference material:

- Its canonical scope is `User`.
- It is tag-driven and may be pulled into Moment context assembly.
- It is not Creator-scoped and does not own World KeyBlocks.

### 5.3 CLI `creator kb` — local work-scope file index

`nexus42 creator kb` is a CLI command group for local knowledge-asset workflows:

- Today, its default `work` scope is a local file/index workflow under the active `creator_id` and `workspace_slug`.
- Its current implementation is not equivalent to `nexus-kb` or `nexus-knowledge`.
- Future `--scope world` behavior must route to the World-scoped narrative KB model (`nexus-kb` + `nexus-narrative`).
- Future user/global knowledge behavior must route to `nexus-knowledge`, not to `nexus-kb`.

### 5.4 Prohibited shorthand

The term `KB` MUST be qualified in architecture/spec text when ambiguity matters:

- Use **World KB** or **narrative KB** for `nexus-kb`.
- Use **User knowledge** or **global knowledge index** for `nexus-knowledge`.
- Use **CLI local work KB index** for `nexus42 creator kb --scope work`.

### 5.5 World KB promotion state machine (V1.50 normative)

> **Status**: Normative (V1.50) — V1.50 T-B P1 shipped on 2026-06-18. Migration `202606180002_kb_extract_jobs.sql` landed; review-time extraction hook verified end-to-end; promotion row promoted Draft → Normative at V1.50 P-last.
> **Plan**: [2026-06-18-v1.50-kb-auto-promotion.md](../../plans/2026-06-18-v1.50-kb-auto-promotion.md) (Done; archived)
> **Cross-refs**: [workflow-profile.md §11.5](novel-writing/workflow-profile.md#115-auto-chronology-per-work-opt-in) — auto-advance logs auto-promotion status; [quality-loop.md §3](novel-writing/quality-loop.md) — review-time extraction hook.

World KB rows enter the World through a **promotion state machine** governed by `kb_extract_jobs.status` and the World-scoped `KeyBlocks` (`nexus-kb` storage, see §5.1.1).

#### 5.5.1 States (normative)

| State | Meaning | Visible to author via |
| --- | --- | --- |
| `manual` | Author inserted KB row directly via `creator world kb edit` (or initial scaffold) | `creator world kb list` |
| `pending` | `novel-review-master` extracted candidate; awaiting author confirm | `creator world kb pending` |
| `confirmed` | Author confirmed via `creator world kb adopt <id>`; row is now a regular `KeyBlock` | `creator world kb list` |
| `rejected` | Author dismissed via `creator world kb reject <id>`; row archived in `Logs/kb/rejected/` | `creator world kb rejected` (audit only) |

#### 5.5.2 Transitions (normative)

```text
manual   → confirmed | rejected           (initial state on direct insert; auto-marks confirmed unless `creator world kb edit --pending` requested)
pending  → confirmed | rejected           (only via CLI adopt/reject)
confirmed → (terminal; may be edited via creator world kb edit; deletion via creator world kb delete which logs to rejected)
rejected → (terminal; archived)
```

Invalid transitions return `422` with stable error code on Local API.

#### 5.5.3 Promotion gate (normative V1.50)

`pending → confirmed` requires:

- `creator world kb adopt <extract_job_id>` invocation
- Author identity matches `works.creator_id` (no cross-author adoption)
- Optional `--with-merge` flag (V1.50 T-B P1 extension) merges proposed payload into an existing KB row instead of creating a new one

#### 5.5.4 Rejected retention

Rejected promotion candidates are retained in `Logs/kb/rejected/<YYYY-MM-DD>-<extract_job_id>.md` for audit. Retention is **indefinite** by default (no TTL); future iterations may add a `--prune-rejected` CLI.

#### 5.5.5 Relationship to existing `nexus-kb` taxonomy

The promotion state machine does **not** change the `BlockType` enum (see §5.1.1 SSOT) or the `ValidationMode` constraints (see V1.40 P1 validation module). It governs **how** a row enters the World, not **what** the row contains.

#### 5.5.6 LLM extraction pathway (V1.51 T-A P0 — Normative)

V1.51 T-A P0 closes `R-V150KBED-01`. The V1.50 heuristic defaulted every
review-time candidate to `block_type_guess='character'` (capitalized noun
phrase), forcing authors to correct the type on adopt for every non-character
entity. V1.51 replaces the heuristic with the `nexus.llm.extract` capability
(see [llm-extract.md](llm-extract.md)) at the review-time extraction hook.

The state machine in §5.5.1–§5.5.2 is **unchanged** — LLM extraction only
improves the *quality* of the `block_type_guess` + `canonical_name_guess`
proposed in the `pending` row. What changes:

| Column / field | V1.50 heuristic | V1.51 LLM pathway |
| --- | --- | --- |
| `block_type_guess` | always `character` | LLM-judged wire `BlockType` |
| `canonical_name_guess` | matched phrase as-is | LLM-extracted canonical name |
| `llm_confidence` (new) | `NULL` | LLM self-reported `0.0`–`1.0` |
| `llm_source_quote` (new) | `NULL` | verbatim chapter excerpt |
| `proposed_payload.tags` | `["novel","heuristic-extracted"]` | `["novel","llm-extracted"]` |

The promotion gate (§5.5.3), the `confirmed`/`rejected` terminal states
(§5.5.2), and the `ValidationMode::Novel` re-run on adopt (§5.5.5) are all
unchanged — the LLM only affects what the author *sees* on a `pending` row, not
how it transitions. When the LLM worker is unavailable, the hook falls back to
the V1.50 heuristic so no-worker environments (hermetic tests,
daemon-without-worker) remain functional.

#### 5.5.7 Work→KeyBlock provenance linkage (Draft V1.52 overlay)

**Status**: Draft (V1.52 — body authored in plan `2026-06-19-v1.52-work-keyblock-provenance-and-essay-profile`)  
**Authoring plan**: `2026-06-19-v1.52-work-keyblock-provenance-and-essay-profile`  
**Promotes to Normative**: P-last of V1.52

##### 5.5.7.1 Purpose

When a `pending` KB candidate is promoted to `confirmed` via `creator world kb adopt`, the source Work that produced the extraction and the extraction pathway are recorded in `kb_key_blocks`. This closes the ownership ambiguity flagged in R-V150KBED-02: KB authorization gates can now prefer the source Work's author rather than relying only on World ownership.

##### 5.5.7.2 Schema extension

Three nullable columns added to `kb_key_blocks`:

| Column | Type | Description |
|--------|------|-------------|
| `source_work_id` | TEXT (nullable) | FK to `works.work_id` — the Work that produced this KB row |
| `source_chapter` | INTEGER (nullable) | Chapter number where the entity was extracted (novel only) |
| `source_provenance_kind` | TEXT (nullable, CHECK) | Enum describing how the row entered the KB graph |

`source_provenance_kind` permitted values:

| Value | Meaning |
|-------|---------|
| `manual` | Author directly added via `creator world kb add` or initial scaffold |
| `review_time_extract` | Review-time extraction hook (V1.50 heuristic or V1.51 LLM) |
| `finalize_time_extract` | Finalize-time extraction (novel finalize hook) |
| `cross_chapter_rescan` | V1.51 T-A P1 cross-chapter reconciliation |
| `author_explicit` | Author explicitly confirmed/adopted a pending candidate |

All three columns default to `NULL`; existing rows are not backfilled. Rows without provenance (legacy, manual entry) continue to resolve ownership via `narrative_worlds.owner_creator_id`.

##### 5.5.7.3 Author gate interaction

When a KeyBlock carries `source_work_id`, the authorization check for edit/delete operations prefers the Work author:

1. If `source_work_id` is present → verify `works.creator_id` matches the active Creator
2. Otherwise → fall back to `narrative_worlds.owner_creator_id` (existing behavior, unchanged)

This is narrow: the gate only changes when provenance is present. It does not alter World-scoped uniqueness, `ValidationMode`, or the promotion state machine (§5.5.1–§5.5.2).

##### 5.5.7.4 Adopt-flow provenance population

On `creator world kb adopt <extract_job_id>`:

- `source_work_id` is resolved from the extract job's source work context (`kb_extract_jobs.source_work_id` if available, or the linked Work via the extract job's artifact locator)
- `source_chapter` is copied from `kb_extract_jobs.source_chapter_id`
- `source_provenance_kind` is inferred from the extract job's origin:
  - Jobs created by the review-time extraction hook → `review_time_extract`
  - Jobs created by the finalize-time extraction hook → `finalize_time_extract`
  - Jobs created by `creator world kb add` → `manual`
  - Jobs promoted via `adopt --auto` → `author_explicit`

On `creator world kb adopt --auto <world_ref>`: same logic applied to each auto-promoted candidate individually.

### 5.6 World KB relationship semantics (V1.74 normative)

> **Status**: Normative — V1.74 Shipped β. This section defines first-class inter-entity relationships in the World KB graph. The V1.50–V1.73 model defined KeyBlocks, SourceAnchors, and the promotion state machine; V1.74 adds typed edges between confirmed/non-deleted World KB entities.

World KB relationships are World-scoped graph edges owned by the same World as their source and target KeyBlocks. A relationship row does not transfer ownership, change the KeyBlock lifecycle, or replace SourceAnchor provenance; it records author-visible semantic linkage between two World KB entities.

#### 5.6.1 Storage and identity

- A relationship is stored as one `kb_relationships` row with `relationship_id`, `world_id`, `source_entity_id`, `target_entity_id`, `relation_type`, optional `custom_label`, `symmetric`, optional `confidence`, optional `source_anchor_ids`, optional `metadata`, timestamps, and `revision`.
- `source_entity_id` and `target_entity_id` MUST refer to non-deleted `kb_key_blocks.key_block_id` rows in the same `world_id`.
- Self-loops are prohibited: `source_entity_id == target_entity_id` is invalid and MUST be rejected as a validation error.

#### 5.6.2 Directed + symmetric model

Relationships are stored as directed edges. The `symmetric` flag marks a directed row whose author-visible meaning should be projected in both directions. Implementations MUST NOT store a second reverse row for symmetry. On canonical graph reads, a symmetric row emits both the stored projection and a derived reverse projection that share the same `relationship_id`; editing or deleting either projection targets the single stored row.

#### 5.6.3 Taxonomy

The wire taxonomy is the `WorldKbRelationshipKind` enum. V1.74 core values are: `allied_with`, `opposes`, `parent_of`, `child_of`, `member_of`, `located_in`, `rules_over`, `references`, `serves`, `rival_of`, `mentor_of`, and `custom`. Wire values are snake_case. When `relation_type = custom`, `custom_label` is required and carries the author's narrative label. When `relation_type` is any core enum value, `custom_label` is ignored or normalized to absent.

#### 5.6.4 Source-anchor grounding

Relationship grounding is optional. `source_anchor_ids` may be empty for author-asserted worldbuilding relationships that are true in the author's model but not yet grounded in extracted text. Non-empty `source_anchor_ids` indicate grounding and MUST resolve to source-anchor projections in the same World. UI may display “author-asserted” for empty grounding and “N anchors” for grounded relationships.

#### 5.6.5 Confidence

`confidence` is optional and display-only in V1.74. It may be author-set or future-extraction-set, but it MUST NOT drive validation, graph layout weighting, filtering, or conflict resolution in this iteration. Values, when present, are in the inclusive range `0.0..=1.0`.

#### 5.6.6 Optimistic concurrency

Relationship writes use per-row OCC through `kb_relationships.revision`. Canonical reads expose the current row version; mutating requests submit `expected_version`. A stale `expected_version` MUST return a conflict before mutation. Successful updates increment the stored `revision`; deletes compare against the expected revision before removing the row.

#### 5.6.7 Extraction-sourced suggested relationships (V1.76 γ)

`kb_relationships.needs_review = 1` marks an edge as a **suggestion** proposed by
extraction (`nexus.llm.extract`) and not yet author-confirmed; it is not part of
the default confirmed graph. `source = 'extraction'` marks extraction provenance
while `source = 'manual'` marks author-created rows. Extraction-sourced rows
remain normal `kb_relationships` rows with OCC, symmetric projection, confidence
validation, and optional grounding; **promotion is clearing `needs_review`**,
not moving through a pending/confirmed/rejected relationship state machine. A
full relationship promotion state machine (mirroring entities) is post-1.0.

**Entity-existence prerequisite**: extraction persists a relationship suggestion
only when both endpoints already resolve to non-deleted KeyBlocks in the same
World. A candidate whose endpoints are missing or ambiguous is skipped + logged
and may be re-proposed on a later rescan after the author promotes the endpoint
entities. Review-time extraction does NOT confirm entity candidates in the same
pass, so relationships involving newly suggested entities are deferred.

The GET graph defaults to excluding `needs_review` rows; `?include_suggested=true`
surfaces them. The "extraction suggests, author decides" split (§5.5) is extended
from entity candidates to relationships without cloning the entity promotion
state machine.

---

## 6. Scope transition rules

### 6.1 Ownership does not move by reference

Cross-scope references do not transfer ownership. A lower-scope entity may reference
an ancestor ID, and an ancestor may index descendants, but the canonical owner remains
the entity's declared scope.

### 6.2 User knowledge → Moment

`nexus-moment-context-assembly` may read user-scoped knowledge from
`nexus-knowledge` and include selected slices in a Moment snapshot. This produces a
read-only assembled context view; it does not move the underlying knowledge out of
User scope.

### 6.3 User knowledge → World KB

Promoting User knowledge into a World creates or updates a World-scoped narrative KB
asset through `nexus-kb`/`nexus-narrative`. The promoted World KB asset MUST carry
source/anchor provenance back to the User knowledge material when available. The
source material remains User-scoped.

### 6.4 Creator memory → Moment

Creator SOUL and long-term memory may be selected into a Moment snapshot. The Moment
snapshot is derived context; it does not mutate the Creator memory store.

### 6.5 World/Timeline/Event → Moment

A Moment is created or assembled at an Event boundary. Moment assembly may read World
state, Timeline recency, Event details, and World KB assets. The Moment MUST be treated
as a session-start snapshot, not as the owner of narrative state.

### 6.6 Timeline and history changes

Timeline/Event history is immutable for canonical narrative purposes. Corrections,
alternate histories, or rewrites use Fork semantics through `nexus-narrative`; they do
not mutate prior Event ownership in place.

**World fork / community branching is platform scope.** The local `nexus42` CLI and daemon do not implement fork creation, fork listing, or fork merge commands. `World::fork` in `nexus-narrative` may carry the domain model for platform sync, but no local product surface exposes fork operations. See planning decision PD-01 in the deferred-features tracker.

### 6.7 Creator/User pairing

Pairing links a Creator to a User for account and platform visibility. Pairing does not
make `User` the operational identity for agent execution. Operational actor fields for
daemon/orchestration remain Creator-oriented (`creator_id`) unless a User/Pairing cloud
operation is explicitly being performed.

### 6.8 Cloud transport boundary

Cloud transport may synchronize or fetch representations of multiple scopes, but it
does not own their domain invariants. In particular:

- User/Pairing invariants go through `nexus-cloud-domain`.
- Narrative World/Timeline/Event/KB invariants go through `nexus-narrative` and `nexus-kb`.
- Moment assembly remains `nexus-moment-context-assembly`; optional cloud Stage-1 is an input source, not a replacement owner.

---

## 7. Wiring implications for V1.23

1. `nexus-moment-context-assembly` target wiring should aggregate local `nexus-creator-memory`, `nexus-narrative`, `nexus-kb`, and `nexus-knowledge` inputs for Stage-0 moment context.
2. `nexus-narrative` is the core entry point for creative-work narrative state and should be the natural owner for World/Timeline/Event queries consumed by context assembly and CLI/daemon local APIs.
3. `nexus-knowledge` must be described and wired as User-scoped global knowledge, not Creator-scoped knowledge.
4. `nexus-cloud-sync` must depend on and use `nexus-cloud-domain` for User/Pairing invariants.
5. `nexus-kb` should become the key module for World-scoped KB graph insertion and query, with `nexus-narrative` owning the surrounding narrative aggregates.
