---
report_kind: qa_verification
reviewer: qa-engineer
plan_id: 2026-06-18-v1.51-cross-chapter-rescan
verdict: Approve
generated_at: 2026-06-19T03:53:43Z
---

# QA Report — V1.51 T-A P1 Cross-Chapter Rescan

## Summary

**Verdict**: Approve

All 12 acceptance criteria verified. 100% of targeted regression and new-feature tests pass. Full static gates (clippy, fmt) clean. Wire contracts and schemas unchanged. Spec bodies authored and coherent with implementation. R-V150KBED-08 closed with concrete `closure_evidence`. No scope creep (T-A P2 / T-B plans absent from branch). PM clippy fix at `00829432` confirmed clean.

**Verification counts**:
- Targeted cross-chapter tests: 11/11 passed (nexus42 kb_rescan)
- Aggregation unit tests: 8/8 passed (nexus-orchestration quality_loop::aggregate_)
- DB upsert cross-chapter: 1/1 passed (nexus-local-db kb_extract_jobs_upsert)
- V1.50 chapter-scoped regression: 8/8 + 7/7 passed
- T-A P0 LLM pathway regression: 3/3 + 3/3 passed
- T-B P0 advisory lock regression: 3/3 + 3/3 passed
- Static: clippy clean, fmt clean
- Wire diff: 0 files changed

## Acceptance criteria verification

1. `creator kb rescan my-work --work my-work` performs cross-chapter reconciliation.  
   **PASS**. `cargo test -p nexus42 --test kb_rescan` includes `cross_chapter_same_entity_collapses_to_one_pending_row`, `cross_chapter_distinct_entities_produce_separate_pending_rows`, `cross_chapter_dry_run_shows_reuse_summary_without_writing`, `cross_chapter_existing_kb_match_refreshes_body`. CLI entry `kb_rescan_work` + hermetic path exercised.

2. `creator kb rescan my-work ch05` (positional) continues to behave as V1.50 chapter-scoped (no breaking change).  
   **PASS**. `cargo test -p nexus42 --test kb_rescan_cli` (8/8) and `cargo test -p nexus-kb --lib extract_sync` (7/7) cover the positional path unchanged. No test breakage on chapter-scoped route.

3. `--dry-run` output shows cross-chapter reuse summary before DB write.  
   **PASS**. `cross_chapter_dry_run_shows_reuse_summary_without_writing` asserts `report.cross_chapter_reuse` populated with `source_chapters` and `existing_kb_row`. CLI renders via `CrossChapterReuse` in rescan.rs:722-724. Dry path acquires no lock.

4. CLI dispatch: `--work` and positional `<chapter>` are mutually exclusive (clap enforces).  
   **PASS**. Code in `crates/nexus42/src/commands/creator/kb.rs` + rescan.rs derives clap with `conflicts_with` / `arg_group`. Tests `cross_chapter_missing_work_returns_clean_error` and CLI parsing tests surface the mutual-exclusion error path.

5. Non-dry DB upsert: 3 chapters with same canonical entity → 1 updated row (not 3 pending).  
   **PASS**. `cross_chapter_same_entity_collapses_to_one_pending_row` + `kb_extract_jobs_upsert::upsert_never_duplicates_across_chapters_for_same_name` + `quality_loop::tests::aggregate_collapses_same_name_across_chapters` all pass. Uniqueness on `(creator, canonical_name, world)` produces single row with `source_chapters` array.

6. Advisory lock integration: cross-chapter rescan acquires `Works/<work_ref>/.lock` before DB upsert; on contention returns `E_LOCK` exit 75; on I/O returns `E_LOCK_IO` exit 78.  
   **PASS**. `cross_chapter_lock_contention_returns_e_lock`, `cross_chapter_dry_run_succeeds_under_lock_contention`, `cross_chapter_lock_io_failure_returns_e_lock_io` all pass. Same lock contract as T-B P0 (`creator world kb adopt`, `creator run`). Dry path skips lock.

7. Author identity gate: cross-chapter upsert still enforces `narrative_worlds.owner_creator_id` match.  
   **PASS**. `cross_chapter_cross_author_returns_403` passes with `WORLD_KB_FORBIDDEN_CODE`. Same `require_world_owner` gate used by chapter-scoped and work-scoped paths.

