---
report_kind: qc-consolidated
plan_id: 2026-06-10-v1.41-selection-pool
verdict: Approve (after fix-wave re-review)
generated_at: 2026-06-11T01:30:00+08:00
initial_review_range: "merge-base: 55689706 → tip: 57f573ad"
fix_wave_tip: 97470073
final_review_range: "merge-base: 55689706 → tip: 97470073"
working_branch_verified: iteration/v1.41
review_cwd_verified: /Users/bibi/workspace/organizations/42ch/nexus
reviewers:
  - "@qc-specialist (1, architecture-coherence-maintainability) — initial Request Changes → fix-wave re-review Approve (da5b3ab5)"
  - "@qc-specialist-2 (2, security-correctness) — initial Request Changes → fix-wave re-review Approve (b1b3e690)"
  - "@qc-specialist-3 (3, performance-reliability) — initial Request Changes → fix-wave re-review Approve (55847038)"
plan_review:
  reviewer: "@project-manager"
  scope: "Plan-vs-spec re-read + plan-vs-implementation completion audit"
  notes: "Plan §5 T1 said 'tighten spec if needed during implement'. Implementer added one clarifying sentence to novel-work-pool.md but did not amend cli-spec.md or the path layout. User 2026-06-10 also flagged the path itself is conceptually wrong (Pool is creator-scoped, not Work-scoped). Spec gaps to fix in this wave:"
  spec_gaps:
    - "novel-work-pool.md §3.1 + §3.3 still hard-codes Works/_pool/灵感池/ path; spec must be amended to {workspace}/Pool/Ideas/."
    - "cli-spec.md §6.2D/H does not mention the path at all but the deferred-tracker references Works/_pool/灵感池/; tracker should also be updated to match."
    - "novel-work-pool.md §5 (CLI surface) does not document inspiration promote's --idea semantics (CLI currently allows --idea as optional; spec implies MD body should drive it)."
---

# QC Consolidated Gate — V1.41 P1 (DF-61 selection pool + inspiration)

## Verdict (final, after fix-wave re-review)
**Approve** — 0 Critical, 13 Warning blockers addressed in fix wave (9 actionable + 4 plan-review; user 2026-06-10 path correction included). All 3 QC reviewers returned Approve after targeted re-review.

## Roll-up (initial review)

| Reviewer | Verdict (initial) | Critical | Warning | Suggestion |
|----------|-------------------|----------|---------|------------|
| @qc-specialist (1, architecture) | Request Changes | 0 | 3 | 3 |
| @qc-specialist-2 (2, security+correctness) | Request Changes | 0 | 4 | 4 |
| @qc-specialist-3 (3, performance+reliability) | Request Changes | 0 | 6 | 4 |
| PM plan re-review | Request Changes | 0 | 3 (spec gaps + 1 user path) | — |
| **Consolidated (initial)** | **Request Changes** | **0** | **16 (13 + 3 plan)** | **11** |

## Roll-up (after fix-wave re-review)

| Reviewer | Verdict (re-review) | Commit | Disposition |
|----------|---------------------|--------|-------------|
| @qc-specialist (1) | **Approve** | da5b3ab5 | W-1/W-2/W-3 all resolved; S-2 resolved via bonus `From<T>` impls; S-1/S-3 deferred to V1.42 |
| @qc-specialist-2 (2) | **Approve** | b1b3e690 | W-01/W-02/W-03/W-04 all resolved; S-02 resolved (creator_id in DAOs) |
| @qc-specialist-3 (3) | **Approve** | 55847038 | F-001/F-002/F-003/F-005/F-006 all resolved; F-004 stays out-of-scope (P-last) |
| **Consolidated (final)** | **Approve** | — | All blockers closed; 19 residuals registered; ready for QA verification |

## Consolidated findings (deduped; mapped to fix wave)

### 1. Inspiration MD scaffold path is wrong (user feedback + QC1 W-3 / QC3 F-001) — **HIGHEST PRIORITY**

The current path is `Works/_pool/灵感池/<slug>.md` (inspiration_items.rs:140). This is a category error — the pool is **creator-scoped**, not Work-scoped. The correct path is `{workspace_root}/Pool/Ideas/<slug>.md` where `workspace_root = ~/.nexus42/creators/<id>/workspaces/<slug>/` (i.e. the operational workspace). When a creator switches between Works, they keep the same pool. When they create a new creator (multi-creator), the pool is per-creator.

