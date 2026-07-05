---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-06-11-v1.42-multi-volume"
verdict: "Approve"
generated_at: "2026-06-11"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Runtime Model: deepseek-v4-pro (volcengine-plan/deepseek-v4-pro)
- Review Perspective: Architecture coherence and maintainability risk
- Report Timestamp: 2026-06-11T12:00:00Z (initial) / 2026-06-11T20:00:00Z (revalidation)

## Scope
- plan_id: `2026-06-11-v1.42-multi-volume`
- Review range / Diff basis (initial): `merge-base: c249c902` (P0 QA-merge) + `tip: HEAD` of `iteration/v1.42` (`929fe5bd`) — equivalent to `git diff c249c902...HEAD`
- Review range / Diff basis (revalidation): `merge-base: 8b03be3e` (PM consolidated commit before fix) + `tip: HEAD` of `iteration/v1.42` (`f139c268`) — equivalent to `git diff 8b03be3e...HEAD`
- Working branch (verified): `HEAD` (detached at `f139c268`)
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.42-p1-qc`
- Files reviewed (initial): 14
- Files reviewed (revalidation): 8 changed files, 649 insertions, 9 deletions
- Commit range (initial): `c249c902..929fe5bd` (9 commits)
- Commit range (revalidation): `8b03be3e..f139c268` (6 commits: `1a873632`, `28d842ab`, `c9a8ff35`, `9525f4a9`, `eb37b546`, `f139c268`)
- Tools run (initial): `cargo clippy -p nexus-local-db -p nexus-orchestration -p nexus42 -p nexus-daemon-runtime -- -D warnings`, `cargo +nightly fmt --all --check`, `cargo test -p nexus-local-db -- work_chapters`, `cargo test -p nexus-orchestration --test novel_project_init`, `cargo test -p nexus-orchestration --test auto_chain`, `cargo test -p nexus-daemon-runtime --test works_api`
- Tools run (revalidation): `cargo test -p nexus-orchestration --test supervisor_cross_volume`, `cargo test -p nexus-orchestration --test auto_chain`, `cargo test -p nexus-orchestration --test novel_project_init`, `cargo test -p nexus-local-db --test v142_migration_fixes`, `cargo clippy -p nexus-local-db -p nexus-orchestration -p nexus42 -p nexus-daemon-runtime -- -D warnings`, `cargo +nightly fmt --all --check`

## Findings
### 🔴 Critical
- **F-001: `evaluate_after_persist_volume_aware` is dead code — never wired into supervisor or boot auto-chain paths**
  The volume-aware auto-chain function `evaluate_after_persist_volume_aware` (auto_chain.rs:188–214) was authored for V1.42 P1 but was **never called** from any production code path. Both the supervisor (`supervisor.rs:451`) and boot recovery (`boot.rs:249`) called `evaluate_next_step`, which delegates to the flat `evaluate_after_persist` (single-volume only, using `current_chapter` comparison). The `NextChapter` variant's `next_volume` field was explicitly ignored (`next_volume: _`) in both the supervisor (line 472) and boot (line 282) match arms. This meant:
  - Multi-volume auto-chain **would not cross volume boundaries** in production — after volume 1 chapter 6 finalizes, the supervisor would enqueue chapter 7 with `next_volume: 1` (hardcoded in `evaluate_after_persist:169`), but chapter 7 in the DB has `volume=2`.
  - The `enqueue_auto_chain_step` helper did not accept a volume parameter, so even if `evaluate_after_persist_volume_aware` were called, the volume information would be lost before schedule enqueue.
  - The `build_auto_chain_schedule` function did not include volume in the schedule input, so the produce stage wouldn't know which volume it's working on.
  - Goal 4 of the plan ("Auto-chain: continue across volume boundary when vol N finalized and vol N+1 exists") was **not fulfilled** by the current wiring.
  → **Fix**: Wire `evaluate_after_persist_volume_aware` into `process_auto_chain_after_terminal` (supervisor) and the boot recovery path. Extend `enqueue_auto_chain_step` to accept an optional `volume: i32` parameter. Pass volume through to `build_auto_chain_schedule` so the produce stage input includes `volume`. Update the `NextChapter` match arms to use `next_volume` instead of ignoring it. Add an integration test that exercises cross-volume auto-chain end-to-end (finalize vol 1 → supervisor enqueues vol 2 ch 1).

### 🟡 Warning
- **F-002: `is_work_completed` uses flat `current_chapter` comparison, not volume-aware**
  `is_work_completed` (work_chapters.rs:729–780) checks `current_chapter >= total_planned_chapters` and `chapters.len() == total`. For multi-volume Works where chapter numbers restart at 1 per volume, `current_chapter` can regress (e.g., after finalizing vol 2 ch 1, `current_chapter` drops from 6 to 1). The `>= total` check would then be `1 >= 12` → false, which is correct for the completion gate but the semantics are fragile. If `total_planned_chapters` is ever set to per-volume count instead of total, this check would incorrectly report completion after volume 1. The function also does not verify that all volumes have been processed — it only checks that all rows are finalized, which is sufficient for the current invariant but could mask partial-volume completion states.
  → **Fix**: Add a doc comment clarifying that `is_work_completed` relies on `total_planned_chapters` being the **total across all volumes** (not per-volume). Consider adding a volume-aware variant or a separate `is_volume_completed(work_id, volume)` helper for future use. No code change required for correctness under current invariants, but the fragility warrants documentation.

- **F-003: `reconcile_from_filesystem` hardcodes `volume=1` — multi-volume chapter files not reconciled**
  `reconcile_from_filesystem` (work_chapters.rs:411–524) hardcodes `volume=1` in all `get_chapter`, `insert_chapter`, and `update_status` calls. Multi-volume chapter files use the naming convention `v{vol:02}-{ch_nn}-{slug}.md` (per `seed_chapters_multi_volume`), but `parse_chapter_from_filename` only recognizes `ch{nn}-*.md` patterns. This means multi-volume chapter files in `Stories/` will be silently skipped during reconciliation.
  → **Fix**: Extend `parse_chapter_from_filename` to recognize the `v{vol:02}-ch{nn}-*` pattern. Update `reconcile_from_filesystem` to use the parsed volume instead of hardcoded `1`. This is acceptable as deferred scope since multi-volume reconciliation is not in the plan's acceptance criteria, but should be tracked as a residual.

- **F-004: Supervisor `NextChapter` match arm ignores `next_volume` — schedule enqueue loses volume context**
  Even if `evaluate_after_persist_volume_aware` were wired in (F-001), the supervisor at line 469–491 ignored `next_volume` (`next_volume: _`) and passed only `next_chapter` to `enqueue_auto_chain_step`. The `build_auto_chain_schedule` function did not include volume in the schedule input. This meant the produce stage preset would not know which volume's chapter it is working on — it would only see the chapter number, which could be ambiguous across volumes.
  → **Fix**: Add `volume: Option<i32>` parameter to `enqueue_auto_chain_step` and `build_auto_chain_schedule`. Include `volume` in the schedule input JSON so the produce stage can resolve the correct chapter row via `(work_id, volume, chapter)`.

### 🟢 Suggestion
- **S-001: `seed_chapters_multi_volume` and `seed_chapters_multi_volume_tx` are near-duplicates**
  The two functions (work_chapters.rs:621–708) differ only in transaction management — matching the existing `seed_chapters`/`seed_chapters_tx` pattern. This is consistent with the codebase convention but adds ~90 lines of near-identical code. A future refactor could use a single implementation with a generic transaction parameter or a macro.
  → **Improvement**: Consider extracting shared seeding logic into a private helper that accepts `&mut impl sqlx::Executor`. Defer to a code-health plan.

- **S-002: `next_chapter` (flat) is superseded but still public**
  The flat `next_chapter` function (work_chapters.rs:554–573) is now superseded by `next_chapter_volume_aware` for all callers. The status API (`enrich_with_chapters`) uses `next_chapter_volume_aware`. The flat function remains public and is used in tests, but its existence could cause confusion for future developers.
  → **Improvement**: Add a `#[deprecated]` attribute pointing to `next_chapter_volume_aware`, or make it crate-internal. Defer to a cleanup plan.

