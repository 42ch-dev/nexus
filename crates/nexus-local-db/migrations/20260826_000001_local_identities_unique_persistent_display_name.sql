-- V1.176 P0 QC fix wave (qc3 S-002): honest TOCTOU fence for concurrent
-- same-name mint.
--
-- `bootstrap_local_creator` decides "mint vs converge" with a read-then-insert
-- (0-match → mint). Two processes that both observe 0 matches can both mint,
-- silently producing two persistent identities with the same display name
-- (the 2+ collision leg only catches that on a third run). This unique
-- partial index makes the second INSERT fail with SQLITE_CONSTRAINT — an
-- honest collision (`creator_name_collision:`), never a silent duplicate.
-- Only persistent rows with a non-NULL display_name are constrained; nameless
-- persistent rows (display_name = NULL) are unaffected.
--
-- Pre-existing duplicate persistent display names (only reachable via the
-- old race) are resolved deterministically: per name, the row with the
-- smallest creator_id keeps the name; later rows get display_name = NULL
-- (identity rows are never deleted — the identity survives, it just no longer
-- matches by name, so a subsequent named run converges the kept row through
-- the 1-match leg).
UPDATE local_identities
SET display_name = NULL
WHERE identity_type = 'persistent'
  AND display_name IS NOT NULL
  AND creator_id NOT IN (
      SELECT MIN(creator_id)
      FROM local_identities
      WHERE identity_type = 'persistent' AND display_name IS NOT NULL
      GROUP BY display_name
  );

CREATE UNIQUE INDEX IF NOT EXISTS idx_local_identities_persistent_display_name
    ON local_identities (display_name)
    WHERE identity_type = 'persistent' AND display_name IS NOT NULL;