**Action items**:
- (a) `inspiration_items.rs`: change `format!("Works/_pool/灵感池/{slug}.md")` → `format!("Pool/Ideas/{slug}.md")`. (`rel_path` is workspace-relative; the workspace root is the operational workspace dir passed by the caller.)
- (b) `inspiration_items.rs`: document the `workspace_dir` parameter as **must be the resolved operational workspace path** in the function's doc comment.
- (c) `novel-work-pool.md` §3.1 + §3.3: amend path. Remove the `Works/` prefix. Add an explicit note that the path is relative to the operational workspace root.
- (d) `deferred-features-cross-version-tracker.md` DF-61 row: update the "in-scope" line "DB SSOT + `Works/_pool/灵感池/`" → "DB SSOT + `{workspace}/Pool/Ideas/`".
- (e) `cli-spec.md` §6.2H (or wherever inspiration add is documented): update path.
- (f) **Add a `nexus-home-layout` helper** for the inspiration directory so callers don't have to hand-build the path. New helper: `inspiration_dir(home, creator_id, workspace_slug) -> PathBuf` returning `{workspace_root}/Pool/Ideas/`. Add it to `crates/nexus-home-layout/src/lib.rs` and have the daemon layer use it. This is the canonical place to centralize the layout (per `nexus-home-layout/AGENTS.md` "All crates touching the filesystem must use these helpers — do not hardcode paths.").
- (g) **R-V141P1-N06 superseded** by this fix: when path is routed through `nexus-home-layout` via the new helper, the CWD-relative leak is impossible by construction.
- (h) Update the 9 hermetic tests in `selection_pool.rs` — change the expected `rel_path` to `Pool/Ideas/<slug>.md`. Clean up the test artifacts left over from the previous (broken) path.

### 2. `PoolEntryDto` missing `title` field (QC1 W-1)

`PoolEntryDto` (works.rs:238–245) omits `title` even though the DAO `PoolEntry` struct has it (novel_pool_entries.rs:18–35). CLI `handle_pool_list` (works/mod.rs:536) reads `e.get("title")` and always prints `"?"`.

**Action**: add `pub title: String` to `PoolEntryDto`; populate in all three construction sites (`list_pool` 1331–1341, `promote_pool_entry` 1374–1381, `archive_pool_entry_handler` 1399–1406). Suggestion S-2 (extract `From<PoolEntry> for PoolEntryDto` impl) is a free cleanup — do it in the same commit.

### 3. Cross-creator archive/promote (QC1 W-2 / QC2 W-01) — **security**

`archive_pool_entry_handler` and `archive_inspiration_handler` read `_creator_id` from config (prefix `_` = unused!) and pass only `entry_id`/`item_id` to the DAO. The DAO functions `archive_pool_entry` (novel_pool_entries.rs:159) and `archive_inspiration` (inspiration_items.rs:219) accept only `entry_id`/`item_id`. Creator A can archive creator B's rows. **Same pattern in `promote_inspiration_handler`** — fetches inspiration by `item_id` only, then creates a Work + pool entry attributed to the *current session's* `creator_id`.

**Action**:
- Add `creator_id: &str` parameter to `archive_pool_entry`, `archive_inspiration`, and `get_inspiration` (or pre-fetch with creator check in handler). Add `AND creator_id = ?` to UPDATEs.
- In `promote_inspiration_handler`: verify `item.creator_id == active_creator_id` before creating the Work; return 403/404 on mismatch.
- In the existing `promote_pool_entry` handler, also tighten to verify the entry's `creator_id` matches the active session (the current `works::get_work(..., &creator_id, ...)` pattern is correct for the Work side; add the analogous pool-entry-side check).
- Add hermetic test `test_archive_pool_rejects_cross_creator` + `test_archive_inspiration_rejects_cross_creator` + `test_promote_inspiration_rejects_cross_creator`.

### 4. `promote_inspiration` not atomic (QC2 W-02 / QC3 F-002) — **correctness**

3 sequential DB calls: `works::create_work` → `promote_to_active` (pool) → `promote_inspiration` (item). If step 3 fails, Work + active pool row exist with no `promoted_work_id` on the inspiration item. User sees ghost Work.

**Action**:
- Either: (a) wrap the three writes in a single `Transaction` and roll back if any step fails, OR
- (b) on step 3 failure, delete the Work and pool row (best-effort cleanup) and return 500 with structured error.
- Recommended: (a) — sqlx supports `pool.begin().await` then commit/rollback. Add a hermetic test that drives step-3-failure and asserts no Work + no pool row + no promoted item.

### 5. `mark_work_completed` pool update is best-effort (QC2 W-03 / QC3 F-003) — **correctness**