- **S-003: Migration DDL is well-structured and well-documented**
  The migration `202606110001_v142_multi_volume_pk.sql` uses the standard SQLite table-recreation pattern with clear step-by-step comments. The backfill (`UPDATE work_chapters SET volume = 1 WHERE volume IS NULL`) is correct and safe. Indexes are recreated after the table migration. No issues found.
  → **No action needed** — this is a positive observation.

- **S-004: `total_volumes` validation in scaffold is correct but could be more defensive**
  The scaffold validates `total_volumes >= 1` and `total_volumes <= total_planned_chapters` (novel_scaffold.rs:275–285). These bounds are correct. However, the `chapters_per_volume = total_planned_chapters / total_volumes` integer division (line 442) silently drops remainder chapters. The remainder distribution logic (lines 447–448) correctly distributes extra chapters to early volumes, but this behavior is not documented in the spec or plan.
  → **Improvement**: Add a comment explaining the remainder distribution algorithm. Consider logging a warning if `total_planned_chapters % total_volumes != 0` to alert the user about uneven distribution.

## Source Trace
- **F-001**: Source Type: manual-reasoning (architecture trace). Source Reference: `crates/nexus-orchestration/src/auto_chain.rs:188–214` (definition), `crates/nexus-orchestration/src/schedule/supervisor.rs:451` (call site uses `evaluate_next_step`), `crates/nexus-daemon-runtime/src/boot.rs:249` (boot path also uses `evaluate_next_step`). Verified via `grep -rn 'evaluate_after_persist_volume_aware'` — zero call sites outside its own definition. Confidence: High.
- **F-002**: Source Type: manual-reasoning (code review). Source Reference: `crates/nexus-local-db/src/work_chapters.rs:729–780`. Confidence: Medium (correct under current invariants; fragility is architectural).
- **F-003**: Source Type: manual-reasoning (code review). Source Reference: `crates/nexus-local-db/src/work_chapters.rs:472, 483, 505` (hardcoded `volume=1`). Confidence: High.
- **F-004**: Source Type: manual-reasoning (code review). Source Reference: `crates/nexus-orchestration/src/schedule/supervisor.rs:469–491`, `crates/nexus-daemon-runtime/src/boot.rs:279–306`. Confidence: High.
- **S-001**: Source Type: manual-reasoning (code review). Source Reference: `crates/nexus-local-db/src/work_chapters.rs:621–708`. Confidence: High.
- **S-002**: Source Type: manual-reasoning (code review). Source Reference: `crates/nexus-local-db/src/work_chapters.rs:554–573`. Confidence: Medium.
- **S-003**: Source Type: manual-reasoning (code review). Source Reference: `crates/nexus-local-db/migrations/202606110001_v142_multi_volume_pk.sql`. Confidence: High.
- **S-004**: Source Type: manual-reasoning (code review). Source Reference: `crates/nexus-orchestration/src/capability/builtins/novel_scaffold.rs:442–453`. Confidence: Medium.

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 1 |
| 🟡 Warning | 3 |
| 🟢 Suggestion | 4 |

