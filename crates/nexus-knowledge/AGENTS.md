# nexus-knowledge — Knowledge entries (World + User) + Reference Sources

`nexus-knowledge` owns the **World KnowledgeEntry** domain (merged from the
former `nexus-kb` in V1.139 P1 T1) alongside **User-scoped global knowledge**
and **local reference-source** domain types. After the V1.139 merger it
consolidates three knowledge tiers in one crate.

## Module layout

| Module | Domain | Scope |
|--------|--------|-------|
| `world_kb` | World KB — `KnowledgeEntryRecord` + `SourceAnchor`, `KbStore`, KB extraction/sync/query/validation. `KnowledgeEntryRecord` converts to/from spoke `KnowledgeEntry` at the wire boundary. | World entity (narrative KB) |
| `knowledge` | User knowledge — `UserKnowledgeEntry` (tag-driven, indexed per `user_id`), selectable by Moment context assembly | User entity |
| `reference_source` | Reference sources — local-only research/reference registration | Creator / workspace |
| `store` | `KnowledgeStore` abstraction + `InMemoryKnowledgeStore` (User knowledge) | trait / test impl |
| `errors` | `KnowledgeError` (User knowledge errors) | — |

> **World KB scope inversion (V1.139):** the previous `nexus-knowledge` AGENTS.md
> stated "It is not Creator-scoped and does not own World/narrative KeyBlocks
> (those live in `nexus-kb`)." This is now **inverted** — the crate **does** own
> the World KB domain (the former `nexus-kb` `KeyBlock` aggregate, renamed to
> `WorldKbEntry` in V1.139 P1 T2, then generalized to the owner-aware
> `KnowledgeEntryRecord` in v1.184 P1), relocated under `world_kb/`.
> `crates/nexus-kb/` no longer exists.

## Wire boundary: `KnowledgeEntryRecord` ↔ spoke `KnowledgeEntry` (conversion seam)

Per spec `spoke-adapter-architecture.md` §7.1, `KnowledgeEntryRecord` is the **nexus
domain aggregate** (the v1.184 P1 owner-aware generalization of `WorldKbEntry`);
`spoke_schemas::KnowledgeEntry` is the **wire/standard
boundary type**. Since V1.145 P1a the **sole conversion seam** lives in
`nexus-spoke-adapter::conversion` as the free functions `knowledge_record_to_spoke` /
`spoke_to_knowledge_record` (orphan rule forbids the `From` impls here — both types are
foreign to this crate) plus the `KnowledgeEntryRecordSpokeExt` lifecycle trait; this
crate owns the pure domain aggregate only. `world_kb/mod.rs` still re-exports
the spoke `KnowledgeEntry` wire type for consumers that read it through this crate.

- **Call-boundary invariant (HARD):** `spoke-operations` functions receive the
  **converted spoke type only** — never `KnowledgeEntryRecord`. Convert first
  (`nexus_spoke_adapter::conversion::knowledge_record_to_spoke(&entry)`), then delegate
  via `nexus-spoke-adapter`.
- **Q13 (prefer spoke):** `KnowledgeEntryRecord` carries the nexus-local **body** content
  (`summary`/`attributes`/`tags`/`state`/`computable`) that spoke deliberately
  keeps product-local. Identity / status / extensions map to/from spoke on
  conversion; they are **not** an independently-authored parallel model.
- **Identity → `extensions.nexus`:** the owner keys (`world_id` / `character_id` /
  `actor_world_binding_id`, exactly one per row), `created_from_command_id`, and the
  provenance fields ride in `extensions.nexus` on the spoke type, via the
  `nexus-spoke-adapter::extensions::{get_*, set_*}` accessors. Holder governance is
  **not** an extension key: the native `holder_entry_id` / `disclosure` columns map to
  the spoke core `KnowledgeEntry.owner` / `.disclosure` fields, and the retired
  `creator_only` key is refused (`legacy_creator_only_unsupported`).
- **Body fidelity:** spoke's body is a closed five-field set
  (`summary`/`attributes`/`tags`/`state`/`computable`); the forward conversion
  maps all five, and the reverse restores the lossless nexus body carrier when
  present (V1.143) before the typed fields (spoke's typed attribute values drop
  null/array/object shapes). When spoke later extends the body, **extend only the
  two free functions in `nexus-spoke-adapter::conversion`** — the validation
  engine and all consumers are unaffected (minimal future delta).