`works::patch_work` is awaited first, then `mark_pool_entry_completed_for_work` is called afterwards and only warns on error. Pool row can remain `active` while Work is `completed` + locked. The assignment's "3 writes recovery story" question is essentially unanswered by the current code.

**Action**:
- Move the pool update into the same transaction as the Work patch (this requires plumbing a `&mut Transaction` through `mark_work_completed`). If transaction scope is too invasive for this slice, the **minimum acceptable** is: if `mark_pool_entry_completed_for_work` returns Err, log a `tracing::error!` AND set `works.completion_locked_at = NULL` so the user can retry via `works status` + manual `pool mark_completed` command (CLI follow-up acceptable). Make sure the supervisor's "skip on completion_locked_at" check then re-tries the pool update on next tick.

### 6. List queries unbounded (QC2 W-04 / QC3 S-1)

`list_pool` and `list_inspiration` have no `LIMIT`/`OFFSET` in DAO, handler, or CLI. At 10,000+ entries this is a memory bomb.

**Action**:
- Add `limit: Option<u32>` and `offset: Option<u32>` parameters to `list_pool_entries` and `list_inspiration` in the DAOs. Default `limit = 200` if absent.
- Add `total_count` to the response so the CLI can show "showing 1–200 of N".
- CLI: add `--limit`/`--offset` flags. Default `--limit 200`.
- Add a hermetic test that inserts 250 entries and asserts `list` returns 200 + total_count=250.

### 7. Missing covering index for status-filtered list (QC3 F-006)

`inspiration_items` has `idx(creator_id)` and `idx(creator_id, rel_path)`. `list_inspiration(creator_id, status)` doesn't use a status-aware index. Same for `novel_pool_entries` non-`active` queries.

**Action**:
- Add a new migration: `crates/nexus-local-db/migrations/202606100004_v141_pool_inspiration_status_index.sql`:
  ```sql
  CREATE INDEX idx_novel_pool_entries_creator_status ON novel_pool_entries(creator_id, status);
  CREATE INDEX idx_inspiration_items_creator_status ON inspiration_items(creator_id, status);
  ```
- Refresh `.sqlx/` offline cache if sqlx-cli is available; otherwise document as residual.
- Add a hermetic test that asserts the new indexes exist (or that the query plan uses them via EXPLAIN — if EXPLAIN is feasible in the harness).

### 8. Sync I/O in async paths (QC3 F-005) — **reliability**

`create_inspiration_with_scaffold` (`std::fs::write` + `rename`) and `write_completion_lock_for_work` (`std::fs::write`) perform blocking disk operations on the async runtime thread.

**Action**:
- Wrap the file I/O in `tokio::task::spawn_blocking(move || { ... })` so the async runtime is not blocked.
- The DAO layer currently uses `sqlx` (async). Keep file I/O out of the DAO if possible — either move it to a small helper that the daemon handler calls via `spawn_blocking`, or have the DAO accept an `async_runtime_handle` parameter and dispatch.
- For P1 minimum: change `add_inspiration_handler` to `tokio::task::spawn_blocking` the file write; leave completion lock for P-last.
- Add a hermetic test that asserts the file write is non-blocking on the runtime (you can assert the runtime was not blocked by measuring the concurrent response time, but a simple test that just calls the handler and asserts correctness is enough).

### 9. Spec gaps (plan review + user feedback)

- `novel-work-pool.md` §3.1, §3.3, §5 — amend path (see Item 1). Add explicit "MD scaffold is at `{workspace_root}/Pool/Ideas/<slug>.md`" to §3.1; replace path in §3.3 + §5.
- `cli-spec.md` §6.2H — same path amendment.
- `novel-work-pool.md` §5 — document `inspiration promote --idea` behavior: if `--idea` is supplied, the new Work's idea = `--idea`; otherwise the new Work's idea = first non-empty line of the MD body; if MD body is empty, the Work is created with idea = `Untitled from <item title>`. This makes the behavior explicit and CLI-help-testable.
- `deferred-features-cross-version-tracker.md` DF-61 row — path + "P1 Implemented (pending QC/QA) → Shipped" status (after fix wave + re-review).

## Out-of-scope (confirmed pre-existing, NOT in P1 fix wave)

