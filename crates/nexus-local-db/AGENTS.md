# nexus-local-db — Local Database Layer

SQLite-backed persistence using sqlx. Used by the daemon runtime and `nexus-orchestration`.

## Migrations

Migration files under `migrations/` are committed — the schema is fully reproducible. **sqlx orders migrations numerically** by the leading version prefix (the digits before the first `_` or non-digit separator).

Common filename shapes in this crate:

- `YYYYMMDD_<description>.sql` — e.g. `20260812_drop_legacy_outbox.sql`
- `YYYYMMDD_<seq>_<description>.sql` — e.g. `20260701_000001_memory_fragments_world_and_soul_narratives.sql`
- **14-digit** `YYYYMMDDHHMMSS_<description>.sql` — e.g. `20260815000001_create_world_findings.sql`

Use a **14-digit prefix** when the migration must run **after** existing 14-digit versions. A shorter `YYYYMMDD` prefix sorts before them (e.g. `20260905` before `20260815000001`) and can run rebuild migrations in the wrong order, dropping columns that later migrations still need. When ordering is sensitive, document the dependency in the migration header (see `20260905000002_actor_knowledge_owners.sql`).

## Key Rules

- **Compile-time checked queries only** — use `sqlx::query!()` / `sqlx::query_as!()` for all static SQL. Runtime `sqlx::query()` only for DDL, PRAGMAs, or truly dynamic SQL with a `// SAFETY:` comment.
- See [`crates/nexus-daemon-runtime/AGENTS.md`](../nexus-daemon-runtime/AGENTS.md) for full sqlx compile-time macro rules and `.sqlx/` commit conventions.
- Do not add local sqlx features beyond what the workspace declares.
- **Pure storage (V1.145 P1b):** this crate is storage-only — DB CRUD primitives (`SqliteKbStore`, `open_pool`, `run_migrations`, CAS helpers). The production adapter (`NexusAdapter`, V1.146 rename) + 6 spoke port impls **moved to `nexus-spoke-adapter/src/adapter/`** (spec §7.4 / §8). `nexus-local-db` has **no `nexus-spoke-adapter` dependency**; the `extensions.nexus` round-trip helpers (`build_extensions_nexus`, `is_known_nexus_key`) are inlined as private local fns in `kb_store.rs` so the legacy INSERT/UPDATE wrappers stay spoke-unaware. See `.mstar/specs/spoke-adapter-architecture.md` §7.4 for the family matrix (production vs stub).
- **Timeline ordering (V1.146 P1):** the former `SqliteNarrativeGateway::get_timeline_ordered` (which called the spoke `order_timeline_events_by_ids` helper via a direct `spoke-operations` dep) was **removed** — it never had a production call site, and the ordered-timeline facet now lives on the spoke-adapter boundary as `NexusAdapter::list_timeline_events_ordered` in `nexus-spoke-adapter` (spec §7.4). `nexus-local-db` no longer depends on `spoke-operations`. See `.mstar/specs/spoke-adapter-architecture.md` §7.4 "Read-path ScopeQuery adoption".

## Holder governance storage (v1.191 P1 — shipped)

This crate owns the storage half of the holder contract
([holder-governance.md](../../.mstar/specs/holder-governance.md) §§2–6); policy stays
in `nexus-core`:

- **Registry.** `src/holders.rs` owns `knowledge_holders`: one row per stored Creator
  or Character (`ensure_creator_holder_in_tx` / `ensure_character_holder_in_tx` commit
  the subject and its holder in one transaction; `resolve_holder` is the read-only
  lookup, which re-derives the id and refuses a corrupt or mismatched row with
  `holder_state_invalid`). Ids are `hld_` + the full lowercase BLAKE3 digest of the
  `nexus-holder-v1\0<kind>\0<subject_id>` bytes, so workspace materializations of the
  same global Creator resolve to the same holder. An absent registry row is never
  provisioned by a normal read.
- **Governance columns.** `kb_key_blocks` gains indexed nullable `holder_entry_id`
  (FK to the registry, `ON DELETE RESTRICT`) and `disclosure`; eligibility predicates
  (`selection_visibility_conjunct`) run **before** keyset/LIMIT/count/rank in
  `actor_knowledge_store.rs` / `kb_store.rs`. `reject_legacy_creator_only_extension`
  refuses the retired `extensions.nexus.creator_only` key (including `false`) on every
  opaque write path, and the record carrier is written as a constant `false`.
- **Scoped by-id reads.** `src/read_scope.rs` derives an admitted selection's owned
  containers (`owned_world_containers`, `creator_actor_view_scope`) and
  `kb_store.rs` exposes the scoped by-id/id-set reads (`list_by_world_scoped`,
  `list_entries_by_ids_admitted`, the scoped `get_*`) used by the compute/snapshot
  consumers, so a foreign or missing holder fails closed instead of falling back to
  an unfiltered World query.
- **Import quarantine.** `knowledge_import_quarantine` holds unresolved/unknown
  governance outside ordinary KB stores, search, MCA, compute, export and Connect.
- **Offline cutover.** Migration `20260918000001_holder_registry.sql` (schema version
  23 → 24) is one ordered, offline, quiesced transaction: preflight (stored World
  controlling Creator, archived identities, legacy bool, malformed extensions,
  WorldSheet legality, native collisions) fails before any mutation, shared rows stay
  `NULL/NULL`, every `creator_only = true` World row becomes its stored World Creator
  holder + `owner-private`, the legacy column and extension are dropped, and
  visibility/FK counts are validated inside the transaction. Rollback is the matching
  old binary plus the complete pre-migration backup — never an in-place reverse
  migration. New persistent tables (registry, quarantine, the ke-extract job tables)
  are registered with the existing writer guards in `writer_protocol.rs`.

## Waived Residuals

<!-- WAIVER: pre-1.0 local-first; see V1.41 P-last residual R-V140P0-S3 — sqlx compile-time cache not shared across crates; acceptable until CI enforce single cache -->
<!-- WAIVER: pre-1.0 local-first; see V1.41 P-last residual R-V140P1-S4 — local-db-schema.md not authored; schema is self-documenting via migrations -->
