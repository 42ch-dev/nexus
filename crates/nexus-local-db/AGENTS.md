# nexus-local-db — Local Database Layer

SQLite-backed persistence using sqlx. Used by the daemon runtime and `nexus-orchestration`.

## Migrations

Migration files under `migrations/` use naming convention `YYYYMMDD_<description>.sql`. All migrations are committed — the schema is fully reproducible.

## Key Rules

- **Compile-time checked queries only** — use `sqlx::query!()` / `sqlx::query_as!()` for all static SQL. Runtime `sqlx::query()` only for DDL, PRAGMAs, or truly dynamic SQL with a `// SAFETY:` comment.
- See [`crates/nexus-daemon-runtime/AGENTS.md`](../nexus-daemon-runtime/AGENTS.md) for full sqlx compile-time macro rules and `.sqlx/` commit conventions.
- Do not add local sqlx features beyond what the workspace declares.
