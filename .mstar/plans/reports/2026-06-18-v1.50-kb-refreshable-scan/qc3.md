---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: "2026-06-18-v1.50-kb-refreshable-scan"
working_branch: "feature/v1.50-kb-refreshable-scan"
review_cwd: "/Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v150-kb-refreshable-scan"
review_range: "merge-base c2831fa25ae7732bac1fe1a11a318e5a7b1626b2..e24574ae5f5f6e8186ee87fa1bc3d3acdc5f885c"
verdict: "Request Changes"
generated_at: "2026-06-17T14:35:00Z"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-3
- Runtime Agent ID: qc-specialist-3
- Runtime Model: zhipuai-coding-plan/glm-5.2
- Review Perspective: performance + reliability (primary); also CI surface
  and regression-test coverage
- Report Timestamp: 2026-06-17T14:35:00Z

## Scope
- plan_id: 2026-06-18-v1.50-kb-refreshable-scan
- Review range / Diff basis: merge-base c2831fa25ae7732bac1fe1a11a318e5a7b1626b2..e24574ae5f5f6e8186ee87fa1bc3d3acdc5f885c
- Working branch (verified): feature/v1.50-kb-refreshable-scan
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v150-kb-refreshable-scan
- Files reviewed: 9 changed (+1946/-13); primary: `crates/nexus42/src/commands/creator/kb/rescan.rs` (577 LOC), `crates/nexus-kb/src/extract_sync.rs` (313 LOC), `crates/nexus-local-db/src/kb_extract_job.rs` (new upsert/cleanup), CLI + DAO test files, migration renumber
- Commit range (identical to Review range): c2831fa2..e24574ae (6 commits: 483adf51, 9daeb4ad, 5b2e7b52, 2cbd5498, 1407ce7a, e24574ae)
- Tools run:
  - `cargo test -p nexus-local-db --test kb_extract_jobs_upsert` (6 passed)
  - `cargo test -p nexus42 --test kb_rescan_cli` (8 passed)
  - `cargo test -p nexus42 --test world_kb_promotion_cli` (11 passed — regression)
  - `cargo test -p nexus-local-db --test kb_extract_jobs_migration` (8 passed — regression)
  - `cargo test -p nexus42 --test world_kb_cli` (9 passed — regression)
  - `cargo test -p nexus42 --test world_kb_authz` (4 passed — regression)
  - `cargo test -p nexus42 --test regression` (15 passed — regression)
  - `cargo test -p nexus-orchestration --test cron_supervisor` (18 passed — regression)
  - `cargo test --workspace` → 1 failure identified (see Findings)
  - `cargo +nightly fmt --all --check` (clean)
  - `cargo clippy -p nexus-kb -p nexus-local-db -p nexus42 -- -D warnings` (clean)
  - Manual code inspection of `upsert_pending_candidate`, `diff_and_apply`, `compute_kb_diff`, `parse_target`, `require_world_owner`, `resolve_workspace_id`, `preview_candidate_outcome`, `extract_sync` apply-loop, and migration file content.

## Findings

### 🔴 Critical
None.