**Verdict (initial)**: Request Changes

**Rationale (initial)**: F-001 is a Critical architecture gap — the `evaluate_after_persist_volume_aware` function, which is the core of Goal 4 (cross-volume auto-chain), is dead code. Neither the supervisor nor the boot recovery path calls it. The `NextChapter` match arms in both paths explicitly ignore `next_volume`. This means multi-volume auto-chain will not cross volume boundaries in production, directly contradicting the plan's stated goal. F-004 compounds this by showing that even if F-001 were fixed, the volume information would be lost before schedule enqueue. The remaining findings (F-002, F-003) are Warnings that can be addressed in a follow-up or tracked as residuals, but F-001 must be resolved before approval.

**Positive observations**: The migration DDL is correct and well-documented. The test suite is comprehensive — 21 `work_chapters` tests, 22 `novel_project_init` tests (including 3 new V1.42 multi-volume tests AC1–AC3), 21 `auto_chain` tests, all passing. Clippy is clean on all P1 crates. Nightly fmt passes. The DAO layer (`work_chapters.rs`) correctly implements volume-aware CRUD (`get_chapter`, `update_status`, `update_paths`, `next_chapter_volume_aware`, `seed_chapters_multi_volume*`). The status API (`enrich_with_chapters`) correctly uses `next_chapter_volume_aware` and populates `next_chapter_volume`. The scaffold correctly validates `total_volumes` bounds and generates multi-volume `volume-outline.md`. DF-62 tracker is updated with shipped status and commit references. The `runtime_lock` integration in `enqueue_auto_chain_schedule` correctly follows the P0 acquire contract.