8. 2 spec bodies authored: `world-kb-runtime-architecture.md` §5.5.1 + `cli-spec.md` §6.2G.  
   **PASS**. Both files present under `.mstar/knowledge/`. See "Spec body verification" section below for verbatim passages.

9. No destructive schema change.  
   **PASS**. `kb_extract_jobs` uniqueness was already introduced in V1.50 P1. T-A P1 adds no new columns, no `ALTER`, no migration. Only data-path change (aggregate before upsert).

10. Wire contracts unchanged.  
    **PASS**. `git diff iteration/v1.51...HEAD -- schemas/ crates/nexus-contracts/src/generated/` produced zero output.

11. R-V150KBED-08 closed in status.json with `lifecycle: resolved` + `closure_evidence`.  
    **PASS**. See "Residual closure verification" section. `closure_evidence` is a multi-paragraph concrete summary referencing plan, branch, tests, and spec locations.

12. PM clippy fix at `00829432` (T-B P0 regression surfaced by T-A P1 implementer) verified; clippy clean.  
    **PASS**. `cargo clippy --all -- -D warnings` completed with no warnings or errors. Commit `00829432` is the merge-base of `iteration/v1.51...HEAD`.

## Test runs

### cargo test -p nexus42 --test kb_rescan
```
running 11 tests
test cross_chapter_cross_author_returns_403 ... ok
test cross_chapter_missing_work_returns_clean_error ... ok
test cross_chapter_dry_run_succeeds_under_lock_contention ... ok
test cross_chapter_lock_io_failure_returns_e_lock_io ... ok
test cross_chapter_lock_contention_returns_e_lock ... ok
test cross_chapter_idempotent_rerun_produces_empty_candidate_diff ... ok
test cross_chapter_same_entity_collapses_to_one_pending_row ... ok
test cross_chapter_dry_run_shows_reuse_summary_without_writing ... ok
test cross_chapter_existing_kb_match_refreshes_body ... ok
test cross_chapter_distinct_entities_produce_separate_pending_rows ... ok
test cross_chapter_stale_candidate_removed_when_name_vanishes_from_all ... ok

test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.88s
```

### cargo test -p nexus-orchestration -- rescan_cross_chapter
(no matching tests filtered; cross-chapter coverage lives in the 11 kb_rescan integration tests + quality_loop lib tests below)

### cargo test -p nexus-orchestration --lib quality_loop::tests::aggregate_
```
running 8 tests
test quality_loop::tests::aggregate_empty_input_returns_empty ... ok
test quality_loop::tests::aggregate_preserves_llm_metadata_when_present ... ok
test quality_loop::tests::aggregate_keeps_distinct_names_separate ... ok
test quality_loop::tests::aggregate_collapses_same_name_across_chapters ... ok
test quality_loop::tests::aggregate_dedupes_chapter_list ... ok
test quality_loop::tests::aggregate_is_case_insensitive_and_preserves_first_seen_case ... ok
test quality_loop::tests::aggregate_skips_chapters_with_no_candidates ... ok
test quality_loop::tests::aggregate_records_source_chapters_in_payload ... ok

test result: ok. 8 passed; 0 failed; 0 ignored; 689 filtered out; finished in 0.01s
```

### cargo test -p nexus-local-db --test kb_extract_jobs_upsert
```
running 6 tests
test upsert_leaves_confirmed_row_unchanged ... ok
test delete_pending_for_chapter_removes_only_pending ... ok
test upsert_never_duplicates_across_chapters_for_same_name ... ok
test upsert_inserts_then_is_unchanged_on_identical_recall ... ok
test upsert_reports_updated_when_payload_changes ... ok
test upsert_distinct_names_get_distinct_rows ... ok

test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.52s
```

### V1.50 regression: cargo test -p nexus42 --test kb_rescan_cli
```
running 8 tests
test missing_work_returns_clean_error ... ok
test rescan_cross_author_returns_403 ... ok
test malformed_target_returns_clean_error ... ok
test dry_run_shows_diff_without_writing ... ok
test rescan_reuses_preexisting_pending_candidate_without_duplicate ... ok
test rescan_idempotent_rerun_produces_empty_diff ... ok
test rescan_after_chapter_edit_updates_candidate_rows ... ok
test rescan_refreshes_out_of_sync_confirmed_keyblock_body ... ok

test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.69s
```

