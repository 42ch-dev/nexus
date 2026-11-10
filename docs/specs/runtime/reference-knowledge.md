# Reference Knowledge — Reference Body Refresh Pipeline (Retired)

## Scope

Former `nexus.reference.refresh` capability and the refreshable reference-body pipeline — retired V1.231

## Current state

| Attribute | Value |
| --- | --- |
| **Status** | **Retired (V1.231)** — no live surface. Kept only as the retirement pointer. |
| **Document class** | Master (retired) |
| **Scope** | The former reference refresh policy model, `reference_sources` refresh columns, `nexus.reference.refresh` capability and its scheduler hook |
| **Retired by** | [ADR K1 — Data handling](../../../.agents/adr/implemented/architecture/2026-11-10-retire-user-scope-and-references.md#data-handling); [local-db-schema.md](local-db-schema.md) §4.1.1 |

### 1. What was retired

- The refresh policy model (`offline` / `on_change` / `scheduled`) and the `refresh_status` lifecycle — meaningless without a registry row.
- The refresh tracking columns on `reference_sources` (`last_refreshed_at`, `refresh_policy`, `refresh_status`) and their indexes, dropped with the table by migration `20261111000001_retire_user_and_reference_stores.sql`.
- The `nexus.reference.refresh` capability and its orchestration builtin. The id no longer resolves: `execute_tool` rejects it as an unknown tool before any handler, DB write, `body.md` rewrite or network fetch.
- The scheduler hook in `crates/nexus-core/src/execution/schedules/refresh.rs`, and the refresh lifecycle DAOs (`set_refresh_policy`, `mark_refreshing`, `mark_refreshed`, `mark_refresh_error`, `find_stale_sources`).
- The `nexus.reference.refresh_policy.get` / `nexus.reference.refresh_status` backlog placeholders — withdrawn with the family.

In-flight refresh work was cancelled or failed closed before the table and body tree were removed ([ADR K1 — Data handling](../../../.agents/adr/implemented/architecture/2026-11-10-retire-user-scope-and-references.md#data-handling), third step).

### 2. Replacement

There is none, and none is scheduled. Periodic refresh of external source material would be part of a future independent source-material proposal, feeding the World KB extract gate rather than a private registry. It MUST NOT revive this capability id or the dropped table.

## Invariants

None. The surface no longer exists; this page records only its retirement.

## Where it lives

- `crates/nexus-local-db/migrations/20261111000001_retire_user_and_reference_stores.sql` — the drop
- `.agents/adr/implemented/architecture/2026-11-10-retire-user-scope-and-references.md` — the authority (K1)

## Change obligations

Any change that reintroduces source-material refresh replaces this page with the new proposal's design; it does not extend this one.