### 🟡 Warning
- **W-QC3-01 (CI surface / pre-existing latent test defect now exercised) — `rollback_drops_schedule_json_column` fails on this branch.** `cargo test -p nexus-local-db --test works_schedule_migration` reports `1 failed` (7 of 8 pass; only `rollback_drops_schedule_json_column` panics). Failure mode:
  ```
  SQLite must support DROP COLUMN (>=3.35): Database(SqliteError { code: 1,
    message: "error in index idx_works_schedule_json_nonempty after drop
              column: no such column: schedule_json" })
  ```
  The test (added in T-A P0 T1, `cdceac31`) simulates a down-migration by `ALTER TABLE works DROP COLUMN schedule_json`. T-A P1 (commit `67db009b`) subsequently added the partial index `idx_works_schedule_json_nonempty` on that column but did not update the rollback test, so `DROP COLUMN` is now blocked by the dependent index. The defect was **latent on the merge-base** because T-B P2 was the first PR to actually make the migrations runnable on a fresh DB (R-V150KBED-06 fixed the duplicate `202606180002` collision; without that fix, `sqlx::migrate!` aborted before any test could run). The plan's "Validation" §7 only enumerates `world_kb_promotion_cli` + `kb_extract_jobs_migration` as the post-`R-V150KBED-06` regression set and missed the full `cargo test -p nexus-local-db` run, so the regression was not caught at sign-off.
  Per `.mstar/AGENTS.md` "Pre-existing claim verification protocol" the claim is **technically TRUE** on the merge-base (the test was already failing with a different mode — the `UNIQUE constraint failed: _sqlx_migrations.version` migration error), but the practical effect on this branch is **a new CI red** that the renumbering fix unmasks. Per `mstar-review-qc` CI gate, "any CI failure defaults to ≥ Warning treatment, must be addressed in this round; CI failure not addressed → cannot give Approve".
  **Fix (1–2 lines, surgical):** in `crates/nexus-local-db/tests/works_schedule_migration.rs:114`, drop the dependent index before the column:
  ```rust
  sqlx::query("DROP INDEX IF EXISTS idx_works_schedule_json_nonempty")
      .execute(&pool).await.expect("drop index");
  sqlx::query("ALTER TABLE works DROP COLUMN schedule_json")
      .execute(&pool).await.expect("SQLite must support DROP COLUMN (>=3.35)");
  ```
  (No source/behavior change in the index migration; the test is a *simulated* rollback, so dropping the partial index mirrors the matching forward migration's index drop. Alternative: split the rollback into two `tx.execute` calls inside a single transaction so the index drop and column drop are atomic.)
  **Re-review:** targeted re-review of just this finding (per `mstar-review-qc` "After Request Changes (default)"). No need to re-run the full 14-test hermetic suite; re-run `cargo test -p nexus-local-db --test works_schedule_migration` and confirm `8 passed`.

- **W-QC3-02 (R-V150KBED-02 only partially self-corrected) — `upsert_pending_candidate` UPDATE branch does not refresh `workspace_id`.** Plan §7 "Key design decisions" item 4 claims "R-V150KBED-02 self-corrects: the rescan resolves `workspace_id` fresh from `narrative_gateway`". The CLI `kb_rescan_hermetic` does call `resolve_workspace_id(pool, creator_id)` once at the start and passes the resolved value into every `upsert_pending_candidate` call — but the DAO's UPDATE statement (line 637–647) only sets `proposed_payload`, `block_type_guess`, and `source_chapter_id`; it deliberately leaves `workspace_id` untouched. The code comment at line 756 calls the column "informational only", so this is a conscious partial fix: the freshly-resolved `workspace_id` only lands on **newly-inserted** rows (via `insert_pending`'s INSERT), not on updated ones. Net effect: a row that was inserted under stale-`workspace_id` and then survives to the next rescan will keep the stale value indefinitely. The plan overstates the fix.
  **Suggested fix (optional for this PR; can be tracked as a residual):** add `workspace_id = ?` to the UPDATE statement's SET list and pass the resolved value. The `idempotency_guard_survives_confirm_but_not_reject` semantics (KB extract lifecycle) are unaffected — the column is denormalized and any non-null value is acceptable per the existing comment.
  Owner for residual: `@fullstack-dev` (or whichever dev takes V1.50 P3+). Target V1.51+.

### 🟢 Suggestion
- **S-QC3-01 (`diff_and_apply` apply-loop is O(N) per update, not O(1)).** The function calls `compute_kb_diff` to get the result struct, then drains `diff.updated` to apply body refreshes. For each update, it does `old_rows.iter().find(|kb| kb.key_block_id == update.key_block_id)` (O(N)) and `new_extracted.iter().find(|(n, _)| n.eq_ignore_ascii_case(&old_kb.canonical_name))` (O(M)). `compute_kb_diff` already built an `active_by_name: HashMap<String, &KeyBlock>` internally for its own matching — that map is dropped before `diff_and_apply` runs. For current scale (MAX_CANDIDATES_PER_PASS=20, LIST_BY_WORLD_LIMIT=500, expected updates ~1–5 per rescan) the O(N+M) per update is a non-issue, but the function is awkward to read because the lookup indices are built and thrown away. **Suggested refactor:** thread a `&HashMap<String, &KeyBlock>` + `&HashMap<String, &KeyBlockBody>` from `compute_kb_diff` into `diff_and_apply` so the apply loop uses pre-built indices. No behavior change; clarity + future-proofing only. Owner: V1.51+ (low priority).