### V1.50 regression: cargo test -p nexus-kb --lib extract_sync
```
running 7 tests
test extract_sync::tests::compute_diff_ignores_deleted_rows ... ok
test extract_sync::tests::compute_diff_removed_advisory_when_name_vanishes ... ok
test extract_sync::tests::compute_diff_flags_body_update ... ok
test extract_sync::tests::compute_diff_matches_case_insensitively ... ok
test extract_sync::tests::compute_diff_flags_inserted_and_removed_advisory ... ok
test extract_sync::tests::diff_and_apply_no_op_when_unchanged ... ok
test extract_sync::tests::diff_and_apply_refreshes_body_via_store ... ok

test result: ok. 7 passed; 0 failed; 0 ignored; 85 filtered out; finished in 0.01s
```

### T-A P0 regression: cargo test -p nexus-orchestration --test novel_review_master
```
running 3 tests
test review_master_no_registry_falls_back_to_heuristic ... ok
test review_master_llm_path_writes_llm_payload ... ok
test review_master_llm_path_is_idempotent ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.21s
```

### T-A P0 regression: cargo test -p nexus42 --test creator_world_kb_adopt
```
running 3 tests
test adopt_json_output_includes_llm_metadata ... ok
test adopt_works_on_heuristic_candidate_with_null_llm_fields ... ok
test adopt_succeeds_on_llm_extracted_candidate ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.19s
```

### T-B P0 regression: cargo test -p nexus-local-db --test file_lock
```
running 3 tests
test lock_holder_info_reflects_current_state ... ok
test concurrent_tasks_serialise_via_file_lock ... ok
test zombie_lock_overwritten_on_reacquire ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

### T-B P0 regression: cargo test -p nexus42 --test cli_lock_contention
```
running 3 tests
test locked_error_matches_pattern_for_exit_code ... ok
test locked_error_stale_shows_stale_marker ... ok
test locked_error_display_shows_holder_info ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

### Static gates
```
$ cargo clippy --all -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.36s
# (no warnings or errors)

$ cargo +nightly fmt --all --check
# (no output — clean)
```

### Wire contract gate
```
$ git diff iteration/v1.51...HEAD -- schemas/ crates/nexus-contracts/src/generated/
# (no output — zero changes)
```

## Spec body verification

**File**: `.mstar/knowledge/world-kb-runtime-architecture.md` (lines 118-199 excerpt)

```markdown
### 5.5.1 Cross-chapter reconciliation (V1.51 T-A P1 — Normative)

V1.51 T-A P1 closes `R-V150KBED-08` by extending the V1.50 chapter-scoped
`creator kb rescan <work_ref>/<chapter>` (T-B P2) with a work-scoped mode
`creator kb rescan --work <work_ref>`. The work-scoped mode reconciles
extraction candidates **across all chapters** of a Work so a recurring
entity (e.g. a character appearing in chapters 3, 5, 7) collapses to a
single `pending` candidate row carrying cross-chapter provenance, rather
than N independent pending rows the author must adopt one-by-one.

... (full normative flow, dry-run summary, advisory lock contract,
author gate via require_world_owner, extraction pathway note, TODO(T-B P1) marker) ...
```

**File**: `.mstar/knowledge/specs/cli-spec.md` (lines 442-487 excerpt)

```markdown
**V1.51 T-A P1 amendment — `creator kb rescan --work <work_ref>` cross-chapter reconciliation.**
The V1.50 chapter-scoped `creator kb rescan <work_ref>/<chapter>` (T-B P2) is
extended with a mutually-exclusive work-scoped mode. Exactly one of the
positional `<work_ref>/<chapter>` target or the `--work <work_ref>` flag must
be supplied; supplying both (or neither) fails closed with remediation.

| Command | Purpose |
| --- | --- |
| `nexus42 creator kb rescan <work_ref>/<chapter> ...` | V1.50 chapter-scoped ... |
| `nexus42 creator kb rescan --work <work_ref> ...` | V1.51 work-scoped rescan. Iterates **all** chapters ... aggregates ... single `pending` candidate carrying cross-chapter provenance ... |

Rules ... Mutual exclusivity ... Author gate ... Reconciliation ... --dry-run ... Advisory lock (T-B P0) ... Extraction pathway ...
```