- **`db::pool::tests::pool_config_from_env_reads_valid_values` flake (8 == 4)** — V1.40 V140P0.5 era. Pre-existing; not introduced by P1. Will not regress-fix in P1.
- **`repeated_sweeps_remain_stable` flake** — R-V139P0-W-B root cause (RVM ID collision, same pattern as the ACH fix in V1.40 P4 T2). Pre-existing since V1.39 P4. V1.41 P-last has a hygiene target that covers it (R-V141P0-N02 for the ACH-style counter fix; the RVM mirror would be a fresh fix in P-last).
- `R-V141P1-N02` (db/pool.rs flake): accept-with-fix → defer to P-last.
- `R-V141P1-N10` (master_decision_timeout flake): accept-with-fix → defer to P-last.

## Suggested residuals (write to `status.json` after fix wave + re-review)

PM will register the following in `residual_findings[2026-06-10-v1.41-selection-pool]` once the re-review passes:

| ID | Severity | Source | Decision | Target |
|----|----------|--------|----------|--------|
| R-V141P1-01 | low | qc1 F-007 (PoolEntryDto title) | accept-with-fix | this fix wave |
| R-V141P1-02 | medium | qc1 F-008 + qc2 W-01 + user 2026-06-10 path | accept-with-fix | this fix wave |
| R-V141P1-03 | medium | qc2 W-02 + qc3 F-002 | accept-with-fix | this fix wave |
| R-V141P1-04 | low | qc2 W-03 + qc3 F-003 | accept-with-fix | this fix wave |
| R-V141P1-05 | low | qc2 W-04 + qc3 S-1 | accept-with-fix | this fix wave |
| R-V141P1-06 | low | qc3 F-005 (sync I/O) | accept-with-fix | this fix wave (inspiration only) + P-last (completion lock) |
| R-V141P1-07 | low | qc3 F-006 (covering index) | accept-with-fix | this fix wave |
| R-V141P1-08 | low | plan re-review spec gaps | accept-with-fix | this fix wave |
| R-V141P1-09 | low | qc2 S-02 (authz at DAO layer) | defer | V1.42 |
| R-V141P1-10 | low | qc2 S-03 (--set-default atomicity) | defer | V1.42 |
| R-V141P1-11 | nit | qc1 S-1 (creator_id DTO exposure) | accept | V1.42 UX |
| R-V141P1-12 | nit | qc1 S-3 (CJK slug → "untitled") | accept | V1.42 UX |
| R-V141P1-13 | nit | qc3 S-2 (slug collision UX) | accept | V1.42 UX |
| R-V141P1-14 | low | qc1 F-004 (cli help polish) | accept | V1.42 |
| R-V141P1-15 | low | qc3 S-3 (observability) | defer | V1.41 P-last |
| R-V141P1-16 | low | `.sqlx/` not refreshed (sqlx-cli unavailable) | defer | V1.41 P-last |
| R-V141P1-17 | low | `db::pool::tests::pool_config_from_env_reads_valid_values` pre-existing flake | accept | backlog |
| R-V141P1-18 | low | `repeated_sweeps_remain_stable` pre-existing flake (R-V139P0-W-B mirror) | accept | V1.41 P-last |
| R-V141P1-19 | nit | `PoolEntryDto`/`InspirationItemDto` construction duplication (qc1 S-2) | accept | V1.42 |

## Targeted re-review plan (after fix wave)

After fix wave, dispatch **targeted re-review** to **all 3 QC reviewers** (N=3 in one turn) — all of them found Warnings:

- **qc-specialist** (N=1, qc-specialist): confirm path, title field, helper added, spec amended.
- **qc-specialist-2** (N=1, qc-specialist-2): confirm cross-creator guard, atomicity, list pagination.
- **qc-specialist-3** (N=1, qc-specialist-3): confirm CWD fix, atomicity, sync I/O wrap, index, list cap.

Reviewer Assignment: `QC re-review: targeted — reviewers: qc-specialist, qc-specialist-2, qc-specialist-3`. Each reviewer updates `qc1.md` / `qc2.md` / `qc3.md` **in place** with `## Revalidation` section. PM consolidates to `qc-consolidated.md` after.

## Summary

| Severity | Count (initial) | Count (after re-review) |
|----------|------------------|------------------------|
| 🔴 Critical | 0 | 0 (all resolved) |
| 🟡 Warning (actionable) | 10 (1 from user feedback) | 0 (all resolved) |
| 🟡 Warning (spec gaps) | 3 | 0 (all resolved via Fix 1 + Fix 9) |
| 🟢 Suggestion | 11 | 11 (forward-looking; tracked in residuals) |
| Out-of-scope pre-existing | 2 | 2 (F-004/R-V141P1-17/18 stay out-of-scope; V1.41 P-last) |

**Initial verdict**: Request Changes
**Final verdict (after fix-wave re-review)**: **Approve**
