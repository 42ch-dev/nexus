---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: "2026-06-18-v1.50-kb-auto-promotion"
verdict: "Request Changes"
generated_at: "2026-06-18"
---

# Code Review Report — V1.50 T-B P1 KB Auto-Promotion (qc3, performance + reliability)

## Reviewer Metadata
- Reviewer: @qc-specialist-3
- Runtime Agent ID: qc-specialist-3
- Runtime Model: MiniMax/MiniMax-M3
- Review Perspective: performance + reliability (V1.49 R-V149P1-02 flake pattern aware)
- Report Timestamp: 2026-06-18

## Scope
- plan_id: `2026-06-18-v1.50-kb-auto-promotion`
- Review range / Diff basis: `merge-base 0ea2995ff45569b541b17097c4c919dabab4bb16..8eec12e5dac2a023a4b4115483505534119c630c`
- Working branch (verified): `feature/v1.50-kb-auto-promotion`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v150-kb-auto-promotion`
- Files reviewed: 14 changed (4 commits, 2197 insertions, 13 deletions)
- Commit range: `c616dc11` (T1) → `841ec302` (T2/T3) → `13494027` (T4/T5/T6) → `8eec12e5` (docs)
- Tools run:
  - `cargo test -p nexus-local-db --test kb_extract_jobs_migration` (7/7 passed)
  - `cargo test -p nexus-orchestration --lib quality_loop` (6/6 passed)
  - `cargo test -p nexus-orchestration --test review_time_extraction` (5/5 passed)
  - `cargo test -p nexus42 --test world_kb_promotion_cli` (8/8 passed)
  - `cargo test -p nexus42 --lib creator` (229 passed, no new regressions)
  - `cargo clippy -p nexus-local-db -p nexus-orchestration -p nexus42 -- -D warnings` (clean)
  - `cargo +nightly fmt --all --check` (clean)

## Findings

### 🟡 Warning

#### W-001 — `kb_adopt` is not wrapped in a single SQLite transaction (data integrity gap, recoverable orphan)
- **Where**: `crates/nexus42/src/commands/creator/world/kb.rs` lines 472–488 (`kb_adopt`).
- **Behavior**:
  ```rust
  // Step 1: insert KeyBlock (may succeed or fail)
  let insert_result = store.insert_key_block(kb).await?;
  // Step 2: flip promotion row to 'confirmed'
  let flipped = mark_confirmed(pool, extract_job_id).await?;
  if !flipped {
      return Err(CliError::Other(format!(
          "Candidate '{extract_job_id}' was no longer pending ... KeyBlock was not duplicated."
      )));
  }
  ```
- **Failure modes** (each independently reachable):
  1. `insert_key_block` succeeds, then `mark_confirmed` returns `Err` (e.g. DB connection drop, transient SQLite error). The user gets `CliError::Other("Failed to mark candidate confirmed: {e}")`, but the `confirmed` `KeyBlock` is already persisted. The promotion row is **still `pending`**, leaving a clean orphan that can be re-attempted. A retry hits the `kb_key_blocks_active_unique` UNIQUE index (`(world_id, block_type, canonical_name) WHERE status NOT IN ('deleted','merged','deprecated')` per `20260525_kb_key_blocks.sql`) and surfaces a generic `Failed to adopt key block ... Duplicate { ... }`.
  2. `insert_key_block` succeeds, then `mark_confirmed` returns `Ok(false)` (race: a concurrent request flipped the row first). The user gets a misleading error message — it claims **"KeyBlock was not duplicated"** but the KeyBlock **was** inserted. The orphan promotion row remains `pending`. The user has no way to tell from the error which row state they ended up in.
- **Why it matters for reliability**:
  - A failed mark_confirmed path silently produces an **inconsistent 2-row state** that is only visible to the user as a confusing error string. The `Logs/kb/rejected/` audit log is not written on adopt, so there is no recovery breadcrumb.
  - The current hermetic test `double_adopt_is_rejected` exercises the "row is already non-pending at `load_pending_candidate`" path, not the "race between `insert_key_block` and `mark_confirmed`" path. The transactional gap is therefore **uncovered by tests**.
- **Fix options** (any one is sufficient):
  1. Wrap both calls in `pool.begin()` + commit; on either error, `tx.rollback()`. The `SqliteKbStore` should expose a `pool` accessor or take a `&mut Transaction` to allow this.
  2. Reorder: `mark_confirmed` first (idempotency on `pending`); on `Ok(true)`, then `insert_key_block`. The orphan direction is reversed (a `confirmed` promotion row without a `KeyBlock`), but the failure is **less** visible to the user (they can re-run and the idempotent `is_idempotent`/`kb_key_blocks_active_unique` covers the retry).
  3. As a smaller mitigation, correct the misleading "KeyBlock was not duplicated" message in the `Ok(false)` branch to clarify the KeyBlock **was** inserted.

#### W-002 — New index `idx_kb_extract_jobs_promotion_status_work` does not cover the primary list query
- **Where**: `crates/nexus-local-db/migrations/202606180002_kb_extract_jobs_extend.sql` lines 45–46; `crates/nexus-local-db/src/kb_extract_job.rs` line 620–640 (`list_pending_for_world`).
- **Index**:
  ```sql
  CREATE INDEX IF NOT EXISTS idx_kb_extract_jobs_promotion_status_work
      ON kb_extract_jobs (promotion_status, work_id);
  ```
- **Migration comment claims**: "Index for the `creator world kb pending <world_ref>` list query and the idempotency pre-check (`is_idempotent` scans pending|confirmed rows for the same work_id + canonical_name_guess)."
- **But the actual query** (the dominant list path the CLI hits):
  ```sql
  SELECT ... FROM kb_extract_jobs
   WHERE world_id = ? AND promotion_status = 'pending'
   ORDER BY created_at ASC LIMIT N
  ```
  filters on `world_id`, **not** `work_id`. The new index does not cover the `world_id` filter. SQLite will fall back to either `idx_kb_extract_jobs_creator` (which excludes `promotion_status` from the leading columns) or a table scan with a `promotion_status` post-filter. For the V1.50 expected scale (10–100 pending rows per world, 10s of worlds) this is sub-millisecond and not a hot path, but the index is **functionally unused** for the path it is documented to support.
- **Secondary concern (idempotency)**: `is_idempotent` filters on `work_id` + `canonical_name_guess` + `promotion_status IN ('pending','confirmed')`. The index covers `(promotion_status, work_id)` (the leading two columns), but the `canonical_name_guess` filter is a post-filter on the indexed range. Acceptable today; for tables with >100k rows this will start to matter.
- **Fix**: Add a covering index for the actual list path:
  ```sql
  CREATE INDEX IF NOT EXISTS idx_kb_extract_jobs_promotion_status_world
      ON kb_extract_jobs (promotion_status, world_id, created_at);
  ```
  Or, for the idempotency path, `(promotion_status, work_id, canonical_name_guess)`. Either way, the current index name should match the actual filtering pattern or be removed.

### 🟢 Suggestion

#### S-001 — `existing_canonical_names` silently swallows SQL errors
- **Where**: `crates/nexus-orchestration/src/quality_loop.rs` lines 488–502.
  ```rust
  let rows: Result<Vec<(String,)>, sqlx::Error> = sqlx::query_as(
      "SELECT canonical_name FROM kb_key_blocks
       WHERE world_id = ? AND status NOT IN ('deleted', 'merged', 'deprecated')",
  ).bind(world_id).fetch_all(pool).await;
  let rows = rows.map_err(nexus_local_db::LocalDbError::from)?;
  Ok(rows.into_iter().map(|(n,)| n).collect())
  ```
  Wait — re-reading, this *does* propagate the error. But the caller's behavior in `persist_candidates` (line 271) is: if the call returns `Err`, `extract_kb_candidates_for_review` propagates it, the supervisor hook logs it at `warn!` and the schedule terminal transition is **not** failed (best-effort). So a flaky `kb_key_blocks` read causes the entire review-time extraction to silently produce **zero** candidates. The user sees no candidates and no error.
- **Fix**: Log at `warn!` before propagating (operator visibility), and consider an explicit "skip" path that still inserts the row with `is_idempotent` as a backstop — the existing `is_idempotent` check is keyed on `work_id` + `canonical_name_guess`, **not** on the KeyBlock's `canonical_name`, so a missed `existing_names` filter would still surface a candidate that the author then has to reject manually. Acceptable as-is, but the silent-zero path is the surprise.

#### S-002 — Rejected log directory `Logs/kb/rejected/` has no bounded retention
- **Where**: `crates/nexus42/src/commands/creator/world/kb.rs` lines 689–734 (`write_rejected_log`).
- **Behavior**: Each reject creates a new file `<YYYY-MM-DD>-<extract_job_id>.md`. Files are never deleted or compressed.
- **Plan §2 stated** "Rejected retention: log to `Logs/kb/rejected/`." The plan did not specify a retention policy. For a long-lived workspace with hundreds of rejected candidates per chapter, the directory grows unbounded.
- **Fix**: Either (a) add a tiny retention sweep (e.g. delete files older than 90 days, triggered lazily on each new reject), or (b) document the retention policy in `entity-scope-model.md` §5.5.4 so operators know to add their own cron.

#### S-003 — `KbStoreError::Duplicate` from adopt surfaces a generic message
- **Where**: `crates/nexus42/src/commands/creator/world/kb.rs` lines 615–626 (`map_kb_store_error`).
- **Behavior**: When a candidate is adopted that duplicates an existing `KeyBlock` (e.g. a different work in the same world also extracted the same name, or the author manually created the KeyBlock earlier), the user sees: `Failed to adopt key block 'xj_abc' in world 'wld_xyz': Duplicate { world_id: ..., name: ..., block_type: ... }`. The `kb_key_blocks_active_unique` UNIQUE index prevents the duplicate from being persisted, which is correct; only the user-facing message is poor.
- **Fix**: Add a match arm in `map_kb_store_error` for `KbStoreError::Duplicate` that returns a `CliError::Api { status: 409, message: ... }` (or `CliError::Other` with a clear "KeyBlock already exists for this canonical_name+block_type in this world" message). Currently the caller cannot tell from the error whether they should reject, edit the existing block, or pick a different name.

#### S-004 — Adopt path: "KeyBlock was not duplicated" message is misleading
- **Where**: `crates/nexus42/src/commands/creator/world/kb.rs` lines 483–488.
- **Behavior**: When `mark_confirmed` returns `Ok(false)` (race), the user is told "KeyBlock was not duplicated" — but by that point `insert_key_block` has already succeeded, so the KeyBlock **was** inserted. The misleading text could cause the author to retry the adopt and hit `KbStoreError::Duplicate` (see S-003), further confusing the recovery.
- **Fix**: Update the message to: "Candidate '{extract_job_id}' was no longer pending (already confirmed/rejected). The KeyBlock WAS inserted; the promotion row update was not applied. Use `creator world kb list` to inspect or `reject` to clean up." (Combined with W-001 fix this becomes a non-issue.)

#### S-005 — Test coverage gap: the transaction-boundary race is untested
- **Where**: `crates/nexus42/tests/world_kb_promotion_cli.rs`.
- **Behavior**: The current suite verifies the **first** failure path (row is already non-pending when `load_pending_candidate` checks), not the **second** path (`insert_key_block` succeeds then `mark_confirmed` returns `Ok(false)`). Adding W-001's fix should include a regression test that mocks the `mark_confirmed` failure to assert the orphan state is **not** created (i.e. the transaction is rolled back).

## Source Trace
- Finding W-001:
  - Source Type: manual-reasoning + diff review
  - Source Reference: `crates/nexus42/src/commands/creator/world/kb.rs:472-488` vs `crates/nexus-local-db/src/kb_store.rs:222-287` (insert_key_block commits immediately)
  - Confidence: High
- Finding W-002:
  - Source Type: manual-reasoning + diff review
  - Source Reference: `crates/nexus-local-db/migrations/202606180002_kb_extract_jobs_extend.sql:45-46` vs `crates/nexus-local-db/src/kb_extract_job.rs:620-640` (list_pending_for_world filter)
  - Confidence: High
- Finding S-001:
  - Source Type: manual-reasoning
  - Source Reference: `crates/nexus-orchestration/src/quality_loop.rs:488-502` (error propagation) + `crates/nexus-orchestration/src/schedule/supervisor.rs:493-498` (non-fatal swallow)
  - Confidence: Medium (depends on whether the existing `is_idempotent` backstop is considered sufficient)
- Finding S-002:
  - Source Type: manual-reasoning
  - Source Reference: `crates/nexus42/src/commands/creator/world/kb.rs:689-734`
  - Confidence: Medium (no operator complaint, but unbounded growth is a long-term concern)
- Finding S-003:
  - Source Type: manual-reasoning + diff review
  - Source Reference: `crates/nexus42/src/commands/creator/world/kb.rs:615-626` vs `crates/nexus-local-db/src/kb_store.rs:268-280` (Duplicate error variant)
  - Confidence: High
- Finding S-004:
  - Source Type: manual-reasoning
  - Source Reference: `crates/nexus42/src/commands/creator/world/kb.rs:483-488`
  - Confidence: High
- Finding S-005:
  - Source Type: doc-rule
  - Source Reference: `crates/nexus42/tests/world_kb_promotion_cli.rs` (missing race-path test)
  - Confidence: High

## Positive observations (performance + reliability)
- **Heuristic regex is compiled once via `OnceLock`**: `capitalized_phrase_regex()` is initialized once per process and reused across all review-time extractions. O(1) startup amortized.
- **MAX_CANDIDATES_PER_PASS = 20** hard cap on the heuristic output bounds the insert-loop work per schedule terminal. The dedup `Vec::contains` is O(n²) in the worst case but bounded to 20² = 400 ops per pass.
- **`is_idempotent` short-circuits before `insert_pending`**: the review-time hook checks idempotency (`work_id` + `canonical_name_guess`) before any INSERT. The V1.29 UNIQUE index `(creator_id, work_entry_id, world_id) WHERE status NOT IN ('failed')` is also cleverly reused — `insert_pending` binds `canonical_name_guess` to the `work_entry_id` column position, giving a DB-level dedup guard. A racing duplicate would hit the UNIQUE constraint and be logged at `warn!` (silent drop, intentional).
- **`list_pending_for_world` has a hard `clamp(1, 500)` limit**: prevents accidental DoS via CLI. ✓
- **26 hermetic tests all pass on first run, parallel-safe**: every test uses a fresh `tempfile::tempdir()` + `open_pool`, no global state, no `OnceCell` shared mutable state. The V1.49 R-V149P1-02 flake pattern does not apply.
- **Tracing levels are appropriate**: `info!` for the successful insert, `warn!` for non-fatal errors that need operator attention, `debug!` for no-op skips. The supervisor hook's "non-fatal" wrapper matches the existing review-findings hook convention.
- **`resolve_workspace_id` falls back to `creator_id` on lookup failure**: the column is informational only (extraction keys off `world_id` + `work_id`), so the silent fallback does not affect correctness.
- **All 26 hermetic tests pass + 229 nexus42 lib tests pass + clippy clean + nightly fmt clean**: the implementation is correct and well-tested on the happy path.

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 2 |
| 🟢 Suggestion | 5 |

**Verdict**: **Request Changes**

Per `mstar-review-qc` gate: "存在未解决的 `Critical` 或 `Warning` → `Request Changes`". Two unresolved Warnings (W-001 transaction gap, W-002 index column mismatch) must be addressed before this plan merges.

The code is largely sound for the V1.50 expected scale. The two Warnings are recoverable but leave the data model in an inconsistent state on edge cases (DB error between two operations, or wrong-index expectations when the table grows). Both are surgical fixes; W-001 is a 5-line change wrapping two calls in `pool.begin()`, W-002 is renaming/adding one index. After addressing W-001 + W-002, this plan is ready to merge.

## Test evidence
- `cargo test -p nexus-local-db --test kb_extract_jobs_migration` → 7 passed, 0 failed
- `cargo test -p nexus-orchestration --lib quality_loop` → 6 passed, 0 failed
- `cargo test -p nexus-orchestration --test review_time_extraction` → 5 passed, 0 failed
- `cargo test -p nexus42 --test world_kb_promotion_cli` → 8 passed, 0 failed
- `cargo test -p nexus42 --lib creator` → 229 passed, 0 failed (no regressions)
- `cargo clippy -p nexus-local-db -p nexus-orchestration -p nexus42 -- -D warnings` → clean
- `cargo +nightly fmt --all --check` → clean

All 26 hermetic tests in the plan's scope pass; the Warnings above are not test failures but architectural/structural concerns.

---

## Revalidation

```yaml
---
report_kind: qc-revalidation
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: 2026-06-18-v1.50-kb-auto-promotion
working_branch: feature/v1.50-kb-auto-promotion
review_cwd: /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v150-kb-auto-promotion
review_range: 8eec12e5..bab531a1
fix_wave_commits:
  - 2cfbd49e (R-V150KBED-03)
  - 02cc52d5 (R-V150KBED-04)
  - 125533a8 (R-V150KBED-05)
  - bab531a1 (plan completion report)
