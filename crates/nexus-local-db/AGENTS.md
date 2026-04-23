# nexus-local-db — Local Database Layer

Provides SQLite-backed persistence using sqlx. Used by `nexus42d` (daemon) and `nexus-orchestration` for structured data storage.

## Migrations

Migration files live under `migrations/` with the naming convention `YYYYMMDD_<description>.sql`.

When adding a new migration:

1. Write the migration SQL file in `migrations/`.
2. Run `cargo sqlx database reset` and `cargo sqlx prepare --workspace --all -- --all-targets` to update `.sqlx/` metadata.
3. Commit `migrations/` **and** `.sqlx/` in the same commit.

See [`crates/nexus42d/AGENTS.md`](../nexus42d/AGENTS.md) for full sqlx compile-time macro rules and `.sqlx/` commit conventions.

## Key Rules

- **Compile-time checked queries only** — use `sqlx::query!()` / `sqlx::query_as!()` for all static SQL. Runtime `sqlx::query()` only for DDL, PRAGMAs, or truly dynamic SQL with a `// SAFETY:` comment.
- All migration files are committed to git — the database schema is fully reproducible from source.
- This crate re-uses the workspace `sqlx` dependency. Do not add local features beyond what the workspace declares.
