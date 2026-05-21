# nexus-knowledge — User Knowledge + Reference Sources

`nexus-knowledge` owns User-scoped global knowledge and local reference-source
domain types. It is not Creator-scoped and does not own World/narrative
KeyBlocks (those live in `nexus-kb`).

The crate currently provides:

- `knowledge`: tag-driven global knowledge entries indexed per `user_id`, for
  material that Moment context assembly may select.
- `reference_source`: local-only research/reference registration types.
- `store`: `KnowledgeStore` abstraction plus `InMemoryKnowledgeStore` for tests
  and prototypes; SQLite-backed persistence is future work.

## Key Rules

- **Contracts-first**: use `nexus-contracts` for shared wire/local types. Do not
  hand-write duplicate DTOs.
- **Scope clarity**: qualify “knowledge” as User knowledge when ambiguity matters;
  do not use this crate for World KB or Creator memory semantics.

## Dependencies

- `nexus-contracts` (generated types)