verdict: Approve
generated_at: 2026-06-18
---
```

### Revalidation scope
- **Type**: Targeted re-review of fix-wave (per `mstar-review-qc` SKILL.md "After Request Changes (default)").
- **Targeted scope**: My two blocking Warnings from the initial wave only — **W-001 (R-V150KBED-03)** and **W-002 (R-V150KBED-04)**. R-V150KBED-05 belongs to qc1 (architecture coherence: reject log path uses work_ref not work_id) and is not in qc3's scope; the docs commit (`bab531a1`) is the plan Completion Report v2 addendum and not subject to code review.
- **Diff basis verified**: `git diff 8eec12e5..bab531a1 --stat` shows 10 files changed, 1094 insertions, 29 deletions. The 4 substantive fix-wave commits are present in the order listed.
- **Head SHA (post-fix)**: `bab531a1` (verified via `git log --oneline 8eec12e5..bab531a1`).
- **Original sections preserved**: Scope, Findings (incl. W-001/W-002 evidence and S-001..S-005), Source Trace, Positive observations, Summary, and Test evidence above are immutable per Assignment. This Revalidation section is the only addition.

### Per-finding disposition

#### R-V150KBED-03 (W-001 — `kb_adopt` transaction wrap) — **RESOLVED**
- **Fix commit**: `2cfbd49e` (250 insertions, 13 deletions across 4 files).
- **Implementation** (verified in `crates/nexus42/src/commands/creator/world/kb.rs` lines 477–509):
  - `pool.begin()` is now called before the insert; `tx.rollback()` (explicit) and `tx.commit()` (success) replace the previous no-transaction flow.
  - Two new tx-aware sibling functions were added to keep the trait contract intact:
    - `SqliteKbStore::insert_key_block_in_tx` (`crates/nexus-local-db/src/kb_store.rs`) — executes the same `validate_canonical_name` + `validate_body` + INSERT as the trait impl, but against `&mut **tx`. The function is explicitly documented with a "**Keep in sync with `KbStore::insert_key_block`**" guard so future drift between the two paths is detectable at review time. The SQLite UNIQUE-constraint (`code == "2067"`) → `KbStoreError::Duplicate` mapping is preserved.
    - `mark_confirmed_in_tx` (`crates/nexus-local-db/src/kb_extract_job.rs`) — same conditional UPDATE `WHERE job_id = ? AND promotion_status = 'pending'` as `mark_confirmed`, but against `&mut **tx`. Same "Keep in sync" guard.
  - **Error-path coverage** is correct and matches the assignment's required fix:
    - On `insert_key_block_in_tx` `Err` → `tx` is dropped via `?` → sqlx auto-rolls back. Inline comment documents this.
    - On `mark_confirmed_in_tx` `Err` → same auto-rollback path. Inline comment documents this.
    - On `mark_confirmed_in_tx` `Ok(false)` (race) → explicit `tx.rollback().await` is called and best-effort logged via `tracing::error!`. The orphan KeyBlock insert is undone before the error surfaces.
  - The misleading "KeyBlock was not duplicated" message has been replaced with "The transaction was rolled back; no orphan row created." — directly addresses S-004 from the original wave as a beneficial side-effect.
- **Regression test** (verified in `crates/nexus42/tests/world_kb_promotion_cli.rs`, new test `kb_adopt_failure_rolls_back_insert`):
  - Pre-flips the candidate via `mark_confirmed(&pool, ...)` to simulate a race winner.
  - Replicates the `kb_adopt` tx boundary verbatim (`begin → insert_key_block_in_tx → mark_confirmed_in_tx → rollback`) because `kb_adopt` itself would reject the pre-flipped row at `load_pending_candidate` (the first-failure path is already covered by the pre-existing `double_adopt_is_rejected` test).
  - Asserts the core invariant: after rollback, **no orphan `KeyBlock` is persisted** (uses `SqliteKbStore::list_by_world` to scan `kb_key_blocks` and asserts `canonical_name != "Race Candidate"` for all rows).
  - Also asserts the race winner's `confirmed` state on the candidate is preserved (no over-rotation).
- **Test evidence (re-run in this re-review)**:
  - `cargo test -p nexus42 --test world_kb_promotion_cli` → **11 passed, 0 failed** (was 8 in wave-1; +1 new test, +2 R-05 regression tests landed in the same file from the qc1 fix-wave).
  - The new `kb_adopt_failure_rolls_back_insert` test passes.
- **Verdict on R-03**: The fix is complete, the test is well-designed (real tx boundary replication, not a mock), and the implementation preserves the trait contract. The duplication of validation+INSERT logic between `insert_key_block` and `insert_key_block_in_tx` is explicitly documented as a "keep in sync" contract — a future refactor could extract the common path, but that is out of scope for this targeted fix-wave and is correctly flagged as Suggestion-level in the original report (S-005 is partially addressed by the new test). **No remaining qc3 concerns.**

#### R-V150KBED-04 (W-002 — index column rename) — **RESOLVED**
- **Fix commit**: `02cc52d5` (84 insertions, 5 deletions across 2 files).
- **Implementation** (verified in `crates/nexus-local-db/migrations/202606180002_kb_extract_jobs_extend.sql` lines 42–64):
  - Old unused index `idx_kb_extract_jobs_promotion_status_work` on `(promotion_status, work_id)` is dropped via `DROP INDEX IF EXISTS`.
  - New index `idx_kb_extract_jobs_promotion_status_world` on `(promotion_status, world_id, created_at)` is created via `CREATE INDEX IF NOT EXISTS`.
  - Column choice matches `list_pending_for_world`'s actual filter (`WHERE world_id = ? AND promotion_status = 'pending' ORDER BY created_at ASC LIMIT N`). The leading `(promotion_status, world_id)` covers the equality filters, and `created_at` is the trailing order column for an index-ordered scan with no filesort — the original wave-1 W-002 fix recommendation is faithfully implemented.
  - Migration comment is **fully rewritten** to reflect the actual query path (verbatim SQL, function reference, spec section §5.5.2) and to document the R-V150KBED-04 rationale (old index unused for the documented path, drop+replace with world_id-covering index).
  - The comment also notes that `is_idempotent` (keyed on `work_id + canonical_name_guess`) still benefits from the leading `promotion_status` column, and that a dedicated `(promotion_status, work_id, canonical_name_guess)` index can be added later if the table grows beyond ~100k rows — this is a thoughtful, future-proofing note.
- **Regression test** (verified in `crates/nexus-local-db/tests/kb_extract_jobs_migration.rs`, new test `pending_list_uses_world_id_covering_index`):
  - Uses `EXPLAIN QUERY PLAN` to assert the planner picks the new `idx_kb_extract_jobs_promotion_status_world` index and **does NOT** reference the legacy `idx_kb_extract_jobs_promotion_status_work` index.
  - Mirrors the `list_pending_for_world` query shape verbatim.
  - Hermetic: uses `fresh_pool()` and seeds one pending row for planner stats.
  - SAFETY comment on the static `EXPLAIN QUERY PLAN` SQL mirror — appropriate per `nexus-local-db` AGENTS.md compile-time-query rule.
- **Test evidence (re-run in this re-review)**:
  - `cargo test -p nexus-local-db --test kb_extract_jobs_migration` → **8 passed, 0 failed** (was 7 in wave-1; +1 new test).
  - The new `pending_list_uses_world_id_covering_index` test passes.
- **Verdict on R-04**: The fix is complete, the column choice is correct, the comment is now self-documenting, and the EXPLAIN-based test is deterministic on SQLite's planner. **No remaining qc3 concerns.**

#### R-V150KBED-05 (qc1's finding — reject log path) — **OUT OF SCOPE**
- This finding belongs to qc1's initial-wave review (reject log path uses work_ref not work_id). The fix is in the same fix-wave range (`125533a8`) but is not a qc3 concern (not performance + reliability). Flagged for completeness only.

### Static analysis & format check (re-run in this re-review)
- `cargo clippy --all -- -D warnings` → **clean** (CI-equivalent command; full workspace, not just the touched crates).
- `cargo +nightly fmt --all --check` → **clean** (nightly required per `AGENTS.md` to honor the `.rustfmt.toml` `ignore` field for `crates/nexus-contracts/src/generated/`).
- No new warnings introduced by the fix-wave; no `#[allow(...)]` suppressions added without justification.

