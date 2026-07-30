# nexus-local-db — Local Database Layer

SQLite-backed persistence using sqlx. Used by the daemon runtime and `nexus-orchestration`.

## Migrations

Migration files under `migrations/` use naming convention `YYYYMMDD_<description>.sql`. All migrations are committed — the schema is fully reproducible.

## Key Rules

- **Compile-time checked queries only** — use `sqlx::query!()` / `sqlx::query_as!()` for all static SQL. Runtime `sqlx::query()` only for DDL, PRAGMAs, or truly dynamic SQL with a `// SAFETY:` comment.
- See [`crates/nexus-daemon-runtime/AGENTS.md`](../nexus-daemon-runtime/AGENTS.md) for full sqlx compile-time macro rules and `.sqlx/` commit conventions.
- Do not add local sqlx features beyond what the workspace declares.
- **Pure storage (V1.145 P1b):** this crate is storage-only — DB CRUD primitives (`SqliteKbStore`, `open_pool`, `run_migrations`, CAS helpers). The production adapter (`NexusAdapter`, formerly `NexusBaselineAdapter`) + 6 spoke port impls **moved to `nexus-spoke-adapter/src/adapter/`** (spec §7.4 / §8). `nexus-local-db` has **no `nexus-spoke-adapter` dependency**; the `extensions.nexus` round-trip helpers (`build_extensions_nexus`, `is_known_nexus_key`) are inlined as private local fns in `kb_store.rs` so the legacy INSERT/UPDATE wrappers stay spoke-unaware. See `.mstar/specs/spoke-adapter-architecture.md` §7.4 for the family matrix (production vs stub).
- **Timeline ordering (V1.146 P1):** the former `SqliteNarrativeGateway::get_timeline_ordered` (which called the spoke `order_timeline_events_by_ids` helper via a direct `spoke-operations` dep) was **removed** — it never had a production call site, and the ordered-timeline facet now lives on the spoke-adapter boundary as `NexusAdapter::list_timeline_events_ordered` in `nexus-spoke-adapter` (spec §7.4). `nexus-local-db` no longer depends on `spoke-operations`. See `.mstar/specs/spoke-adapter-architecture.md` §7.4 "Read-path ScopeQuery adoption".

## Waived Residuals

<!-- WAIVER: pre-1.0 local-first; see V1.41 P-last residual R-V140P0-S3 — sqlx compile-time cache not shared across crates; acceptable until CI enforce single cache -->
<!-- WAIVER: pre-1.0 local-first; see V1.41 P-last residual R-V140P1-S4 — local-db-schema.md not authored; schema is self-documenting via migrations -->