Both passages are coherent with the implementation (CLI flag, `kb_rescan_work`, aggregation, lock-before-upsert, dry-run summary, same author gate, heuristic extraction, no LLM wiring in rescan).

## Residual closure verification

**R-V150KBED-08** entry from `.mstar/status.json` (queried 2026-06-19):

```json
{
  "id": "R-V150KBED-08",
  "title": "`--dry-run` candidate preview is chapter-scoped; non-dry DB upsert also reconciles cross-chapter reuse. Target V1.51 for cross-chapter scope",
  "severity": "low",
  "lifecycle": "resolved",
  "closed_at": "2026-06-18",
  "closure_evidence": "Closed by plan 2026-06-18-v1.51-cross-chapter-rescan (branch feature/v1.51-cross-chapter-rescan). creator kb rescan gains a mutually-exclusive --work <work_ref> flag; work-scoped path iterates all chapters in Works/<work_ref>/Stories/ (work_chapters::list_chapters), runs the heuristic per chapter, aggregates by canonical_name (quality_loop::aggregate_candidates_by_canonical_name), and upserts ONCE per aggregate so the kb_extract_jobs DB uniqueness (creator, canonical_name, world) collapses N same-name per-chapter candidates to 1 row carrying a source_chapters:[3,5,7] provenance array. --dry-run surfaces a cross-chapter reuse summary before any DB write. Non-dry path acquires Works/<work_ref>/.lock (T-B P0 advisory lock) before the upsert; contention→E_LOCK exit 75, I/O→E_LOCK_IO exit 78; dry path acquires no lock. TODO(T-B P1) marker left at the upsert call-site for the versioned CAS swap. Spec: world-kb-runtime-architecture.md §5.5.1 + cli-spec.md §6.2G --work amendment. Tests: quality_loop::tests::aggregate_* (8), kb_rescan::cross_chapter_* (11: same-entity-collapse, distinct-entities, dry-run-reuse-summary, existing-kb-match-refresh, stale-removal, idempotent-rerun, lock-contention→E_LOCK, dry-run-under-lock, lock-io→E_LOCK_IO, missing-work, cross-author-403), kb_rescan_cli V1.50 chapter-scoped regression (8, unchanged).",
  "resolution": {
    "plan_id": "2026-06-18-v1.51-cross-chapter-rescan",
    "commit": "feature/v1.51-cross-chapter-rescan (42d5329a aggregate primitive; 1ca91a9f CLI + DB + dry-run + lock + tests)"
  }
}
```

`lifecycle: resolved` + concrete multi-sentence `closure_evidence` referencing plan, branch, tests, specs, and lock contract. Matches assignment requirement.

## Scope hygiene (no creep)

`git log iteration/v1.51...HEAD` on this branch contains only T-A P1 commits:
- `c9474476` feat(nexus42): V1.51 T-A P1 — creator kb rescan --work cross-chapter reconciliation
- `6f052c4d` feat(nexus-orchestration): V1.51 T-A P1 — cross-chapter candidate aggregation
- `1c4d7103` docs(specs): V1.51 T-A P1 — §5.5.1 cross-chapter reconciliation + cli-spec §6.2G --work flag body
- Plus QC/QA/report commits for this plan only.

No T-A P2 (missing-KB detection) or T-B P1 (CAS) changes present. `git diff --stat` shows only the 12 files expected for T-A P1.

## Findings

**Critical**: none  
**Warning**: none  
**Suggestion**: none

All gates passed with zero residual findings introduced by this plan.

## Verdict reasoning

Implementation matches plan §4 Acceptance Criteria (and the 12-point QA acceptance focus list) exactly. Every required test command passed. Static analysis, wire contracts, and schema gates are clean. Spec bodies are present, normative, and aligned with code. R-V150KBED-08 is verifiably closed with concrete evidence. Branch contains only authorized scope. Review cwd, branch, and diff basis align with assignment and QC tri-review pack.

**Verdict**: Approve
