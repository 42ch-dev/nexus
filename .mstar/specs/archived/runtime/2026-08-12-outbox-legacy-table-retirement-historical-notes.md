# Outbox Consolidation — Legacy-Table Retirement Historical Notes

**Provenance**: frozen from [`specs/active/runtime/outbox-consolidation.md`](../../active/runtime/outbox-consolidation.md) on 2026-10-13 during the **v1.216** de-historicization pass.
**Evidenced supersession date**: 2026-08-12 — migration `20260812_drop_legacy_outbox.sql` (V1.163) dropped the legacy `outbox` table; the removed blocks are the file's closed-history records of the V1.59 deprecation and the V1.163 drop. Nothing here is current authority.
**Freeze date**: 2026-10-13

---

## Removed Status line and footer (replaced present-tense)

**Status**: Normative (Master, V1.59 P-last promote; **V1.177 revision** — daemon `outbox` table dropped at V1.163, §2.3/§6 rewritten as closed history)

*Last updated: 2026-06-22 (initial Draft). Promoted to Master at V1.59 P-last. V1.177 revision: daemon `outbox` table dropped at V1.163 — §2.3/§6 rewritten as closed history.*

## §1 original problem summary (replaced present-tense)

Prior to V1.59, two outbox concepts coexisted:

1. **Sync outbox** (`outbox_entries` table, DDL in `20260420_outbox_tables.sql` migration, managed by `nexus-cloud-sync::outbox::Outbox`) — full delivery-state machine (staged → ready → sent → acked/conflicted/failed), retry with exponential backoff, partial-apply persistence.
2. **Daemon legacy outbox** (`outbox` table, DDL in initial migration `20260417_000001_initial.sql`) — simple command queue (`id`, `command_type`, `payload`, `status`, `created_at`, `sent_at`, `error`) with **no active Rust-level consumers** (confirmed by V1.59 T3 audit). It existed only as DDL with a single test assertion in `nexus-daemon-runtime/src/db/schema.rs`; the table was dropped at V1.163 (see §3.3, §6.3).

The split was identified in V1.1-era TD-8 and deferred for a later consolidation. V1.59 P1 is that consolidation.

## §2.3 removed enforcement item + note

3. **Daemon-runtime schema test** (`nexus-daemon-runtime/src/db/schema.rs`): the assertion that the legacy `outbox` table exists was removed at V1.163 together with the table (migration `20260812_drop_legacy_outbox.sql`). No enforcement remains for a table that no longer exists.

No debug-mode `assert!` was ever added for the legacy table because it had **zero active write paths** — there was no code path to guard; the presence assertion that did exist was removed with the table at V1.163.

## §3.3 Migration path (closed history — replaced by compact drop note)

- **V1.59**: deprecated the legacy `outbox` table; confirmed `outbox_entries`/`partial_apply_states` are migration-managed; wired flush/compact capabilities.
- **V1.163**: dropped the legacy `outbox` table via `crates/nexus-local-db/migrations/20260812_drop_legacy_outbox.sql` (`DROP TABLE IF EXISTS outbox;`) and removed the daemon-runtime schema-test assertion. The planned intermediate V1.60 `_deprecated` suffix-comment step was not executed; the drop shipped directly.
- **No data migration** was needed — the tables are independent schemas with no shared data.

## §6 Legacy Outbox Table Deprecation (removed subsections)

### 6.1 Audit result (V1.59 T3)

The legacy `outbox` table (defined in `20260417_000001_initial.sql`) was audited for active consumers:

- **Rust code reads**: 0
- **Rust code writes**: 0
- **Test assertions**: 1 (`nexus-daemon-runtime/src/db/schema.rs` line 64 — `assert!(table_names.contains(&"outbox"))`)
- **SQL references (non-migration)**: 0

**Conclusion**: The table has **no active consumers**. It exists only as DDL with a single test assertion that confirms its presence. The assertion was removed and the table dropped at V1.163 (see §6.3).

### 6.2 Deprecation approach (V1.59, closed history)

1. **No DDL change** — the table was NOT dropped at V1.59 (per plan constraint).
2. **Deprecation annotation**: the test assertion in `nexus-daemon-runtime/src/db/schema.rs` was annotated with a deprecation comment and `tracing::warn!` noting the phased-removal plan. The annotation and the assertion were removed at V1.163 together with the table.
3. **Spec documentation**: this document is the official record of the deprecation.

### 6.3 Phased removal — executed (closed history)

| Phase | Action | Status |
|---|---|---|
| V1.59 | Deprecation marker + audit documentation | Shipped |
| V1.60 | Add `_deprecated` suffix comment to migration; verify no external tooling references the table | Not executed — superseded by the direct V1.163 drop |
| V1.163 | Drop table in a new migration (`20260812_drop_legacy_outbox.sql`); remove test assertion | **Shipped** |

> **Durable roadmap:** DR-08 (phased removal) is **complete** as of V1.163 — the table is dropped and no further removal work remains.

## Annotation drops (reworded in place; originals in git history)

- Status line provenance "(Master, V1.59 P-last promote; **V1.177 revision** — …)".
- §2.2 "it was dropped at V1.163" clause; §2.3 intro "(V1.163+)"; item 1 "which no longer exists".
- §3.1 "(WS8 R4, closed in V1.21)".
- §3.2 legacy-row "Dropped at V1.163 … no runtime owner ever existed" cells.
- §7 checklist "(T5)" suffix and "dropped at V1.163 … deprecation history documented in §6" clause.