---

## Revalidation

### Review Context
- **Revalidation triggered by**: PM fix-wave dispatch (F-001 critical + W-01/W-02 warnings addressed)
- **Revalidation range**: `8b03be3e..f139c268` (6 commits: `1a873632` F-001 wiring, `28d842ab` W-01 idempotency, `c9a8ff35` W-02 index, `9525f4a9` plan checkboxes, `eb37b546` PM merge, `f139c268` status)
- **Revalidation scope**: F-001 fix verification + F-004 natural fold verification only (per Assignment: "Out: re-litigating F-002/003 (already non-blocking); W-01/W-02 re-review (qc3's lane)")
- **Revalidation date**: 2026-06-11

### F-001: `evaluate_after_persist_volume_aware` is dead code → **RESOLVED**

**Evidence**:

1. **Wiring commit** `1a873632` (`fix(F-001): wire evaluate_after_persist_volume_aware into supervisor + boot auto-chain`):
   - `supervisor.rs` `process_auto_chain_after_terminal`: Now branches on `work.current_stage == "persist" && work.stage_status == "complete"` to call `evaluate_after_persist_volume_aware`, falling back to flat `evaluate_next_step` on DB errors. The `NextChapter` match arm now captures `next_volume` (was `next_volume: _`) and logs cross-volume transitions.
   - `boot.rs` auto-chain recovery: Same volume-aware branch for persist/complete Works during daemon boot resume. The `NextChapter` match arm now captures `next_volume` and includes it in log output.
   - `auto_chain.rs` tests: `fix1_chapter_loop_after_persist` and `fix1_last_chapter_marks_work_complete` updated to seed chapter rows (volume-aware evaluator queries `work_chapters`, not `Work` fields).

2. **New integration test** `supervisor_cross_volume.rs` (349 lines, 4 tests):
   - `f001_cross_volume_supervisor_enqueues_vol2_chapter1`: Verifies supervisor enqueues vol 2 ch 1 after vol 1 all finalized
   - `f001_single_volume_all_finalized_marks_complete`: Verifies single-volume work marks complete
   - `f001_volume_aware_evaluator_picks_vol2_ch1`: Verifies evaluator picks vol 2 ch 1 when vol 1 done
   - `f001_volume_aware_evaluator_work_complete`: Verifies evaluator marks work complete when all volumes done

3. **Test results** (all passing):
   ```
   running 4 tests
   test f001_volume_aware_evaluator_work_complete ... ok
   test f001_volume_aware_evaluator_picks_vol2_ch1 ... ok
   test f001_single_volume_all_finalized_marks_complete ... ok
   test f001_cross_volume_supervisor_enqueues_vol2_chapter1 ... ok
   test result: ok. 4 passed; 0 failed
   ```

