-- graph-flow 0.8 session OCC version (2026-09-10 dependency sweep, PR #276)
-- Design: `.mstar/plans/20260910-dependency-bot-sweep.md` (T1 brief §3)
--
-- Append-only migration. Adds the durable `graph_version` column backing
-- graph-flow 0.8's optimistic-locking `Session.version` to
-- `orchestration_sessions`. Pure `ALTER TABLE ... ADD COLUMN` — every old
-- row/blob is preserved; existing rows start at graph_version 0 and gain a
-- version only when legitimately written.
--
-- Ordering: 14-digit prefix `20260910000001` so this runs AFTER all shipped
-- 14-digit migrations (latest: 20260907000002) — see crate AGENTS.md.
--
-- Clock separation (brief §3.1): `graph_version` maps exclusively to
-- graph-flow's `Session.version` (runner position/context OCC).
-- `state_revision` remains the authoritative workflow control CAS;
-- `execution_version` remains the 0-legacy/1-authoritative format marker.
-- Neither is replaced or re-purposed by this column.

ALTER TABLE orchestration_sessions ADD COLUMN graph_version INTEGER NOT NULL DEFAULT 0;