- **S-QC3-02 (per-row transaction wrapper is a deliberate non-goal, document why).** `sync_candidates` does N+1 upsert/deletes and `sync_kb_rows` does M+1 `update_key_block` calls without an outer `BEGIN IMMEDIATE`/`COMMIT`. For a single-user local CLI this is fine (idempotent: the unique index on `(creator_id, work_entry_id, world_id)` and the per-row store contract protect against double-write; a partial-failure rescan is re-runnable). The qc2 reviewer also flagged this. **Suggestion:** add a one-line note to the plan or `kb_rescan_hermetic` doc-comment explicitly stating "no outer transaction by design; the rescan is re-runnable for partial recovery; the V1.51+ daemon-driven rescan (per compass §0.1 non-goal) is the right time to add an outer tx if/when concurrent rescans become possible". Not blocking for this PR.

- **S-QC3-03 (`--dry-run` is bounded by upstream caps, not by a CLI `--limit`).** The dry-run output is naturally bounded by `MAX_CANDIDATES_PER_PASS=20` (heuristic cap, quality_loop.rs:52) and `LIST_BY_WORLD_LIMIT=500` (kb_store.rs:96, applied during the `kb_key_blocks` diff), so even on a pathological chapter the JSON/human report stays small. The plan's "Issues/Risks" §R-V150KBED-08 already notes a different dry-run concern (cross-chapter preview faithfulness). A `--limit` flag is not needed for current scale; revisit if V1.51+ raises the per-pass cap.

- **S-QC3-04 (parallel test safety confirmed).** The 14 new hermetic tests (6 `kb_extract_jobs_upsert` + 8 `kb_rescan_cli`) each build a fresh `tempfile::tempdir()` + `Schema::init(&db_path)` + `seed::world`, so they share no state across tests and are safe under `cargo test`'s default parallel test runner. Ran 5x with no flake, no ordering dependency. Good.

- **S-QC3-05 (R-V150KBED-06 migration renumbering is correct and minimal).** The renumber 202606180002 → 202606180003 only changes the migration version header; the file body (`CREATE INDEX IF NOT EXISTS idx_works_schedule_json_nonempty ...`) is identical to the pre-renumber content. The companion `202606180002_kb_extract_jobs_extend.sql` retains the `202606180002` slot. The `cron_supervisor` EXPLAIN test (`partial_index_used_in_schedule_json_scan`) keys on the index *name* (`idx_works_schedule_json_nonempty`), not the migration version, so it remains green (verified: `cargo test -p nexus-orchestration --test cron_supervisor` → 18 passed, including the EXPLAIN assertion). Fix is minimal, surgical, and follows the V1.50 T-A P1 acceptance criteria.

## Source Trace
- Finding W-QC3-01:
  - Source Type: linter / test execution
  - Source Reference: `cargo test -p nexus-local-db --test works_schedule_migration` → `rollback_drops_schedule_json_column` panic at `crates/nexus-local-db/tests/works_schedule_migration.rs:117`
  - Confidence: High
  - Cross-check: identical file at merge-base `c2831fa2` (commit `87ea2ef6` history; test not modified in this branch per `git log c2831fa2..HEAD -- crates/nexus-local-db/tests/works_schedule_migration.rs` → empty). Index migration 67db009b introduced the dependent index. R-V150KBED-06 (2cbd5498) is what unmasks the test by making `sqlx::migrate!` succeed.
- Finding W-QC3-02:
  - Source Type: manual-reasoning + code inspection
  - Source Reference: `crates/nexus-local-db/src/kb_extract_job.rs:636-647` (UPDATE statement); `crates/nexus42/src/commands/creator/kb/rescan.rs:173-174` (CLI calls `resolve_workspace_id` once and forwards); plan §7 item 4
  - Confidence: High
- Finding S-QC3-01:
  - Source Type: manual-reasoning
  - Source Reference: `crates/nexus-kb/src/extract_sync.rs:146-187` (apply loop); `crates/nexus-kb/src/extract_sync.rs:86-95` (HashMap built inside `compute_kb_diff` and discarded)
  - Confidence: High
- Findings S-QC3-02, S-QC3-03, S-QC3-04, S-QC3-05: source trace as above or in their respective sections.

## Performance / Reliability Checks (per assignment perspective)