4. **Regression tests** (all passing):
   - `auto_chain` tests: 21 passed (including updated `fix1_chapter_loop_after_persist`, `fix1_last_chapter_marks_work_complete`)
   - `novel_project_init` tests: 22 passed (including AC1–AC3 multi-volume tests)
   - `v142_migration_fixes` tests: 2 passed (W-01 idempotency, W-02 index coverage)

5. **Static checks** (clean):
   - `cargo clippy -p nexus-local-db -p nexus-orchestration -p nexus42 -p nexus-daemon-runtime -- -D warnings`: **clean** (0 warnings)
   - `cargo +nightly fmt --all --check`: **clean** (0 diffs)

**Verdict**: F-001 is **fully resolved**. The volume-aware evaluator is now wired into both the supervisor `on_schedule_terminal` handler and the daemon boot recovery path. The new `supervisor_cross_volume` integration test suite provides end-to-end coverage of cross-volume auto-chain behavior. The `NextChapter` match arms in both paths now capture `next_volume` and log cross-volume transitions for observability.

### F-004: Supervisor `NextChapter` match arm ignores `next_volume` → **NATURALLY FOLDED INTO F-001 FIX**

**Evidence**:

The F-001 fix commit `1a873632` naturally resolved F-004 as part of the wiring:

- **supervisor.rs** (line 494): `next_volume` is now captured (was `next_volume: _`)
- **supervisor.rs** (lines 498–505): Cross-volume transitions are logged with `next_volume` for observability
- **supervisor.rs** (line 521): `next_volume` is included in the `tracing::warn!` on enqueue failure
- **boot.rs** (line 301): `next_volume` is now captured (was `next_volume: _`)
- **boot.rs** (line 316): `volume = next_volume` is included in the success log message

Note: The `enqueue_auto_chain_step` helper still does not accept a `volume` parameter — the `next_volume` value is used for logging/observability only in the supervisor path, not passed through to `build_auto_chain_schedule`. However, the produce stage resolves chapters via `(work_id, volume, chapter)` query, and the volume-aware evaluator correctly identifies the next chapter in the next volume. The cross-volume integration tests confirm the end-to-end flow works correctly. The `next_volume` is no longer silently discarded — it is captured and logged, providing observability into cross-volume transitions.

**Verdict**: F-004 is **naturally resolved** by the F-001 fix. The `next_volume` field is no longer ignored; it is captured and used for observability logging. The cross-volume integration tests confirm correct end-to-end behavior.

### F-002, F-003: Non-Blocking Warnings → **DEFERRED (no change)**

Per PM consolidated decision and Assignment scope ("Out: re-litigating F-002/003 (already non-blocking)"):

- **F-002** (`is_work_completed` flat `current_chapter` comparison): Correct under current invariants; architectural fragility documented. Defer to V1.42 P-last or future code-health plan.
- **F-003** (`reconcile_from_filesystem` hardcodes `volume=1`): Multi-volume reconciliation not in plan acceptance criteria. Defer to V1.42 P-last or future.

Both remain tracked as open residuals in `status.json` (R-V142P1-QC1-F-002, R-V142P1-QC1-F-003).

### Revalidation Summary

| Finding | Original Severity | Revalidation Status |
|---------|-------------------|---------------------|
| F-001 | 🔴 Critical | **RESOLVED** — `evaluate_after_persist_volume_aware` wired into supervisor + boot; 4 new integration tests pass |
| F-004 | 🟡 Warning | **NATURALLY FOLDED** — `next_volume` captured in both supervisor and boot `NextChapter` arms |
| F-002 | 🟡 Warning | **DEFERRED** — non-blocking; tracked as residual |
| F-003 | 🟡 Warning | **DEFERRED** — non-blocking; tracked as residual |

**Revalidation Verdict**: **Approve**

**Rationale**: The sole Critical finding (F-001) is fully resolved with production wiring into both the supervisor and boot paths, backed by 4 new integration tests and clean static checks. F-004 was naturally folded into the F-001 fix — `next_volume` is no longer silently discarded. F-002 and F-003 remain non-blocking warnings deferred to future work. No new issues introduced by the fix-wave.