- **Name collision resolved (R-V1139P0-004):** the User-scoped struct is
  `UserKnowledgeEntry` (in `knowledge`); the World KB spoke boundary type is
  `spoke_schemas::KnowledgeEntry` (re-exported from `world_kb`).

## Holder governance and read selection (v1.191 P1 — shipped)

The domain record and the store traits own the native holder contract; authority
stays in `nexus-core` admission, never in a client value (durable
[holder-governance.md](../../.mstar/specs/holder-governance.md) §§1–5):

- **Native pair.** `KnowledgeEntryRecord.holder_entry_id: Option<String>` /
  `.disclosure: Option<String>` carry the resolved holder and its disclosure
  (`"owner-private"` only; shared is the *absence* of disclosure, never the string
  `"shared"`). `owner: KnowledgeOwnerRef` keeps its closed World / Character /
  ActorWorldBinding container meaning — it is never a holder.
- **Closed audience.** `KnowledgeAudience` (`shared` / `author-only` /
  `character-private{character_id}`) is resolved by `resolve_authored_governance`:
  omitted create ⇒ shared, omitted patch ⇒ preserve the stored pair, explicit
  `shared` ⇒ clear both. `validate_native_governance` is the closed-space check
  (`(None,None)` / `(holder,None)` / `(holder,owner-private)`).
- **Reserved raw keys.** `reject_reserved_authoring_keys` refuses `creator_only`
  **by presence, including `false`** (stable reason `legacy_creator_only_unsupported`)
  and refuses client-authored `holder_entry_id` / `disclosure`; the authoring
  surface is `audience` only.
- **Ordinary writes cannot transfer governance.** `KbStoreError::ImmutableGovernance`
  rejects a governance move through `update_knowledge_entry`; the in-memory store
  validates the native pair on insert and update paths, so it enforces the same
  invariant the SQLite columns + registry FK own.
- **Read selection.** `KnowledgeReadScope` / `KnowledgeReadPolicy`
  (`CreatorManagement` vs `ActorView`) is a non-`Serialize`, non-`Default`,
  private-field type with no `nexus-core` dependency; constructing an `ActorView`
  requires a nonempty holder. Core builds it from admitted context and the
  adapter/storage validate the stored subject/container tuple — a client value is
  never authority.
- **Extraction split.** `world_kb/extract_finalize.rs` separates *prepare*
  (validate the body, allocate the candidate id once, attach source + trusted
  governance) from *persist-prepared* (no new id, no default governance), so the
  production extraction callers persist exactly the ids and governance the
  protocol returned.

## Key Rules

- **Contracts-first**: use `nexus-contracts` for retained nexus-local enums
  (`BlockType`, `KeyBlockStatus`) and `nexus-contracts` daemon-api envelopes.
  World KB wire identity is the spoke `KnowledgeEntry` (via the conversion seam).
- **Scope clarity**: qualify "knowledge" as World KB (`world_kb` /
  `KnowledgeEntryRecord`) vs User knowledge (`knowledge` / `UserKnowledgeEntry`) when
  ambiguity matters; do not use this crate for Creator memory semantics.
- **Lifecycle delegation boundary**: spoke-provided standard lifecycle
  invariants (promote gate, status transitions, extension merge) **and the
  `KnowledgeEntryRecord ↔ KnowledgeEntry` conversion seam** live in
  `nexus-spoke-adapter` (since V1.145 P1a — dep-graph reversal, spec §8), **not**
  in this crate. This crate owns domain types and the `KbStore` /
  `KnowledgeStore` traits only.
- **Persistence boundary (DF-43)**: `nexus-local-db` is the sole production
  SQLite persistence owner (both for User knowledge via
  `SqliteKnowledgeStore` and for World KB via its `kb_store`). This crate
  provides domain types, traits, and `InMemory*` test stores only. Do not add a
  second SQLite connection, file-backed store, or migration path in this crate.

## Dependencies

- `nexus-contracts` (retained `BlockType` / `KeyBlockStatus` enums + daemon-api envelopes)
- `spoke-schemas` (standard `KnowledgeEntry` wire type — re-exported from `world_kb`)

> Since V1.145 P1a this crate **no longer depends on `nexus-spoke-adapter`**.
> The conversion seam + lifecycle delegation moved there, reversing the former
> edge to `nexus-spoke-adapter → nexus-knowledge` (spec §8).