### Open Suggestions from wave 1 (still open, non-blocking)
The original wave 1 raised 5 Suggestions (S-001 through S-005) that were explicitly **non-blocking** in the wave-1 verdict. The fix-wave scope was limited to R-03 + R-04 (the two Warnings). For traceability, the open Suggestions are:
- **S-001** — `existing_canonical_names` silent-zero on read failure (operator visibility).
- **S-002** — Rejected log directory `Logs/kb/rejected/` unbounded retention.
- **S-003** — `KbStoreError::Duplicate` from adopt surfaces generic message (409 mapping).
- **S-004** — Misleading "KeyBlock was not duplicated" message (effectively addressed as a side-effect of R-03 — message is now "The transaction was rolled back; no orphan row created.").
- **S-005** — Test coverage gap for transaction-boundary race (addressed by R-03's new `kb_adopt_failure_rolls_back_insert` test).

These are **Suggestion-level** (non-blocking) and remain in the qc3 open-issues set for future planning. Per `mstar-review-qc` gate rules, unresolved Suggestions alone do **not** block an `Approve` verdict.

### Summary

| Wave | Critical | Warning | Suggestion | Verdict |
|------|----------|---------|------------|---------|
| Wave 1 (initial) | 0 | 2 (W-001, W-002) | 5 | Request Changes |
| Wave 2 (this re-review) | 0 | 0 (R-03 + R-04 RESOLVED) | 5 (unchanged, non-blocking) | **Approve** |

**Verdict (this re-review)**: **Approve**

Per `mstar-review-qc` gate: "无 `Critical` / `Warning` (未解决项) 时，方可 `Approve`." Both blocking Warnings from the initial wave are resolved with surgical, well-tested fixes. The fix-wave introduces no new findings in qc3's scope (performance + reliability). The two remaining concerns — function-level duplication of the INSERT/validation logic between `insert_key_block` and `insert_key_block_in_tx` — are explicitly guarded by "Keep in sync" documentation and are out of scope for this targeted re-review; they remain Suggestion-level and should be tracked in `residual_findings` if PM/QA elect to keep the open Suggestions on the books.
