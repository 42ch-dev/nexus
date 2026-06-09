# Nexus Entity Scope Model

## 0. Document position

| Attribute | Value |
| --- | --- |
| **Status** | Active — normative SSOT for entity scope hierarchy, uniqueness, and crate ownership |
| **Document class** | Master |
| **Scope** | Global/User/Creator/World/Timeline/Event/Moment hierarchy; entity ownership; `kb`/`knowledge` naming boundaries; scope transition rules |
| **Last updated** | 2026-06-08 — V1.37 P2 World KB narrative taxonomy roadmap |
| **Related** | [local-cloud-crate-architecture.md](./local-cloud-crate-architecture.md), [cli-spec.md](./cli-spec.md), [daemon-runtime.md](./daemon-runtime.md), [orchestration-engine.md](./orchestration-engine.md), [local-db-schema.md](./local-db-schema.md), [`docs/ARCHITECTURE.md`](../../../docs/ARCHITECTURE.md) |

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