- **Idempotent upsert cost on a chapter with N candidates:** bounded by `idx_kb_extract_jobs_idempotent` UNIQUE partial index lookup on `(creator_id, work_entry_id, world_id) WHERE status NOT IN ('failed')`. Each upsert does 1 SELECT + (1 INSERT | 1 UPDATE | 0). For N candidates the loop is O(N) DB round-trips; at N=20 (MAX_CANDIDATES_PER_PASS) the per-rescan cost is well under 100ms on a local SQLite DB. Stale-candidate deletion adds `old_for_chapter.len()` DELETE round-trips; at the same scale this is bounded. **No N+1 concern at expected scale (10–100 KB rows per world).**

- **`compute_kb_diff` cost:** O(rows + new_extracted) using a `HashMap<String, &KeyBlock>` indexed on lowercased `canonical_name`. At LIST_BY_WORLD_LIMIT=500 active blocks + MAX_CANDIDATES_PER_PASS=20 new candidates, this is a single linear pass with O(1) lookups in the apply phase. **OK at expected scale.**

- **`diff_and_apply` row-level transaction overhead:** no outer transaction (see S-QC3-02); each `update_key_block` is its own implicit tx. The store's `update_key_block` runs the body through `ValidationMode::Novel` validation, which is a small inline cost. **Bulk SQL is not possible without changing the `KbStore` trait surface; current per-row design is consistent with the rest of `nexus-kb` and is acceptable for the rescan's per-chapter granularity.**

- **`--dry-run` unbounded on a large chapter:** the dry-run report is bounded by upstream caps (S-QC3-03). The `kb_inserted_advisory` and `kb_removed_advisory` lists are populated from the diff, which is itself bounded by `LIST_BY_WORLD_LIMIT=500` + `MAX_CANDIDATES_PER_PASS=20`. The CLI prints a tabular summary plus per-name lines; at 500 names this is ~30KB of human output. **Acceptable; not a real risk.**

- **R-V150KBED-06 (migration renumber):** verified the renumber is minimal (header only) and unblocks the workspace DB init path. Per W-QC3-01, it also surfaces a pre-existing latent test defect that the plan's regression-test enumeration missed.

- **14 hermetic tests parallel-safe:** verified by inspection (each builds its own tempdir + DB; no shared state) and by repeated `cargo test` runs (5x, no flake). No `R-V149P1-02`-style tracing-registry concerns because the tests don't spawn threads or join a global registry.

- **19 regression tests now passing:** the plan enumerates 4 of them (world_kb_promotion_cli 11, kb_extract_jobs_migration 8, kb_rescan_cli 8, kb_extract_jobs_upsert 6 = 33, not 19 — the "19" in the assignment may refer to a different grouping). All 33 (and the 5 additional regression test files I ran: world_kb_cli 9, world_kb_authz 4, regression 15, cron_supervisor 18, kb_extract_jobs_migration 8) pass. No new flake surface. **The one remaining failure is the unrelated works_schedule_migration test (W-QC3-01).**

- **Tracing / log levels:** `nexus-kb` and `nexus-local-db` do not log inside the hot path of `diff_and_apply` / `upsert_pending_candidate`. The CLI's `print_report` writes to stdout. Error mapping in `map_kb_sync_error` preserves the underlying `KbStoreError` shape. **Log-level posture is appropriate.**

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 2 |
| 🟢 Suggestion | 5 |

**Verdict**: Request Changes

The branch is **architecturally sound** (clean three-layer split, surgical
upsert/cleanup primitives, pure domain delta, cross-author gate consistent
with T-B P0/P1, hermetic test parity), the **14 new tests all pass**, the
**R-V150KBED-06 migration fix is correct and minimal**, and the
**34-suite regression set all passes** — but the plan's "Validation" §7
enumerated a narrower regression set than the actual scope, and the
`works_schedule_migration::rollback_drops_schedule_json_column` test fails
on this branch because R-V150KBED-06 unmasks a pre-existing latent defect
in the rollback test (it does not drop the dependent partial index before
the DROP COLUMN). Per the `mstar-review-qc` CI gate, a CI failure defaults
to ≥ Warning and blocks `Approve`. The fix is a 1–2 line change in the
test (drop index first); targeted re-review of just W-QC3-01 is requested.
W-QC3-02 (partial R-V150KBED-02 self-correction) is recommended for residual
tracking but does not block this PR.
