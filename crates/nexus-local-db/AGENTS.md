# nexus-local-db — Local Database Layer

SQLite-backed persistence using sqlx. Used by the daemon runtime and `nexus-orchestration`.

## Migrations

Migration files under `migrations/` use naming convention `YYYYMMDD_<description>.sql`. All migrations are committed — the schema is fully reproducible.

## Key Rules

- **Compile-time checked queries only** — use `sqlx::query!()` / `sqlx::query_as!()` for all static SQL. Runtime `sqlx::query()` only for DDL, PRAGMAs, or truly dynamic SQL with a `// SAFETY:` comment.
- See [`crates/nexus-daemon-runtime/AGENTS.md`](../nexus-daemon-runtime/AGENTS.md) for full sqlx compile-time macro rules and `.sqlx/` commit conventions.
- Do not add local sqlx features beyond what the workspace declares.
- **Pure storage (V1.145 P1b):** this crate is storage-only — DB CRUD primitives (`SqliteKbStore`, `open_pool`, `run_migrations`, CAS helpers). The production `NexusBaselineAdapter` + 6 spoke port impls **moved to `nexus-spoke-adapter/src/adapter/`** (spec §7.4 / §8). `nexus-local-db` has **no `nexus-spoke-adapter` dependency**; the `extensions.nexus` round-trip helpers (`build_extensions_nexus`, `is_known_nexus_key`) are inlined as private local fns in `kb_store.rs` so the legacy INSERT/UPDATE wrappers stay spoke-unaware. See `.mstar/specs/spoke-adapter-architecture.md` §7.4 for the family matrix (production vs stub).
- **`spoke-operations` dep (standard library usage):** `narrative_gateway::get_timeline_ordered` calls the spoke `order_timeline_events_by_ids` ordering helper via a direct `spoke-operations` dep. `spoke-operations` is a leaf dependency (no cycle) — this is standard spoke-library usage, the same way a crate depends on `serde`, not a spoke-adapter aggregation concern. Routing that ordering back through the spoke-adapter boundary is a V1.146 refactor (it needs the narrative read to live in a spoke-adapter-dependent layer, which `narrative_gateway` in local-db cannot be post-reversal). See spec §7.4 "Read-path ScopeQuery adoption".

## Waived Residuals

<!-- WAIVER: pre-1.0 local-first; see V1.41 P-last residual R-V140P0-S3 — sqlx compile-time cache not shared across crates; acceptable until CI enforce single cache -->
<!-- WAIVER: pre-1.0 local-first; see V1.41 P-last residual R-V140P1-S4 — local-db-schema.md not authored; schema is self-documenting via migrations -->
