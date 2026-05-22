# Reference Sources Persistence — local-db vs nexus-knowledge

## Status

Accepted

## Context

Nexus currently has two reference source models:

1. `nexus-local-db` production persistence: a SQLite-backed `reference_sources` table with columns such as `reference_source_id`, `workspace_id`, `source_type`, `uri`, `title`, `tags`, `content_hash`, and `scan_status`. This is the path used by the production daemon `GET /v1/local/references` handler.
2. `nexus-knowledge::ReferenceSource` crate model: an in-crate aggregate with domain methods such as `register()`, `mark_scanned()`, and `ignore()`. This model currently has only `InMemoryKnowledgeStore` support and is not used by the production daemon reference listing path.

These models diverged because `nexus-local-db` became the shipped SQLite persistence surface for local runtime data, while `nexus-knowledge` retained an in-memory domain aggregate without a production persistence adapter. DF-43 tracks this persistence ownership gap.

## Decision

Keep the current `nexus-local-db` `reference_sources` table as the production persistence owner for reference sources.

`nexus-knowledge::ReferenceSource` remains an in-memory crate model for now. It may be aligned with or integrated into the production path in a later plan, but it is not the persistence owner today.

## Rationale

- `nexus-local-db` is already shipped and tested against real SQLite storage.
- `nexus-knowledge` does not currently provide a persistent store implementation for `ReferenceSource`.
- Migrating production ownership to `nexus-knowledge` would require a persistence adapter, data migration, and regression testing beyond the current scope.

## Migration Trigger

Re-evaluate this decision if and when `nexus-knowledge` gains SQLite or file-backed persistence for `ReferenceSource`. At that point, a follow-up plan should compare direct integration, adapter-based reuse, and migration costs before changing production ownership.

## Consequences

- Future plans still need duplicate model alignment work between `nexus-local-db` and `nexus-knowledge::ReferenceSource`.
- No immediate implementation changes are required.
- DF-43 is partially resolved: the production persistence path is decided, while crate-model integration remains open.

## Non-goals

- No Rust source changes.
- No database schema changes.
- No migration implementation.
- No changes to daemon handlers, tests, codegen, or configuration.
