# QC Consolidated — V1.67 P2 Overdue Debt Closure

**plan_id**: 2026-06-26-v1.67-overdue-debt-closure · **Consolidated by**: @project-manager · **Working branch**: iteration/v1.67 · **Review range**: P2 code `138a98fd..ae1b960e`, diff basis `26e477ee`
**Consolidated verdict**: **Request Changes**

| Seat | Verdict | Blocking |
|---|---|---|
| qc1 (arch) | Approve | (0C/0W; 4 Suggestion; acknowledged qc3 W's) |
| qc2 (security) | Approve | (0C/0W; all 9 verified correct) |
| qc3 (perf) | Request Changes | **W-001** `PRAGMA foreign_key_check` diagnostic not fail-closed; **W-002** `world.delta.apply` IN-list unbounded |

No wire/contract change (qc1 verified `schemas/`+`nexus-contracts/` untouched; `check-wire-drift` clean). ✓

## Fix wave (blocking — qc3)
1. **W-001**: enforce `PRAGMA foreign_key_check` as a **hard** post-migration failure — consume the returned rows; if non-empty → return `LocalDbError` (fail the migration) instead of ignoring. Update the migration test to assert failure on a violated FK.
2. **W-002**: bound the `world.delta.apply` batch pre-fetch IN-list — cap/chunk the input (e.g., max items per IN query, chunk + merge), dedupe, so a caller-supplied unbounded input can't blow up the SQL binding/size. Add a regression test for a large/chunked input.

## Deferred as residuals (register)
- `R-V167P2-QC1-S1`: `outline_five_q_nogo_info_logs_dimensions` regression test (R-V152TA-S006 promise-vs-delivery gap; qc1 S1, low).
- `R-V167P2-QC1-S2`: direct unit test for `is_world_owned` (qc1 S2, low).
- `R-V167P2-QC1-S4`: if a 3rd frontmatter-edit capability ships, extract shared helpers to `frontmatter.rs` (qc1 S4, low).
- `R-V167P2-QC3-S1`: `script.section_status.update` file-level OCC/lock vs concurrent manual/file edits (qc3 S-001, low → V1.68+).

## Re-review after fix wave
Targeted: **qc3 only** (raised W-001/W-002). Update `qc3.md` `## Revalidation`.
