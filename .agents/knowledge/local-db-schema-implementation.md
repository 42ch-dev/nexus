# Local DB Schema — Implementation SSOT

## Document metadata

| Field | Value |
| --- | --- |
| **Status** | Active |
| **Authority** | `crates/nexus-local-db/migrations/*.sql` |
| **Normative governance** | `nexus-platform` `.agents/designs/v1-spec/local/local-db-schema-v1.md`, ADR-005 |
| **Stack** | **sqlx** + committed migrations (not rusqlite/deadpool from older spec prose) |

---

## Migration files

| File | Purpose |
| --- | --- |
| `20260417_000001_initial.sql` | Core tables: `workspace_meta`, `creators`, `reference_sources`, `local_identities`, `outbox`, `auth_tokens`, `acp_tool_audit_log`, … |
| `20260418_orchestration_sessions.sql` | Orchestration session storage |
| `20260419_creator_schedules.sql` | Schedule + core-context tables |
| `20260420_outbox_tables.sql` | Outbox extensions / sync queue |
| `20260427_drop_device_code_sessions.sql` | Removes `device_code_sessions` (spec §4 may still mention — **dropped**) |
| `20260511_world_stories.sql` | `world_stories` linkage |

---

## Tables present in code but thin in v1-spec §4

Agents implementing persistence should treat migrations as truth for:

- `local_identities`, `soul_meta`, `memory_pending_review`
- `orchestration_sessions`, `creator_schedules`, `core_context_*`
- `world_stories`

---

## Conventions

- Static SQL: `sqlx::query!` / `query_as!` only (see `nexus-local-db/AGENTS.md`, `nexus-daemon-runtime/AGENTS.md`).
- `DATABASE_URL` for prepare: `sqlite:.sqlx/state.db?mode=rwc` (repo-relative).

---

## Related

- Archived refactor narrative: `.agents/archived/knowledge/local-db-refactor.md`
- FS layout (not SQLite): [local-fs-layout-creator-workspace.md](./local-fs-layout-creator-workspace.md)
