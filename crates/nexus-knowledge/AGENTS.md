# nexus-knowledge — Knowledge entries (World + User) + Reference Sources

`nexus-knowledge` owns the **World KnowledgeEntry** domain (merged from the
former `nexus-kb` in V1.139 P1 T1) alongside **User-scoped global knowledge**
and **local reference-source** domain types. After the V1.139 merger it
consolidates three knowledge tiers in one crate.

## Module layout

| Module | Domain | Scope |
|--------|--------|-------|
| `world_kb` | World KB — `WorldKbEntry` + `SourceAnchor`, `KbStore`, KB extraction/sync/query/validation. `WorldKbEntry` converts to/from spoke `KnowledgeEntry` at the wire boundary. | World entity (narrative KB) |
| `knowledge` | User knowledge — `UserKnowledgeEntry` (tag-driven, indexed per `user_id`), selectable by Moment context assembly | User entity |
| `reference_source` | Reference sources — local-only research/reference registration | Creator / workspace |
| `store` | `KnowledgeStore` abstraction + `InMemoryKnowledgeStore` (User knowledge) | trait / test impl |
| `errors` | `KnowledgeError` (User knowledge errors) | — |

> **World KB scope inversion (V1.139):** the previous `nexus-knowledge` AGENTS.md
> stated "It is not Creator-scoped and does not own World/narrative KeyBlocks
> (those live in `nexus-kb`)." This is now **inverted** — the crate **does** own
> the World KB domain (the former `nexus-kb` `KeyBlock` aggregate, renamed to
> `WorldKbEntry` in V1.139 P1 T2), relocated under `world_kb/`.
> `crates/nexus-kb/` no longer exists.

## Wire boundary: `WorldKbEntry` ↔ spoke `KnowledgeEntry` (conversion seam)

Per spec `spoke-adapter-architecture.md` §7.1, `WorldKbEntry` is the **nexus
domain aggregate**; `spoke_schemas::KnowledgeEntry` is the **wire/standard
boundary type**. The two `From` impls in `world_kb/knowledge_entry.rs` are the
**sole conversion seam** (re-exported from `world_kb`).

- **Call-boundary invariant (HARD):** `spoke-operations` functions receive the
  **converted spoke type only** — never `WorldKbEntry`. Convert first
  (`spoke_schemas::KnowledgeEntry::from(world_kb_entry)`), then delegate via
  `nexus-spoke-adapter` (T3 wires the lifecycle delegation).
- **Q13 (prefer spoke):** `WorldKbEntry` carries the nexus-local **body** content
  (`summary`/`attributes`/`tags`/`state`/`computable`) that spoke deliberately
  keeps product-local. Identity / status / extensions map to/from spoke on
  conversion; they are **not** an independently-authored parallel model.
- **Identity → `extensions.nexus`:** `world_id`, `created_from_command_id`, and
  the three provenance fields ride in `extensions.nexus` on the spoke type, via
  the `nexus-spoke-adapter::extensions::{get_*, set_*}` accessors.
- **Body fidelity (temporary):** spoke's `KnowledgeEntryBody` exposes only
  `computable`/`state` typed maps today (typify drops `additionalProperties`),
  so the forward conversion maps `state` only; `summary`/`attributes`/`tags`
  stay on `WorldKbEntry` (spoke-operations does not consume them). When spoke
  later declares body fields, **extend only the two `From` impls** — the
  validation engine and all consumers are unaffected (minimal future delta).
- **Name collision resolved (R-V1139P0-004):** the User-scoped struct is
  `UserKnowledgeEntry` (in `knowledge`); the World KB spoke boundary type is
  `spoke_schemas::KnowledgeEntry` (re-exported from `world_kb`).

## Key Rules

- **Contracts-first**: use `nexus-contracts` for retained nexus-local enums
  (`BlockType`, `KeyBlockStatus`) and `nexus-contracts` daemon-api envelopes.
  World KB wire identity is the spoke `KnowledgeEntry` (via the conversion seam).
- **Scope clarity**: qualify "knowledge" as World KB (`world_kb` /
  `WorldKbEntry`) vs User knowledge (`knowledge` / `UserKnowledgeEntry`) when
  ambiguity matters; do not use this crate for Creator memory semantics.
- **Lifecycle delegation boundary**: spoke-provided standard lifecycle
  invariants (promote gate, status transitions, extension merge) live in
  `nexus-spoke-adapter`, **not** in this crate. This crate owns domain types,
  the `KbStore` / `KnowledgeStore` traits, and the conversion seam only.
- **Persistence boundary (DF-43)**: `nexus-local-db` is the sole production
  SQLite persistence owner (both for User knowledge via
  `SqliteKnowledgeStore` and for World KB via its `kb_store`). This crate
  provides domain types, traits, and `InMemory*` test stores only. Do not add a
  second SQLite connection, file-backed store, or migration path in this crate.

## Dependencies

- `nexus-contracts` (retained `BlockType` / `KeyBlockStatus` enums + daemon-api envelopes)
- `nexus-spoke-adapter` (extensions.nexus accessors + ops delegation facade)
- `spoke-schemas` (standard `KnowledgeEntry` wire type)
