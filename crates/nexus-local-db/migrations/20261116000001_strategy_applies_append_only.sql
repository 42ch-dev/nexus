-- v1.242 IR P4/P5 Task 2: strategy_applies is append-only and immutable, enforced (I-000444).
--
-- Plan: .mstar/plans/ride-along-apply-ir-hardening.md §Task 2.
--
-- 20261114000001_strategy_applies.sql documents the table as append-only and immutable, but the
-- triggers it ships enforce the approval pin and the writer fence only. An admitted writer can
-- therefore UPDATE a committed row — repointing `supersedes_apply_id` forks or orphans the chain
-- the current-apply reads derive — DELETE one, silently making a superseded approval current
-- again, and supersede a row of a DIFFERENT strategy, splicing one strategy's history into
-- another's.
--
-- Four triggers close it:
--
--   * `immutable_strategy_applies_update` / `immutable_strategy_applies_delete` refuse the
--     mutation itself rather than only the writer. There is no legitimate mutation: `apply.rs`
--     only inserts, and rollback is another append, so the refusal is unconditional.
--   * `supersede_strategy_applies_insert` refuses a supersede link whose target row exists under
--     another strategy. It deliberately does not restate the foreign key (a missing target) or the
--     self-supersede CHECK: it fires only for a link that points at some row of a different
--     strategy.
--   * `append_only_strategy_applies_insert` refuses an `INSERT OR REPLACE` that would resolve onto
--     an existing `apply_id`. REPLACE settles its conflict by DELETING the victim, and that
--     victim's DELETE trigger does not fire while `recursive_triggers` is OFF (the default) — the
--     same trap `20260919000001_operation_receipts.sql` closes with a BEFORE INSERT guard.
--
-- Why a trigger is not shadowing a typed refusal: `apply.rs` always writes the supersede link from
-- a subquery scoped to `cur.strategy_id = ?`, so the cross-strategy link is not expressible by the
-- core at all — there is no `ApplyRefusal` variant for it to lose, and no clean error path turns
-- into a constraint violation. This trigger exists for the admitted non-core writer the rest of
-- these guards exist for.
--
-- The `guard_*` / `pinned_approval_*` / `outbox_*` families from 20261114000001 stay in place:
-- `writer_protocol.rs` requires a guard trio on every persistent table, and the INSERT path they
-- cover is still the only path that writes.

CREATE TRIGGER IF NOT EXISTS immutable_strategy_applies_update
BEFORE UPDATE ON strategy_applies
FOR EACH ROW
BEGIN
  SELECT RAISE(ABORT, 'APPLY_ROW_IMMUTABLE');
END;

CREATE TRIGGER IF NOT EXISTS immutable_strategy_applies_delete
BEFORE DELETE ON strategy_applies
FOR EACH ROW
BEGIN
  SELECT RAISE(ABORT, 'APPLY_ROW_IMMUTABLE');
END;

CREATE TRIGGER IF NOT EXISTS append_only_strategy_applies_insert
BEFORE INSERT ON strategy_applies
FOR EACH ROW
BEGIN
  SELECT RAISE(ABORT, 'APPLY_ROW_IMMUTABLE')
  WHERE EXISTS (
      SELECT 1
      FROM strategy_applies existing
      WHERE existing.apply_id = NEW.apply_id
  );
END;

CREATE TRIGGER IF NOT EXISTS supersede_strategy_applies_insert
BEFORE INSERT ON strategy_applies
FOR EACH ROW
BEGIN
  SELECT RAISE(ABORT, 'APPLY_SUPERSEDE_STRATEGY_MISMATCH')
  WHERE NEW.supersedes_apply_id IS NOT NULL
    AND EXISTS (
        SELECT 1
        FROM strategy_applies superseded
        WHERE superseded.apply_id = NEW.supersedes_apply_id
          AND superseded.strategy_id <> NEW.strategy_id
    );
END;
