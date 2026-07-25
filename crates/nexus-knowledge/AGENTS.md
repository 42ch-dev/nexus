# nexus-knowledge — Knowledge entries (World + User) + Reference Sources

`nexus-knowledge` owns the **World KnowledgeEntry** domain (merged from the
former `nexus-kb` in V1.139 P1 T1) alongside **User-scoped global knowledge**
and **local reference-source** domain types. After the V1.139 merger it
consolidates three knowledge tiers in one crate.

## Module layout

| Module | Domain | Scope |
|--------|--------|-------|
| `world_kb` | World KnowledgeEntry — `KeyBlock` + `SourceAnchor`, `KbStore`, KB extraction/sync/query/validation | World entity (narrative KB) |
| `knowledge` | User knowledge — tag-driven global knowledge entries indexed per `user_id`, selectable by Moment context assembly | User entity |
| `reference_source` | Reference sources — local-only research/reference registration | Creator / workspace |
| `store` | `KnowledgeStore` abstraction + `InMemoryKnowledgeStore` (User knowledge) | trait / test impl |
| `errors` | `KnowledgeError` (User knowledge errors) | — |

> **World KB scope inversion (V1.139):** the previous `nexus-knowledge` AGENTS.md
> stated "It is not Creator-scoped and does not own World/narrative KeyBlocks
> (those live in `nexus-kb`)." This is now **inverted** — the crate **does** own
> the World KnowledgeEntry domain (formerly `nexus-kb`'s `KeyBlock` aggregate),
> relocated under `world_kb/`. `crates/nexus-kb/` no longer exists.

## Key Rules

- **Contracts-first**: use `nexus-contracts` for shared wire/local types. Do not
  hand-write duplicate DTOs. (V1.139 T2/T3 will migrate World KB types onto the
  spoke `KnowledgeEntry` standard object via `nexus-spoke-adapter`.)
- **Scope clarity**: qualify "knowledge" as World KB (`world_kb`) vs User
  knowledge (`knowledge`) when ambiguity matters; do not use this crate for
  Creator memory semantics.
- **Lifecycle delegation boundary**: spoke-provided standard lifecycle
  invariants (promote gate, status transitions, extension merge) live in
  `nexus-spoke-adapter`, **not** in this crate. This crate owns domain types,
  the `KbStore` / `KnowledgeStore` traits, and adapter seams only.
- **Persistence boundary (DF-43)**: `nexus-local-db` is the sole production
  SQLite persistence owner (both for User knowledge via
  `SqliteKnowledgeStore` and for World KB via its `kb_store`). This crate
  provides domain types, traits, and `InMemory*` test stores only. Do not add a
  second SQLite connection, file-backed store, or migration path in this crate.

## Dependencies

- `nexus-contracts` (generated types)
- `nexus-spoke-adapter` (V1.139 — brought in via the nexus-kb merger; the
  relocated `world_kb` modules migrate onto spoke types in T2/T3)
- `spoke-schemas` (V1.139 — spoke standard object schemas)
