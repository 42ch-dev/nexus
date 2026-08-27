---
title: sqlx migration files are immutable once shipped
category: engineering
track: knowledge
last_updated: 2026-08-27
created: 2026-08-27
status: active
---

# sqlx Migration Files Are Immutable Once Shipped (checksum pinning)

## Context

`sqlx::migrate!("./migrations")` embeds migration files **at compile time** and records each applied file's checksum in the `_sqlx_migrations` table of every database that ran it. On every subsequent boot, `Migrator::run` compares the embedded checksum against the stored one — any byte difference in an already-applied migration file (even a comment typo fix) makes `run()` return `MigrateError::VersionMismatch(version)`, which fails daemon/CLI boot for **every existing install**. Fresh installs are unaffected, so the breakage ships silently until a user upgrades.

## Guidance

1. Treat every file under `migrations/` as append-only from the moment it reaches a released build: fixes go into a **new** forward migration or into documentation — never an in-place edit.
2. When a shipped migration contains wrong prose (typo, wrong version attribution), correct the record in the **spec/docs corpus**, noting the migration-header value as known-immutable. Do not "fix" the file.
3. Before locking a plan whose writable set could plausibly touch `migrations/`, add an explicit immutability constraint + a diff-scope test (`git diff --name-only <base>..<head>` contains no `migrations/` path) — cheap to verify, expensive to violate.

## Why This Matters

The apparent fix is always tempting because editing comments looks behavior-free — SQL is untouched, so tests pass. The hazard is data, not code: checksums are computed over file bytes. In v1.178, the drop-migration header said "V1.159 T3" while sibling specs said "V1.59 T3"; archaeology proved V1.59 was correct, but the only safe channel was a provenance note in `.mstar/specs/local-db-schema.md` §4.2 plus tracking the header value as known-immutable (AR-106). An unexplained constraint would eventually be "cleaned up" by a well-meaning editor.

## When to Apply

- Any typo/factual error discovered inside an already-released migration file
- Plan review touching anything under `migrations/`, even comment-only hunks
- Any tooling that mass-edits historical files (formatters, codemods): whitelist-exclude `migrations/`

## Examples

v1.178 (2026-08-27): verified with sqlx 0.8.6 (`sqlx-core/src/migrate/migrator.rs` — `validate_applied_migrations` ignores drift only when `ignore_missing=true`; the run loop hard-fails on `migration.checksum != applied_migration.checksum`). Empirical confirmation during T2 verification: converting `#[allow]`→`#[expect]` elsewhere showed `-D warnings` surface; the no-migrations-path assertion passed via `git diff --name-only`.

## Prevention

Keep the plan-level writable-set exclusion (`migrations/` excluded unless adding new files) as a standing norm; consider a CI check rejecting modifications to previously-released migration paths via `git diff --name-only origin/main...HEAD -- crates/*/migrations/`.
