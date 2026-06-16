## Completion Report v2 — P3 (serial-reliability)

- plan_id: 2026-06-17-v1.49-serial-reliability
- owner: @fullstack-dev
- Working branch used: feature/v1.49-serial-reliability
- Worktree path: .worktrees/v1.49-serial-reliability
- Base: iteration/v1.49 @ cb2d3fde
- Commits:
  - `aeb95397` fix(reconcile): shorten runtime-lock window to write phase only (R-V148P4-W3) — T1
  - `f868961c` feat(findings): wire prune CLI + `--dry-run` preview to retention DAO (§9.4) — T2
  - `c10e4337` fix(review-report): add path-traversal guard to `load_and_parse_review_report` (R-V148P0-W1) — T3

### Cargo verification (last lines of each command)

> Note on the assignment's verification commands: `cargo test -p <crate> <substr>`
> applies a **test-name substring** filter. Several of the listed substrings
> (`findings_api`, `review_report`, `findings`) match **0 test names** (they are
> file/module names, not test names), so the literal commands exit 0 by running
> nothing. To actually cover the changes, the **full relevant suites** were run
> and are shown here; the literal commands also pass (exit 0).

`cargo +nightly fmt --all --check`
```
(checked all; no diff)  fmt exit: 0
```

`cargo clippy -p nexus42 -p nexus-daemon-runtime -p nexus-local-db -p nexus-orchestration -- -D warnings`
```
    Checking nexus-local-db v0.1.0
    Checking nexus-orchestration v0.1.0
    Checking nexus-daemon-runtime v0.1.0
    Checking nexus42 v0.1.0
    Finished `dev` profile [unoptimized + debuginfo] target(s)
```

`cargo test -p nexus42 works` (literal; substring filter)
```
test r5_platform_guard_sync_status_works ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 14 filtered out
```

`cargo test -p nexus-daemon-runtime --test runtime_lock` (full file — covers T1 lock work)
```
test test_reconcile_chapters_dry_run_makes_zero_mutations ... ok
test test_reconcile_chapters_read_phase_runs_unlocked ... ok
test test_reconcile_chapters_releases_lock_on_error ... ok
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured
```

`cargo test -p nexus-daemon-runtime --test findings_api` (full file — covers T2 endpoint)
```
test findings_prune_endpoint_dry_run_and_delete ... ok
test result: ok. 14 passed; 0 failed; 0 ignored; 0 measured
```

`cargo test -p nexus-orchestration --lib review_report` and `auto_chain` (covers T3 + guard)
```
review_report: test result: ok. 13 passed; 0 failed
auto_chain:    test result: ok. 25 passed; 0 failed
  (incl. test load_and_parse_review_report_rejects_path_outside_work_dir ... ok)
```

`cargo test -p nexus-local-db --lib findings::` and `work_chapters::` (covers T1 DAO split + T2 count)
```
findings:      test result: ok. 25 passed; 0 failed
work_chapters: test result: ok. 30 passed; 0 failed
  (incl. test_compute_is_read_only_then_apply_writes ... ok,
        findings_retention_count_preview_matches_prune ... ok)
```

### Acceptance criteria

1. **Reconcile lock strategy documented + tested (no regression on W-2 release fix)** —
   commit `aeb95397`. Strategy: split `reconcile_from_filesystem` into a read-only
   `compute_reconcile_diff` (unlocked filesystem walk + per-chapter DB reads) and a
   write-only `apply_reconcile_diff` (the only phase under the runtime lock). The
   handler computes the diff before `RuntimeLockGuard::acquire` and holds the lock
   only across apply. Tests: `test_reconcile_chapters_read_phase_runs_unlocked`
   (read failure never acquires the lock — R-V148P4-W3 evidence),
   `test_reconcile_chapters_releases_lock_on_error` (refocused to an **apply-phase**
   error via status-conflict + read-only `Stories/`; preserves the W-2 release-on-error
   guarantee), `test_compute_is_read_only_then_apply_writes` (DAO-layer read/write split).
   Tracing emits `acquired_at` / `held_ms` for operator observability.
2. **Prune CLI invokes DAO; hermetic test with seeded old `resolved` rows** —
   commit `f868961c`. `creator works findings prune [--older-than <days>] [--dry-run]`
   (FindingsCommand::Prune) → `POST /v1/local/findings/prune` → DAO. Tests:
   `findings_prune_endpoint_dry_run_and_delete` (dry-run reports 1 and deletes
   nothing; real prune deletes 1, open finding survives),
   `findings_retention_count_preview_matches_prune` (DAO preview == prune, no deletion).
3. **Path guard test for review-report resolution under `Works/<work_ref>/`** —
   commit `c10e4337`. Lexical guard (reject `..`, `/`, `\`, NUL, empty) before path
   construction + canonical prefix guard after existence. Test
   `load_and_parse_review_report_rejects_path_outside_work_dir` covers the
   `Works/<work_ref>/../../../etc/passwd` shape, a separator-bearing ref, and a clean
   ref correctly **not** over-rejected (returns `Missing`).
4. **Residuals R-V148P4-W3 + R-V148P0-W1 evidence collected (PM archives)** —
   R-V148P4-W3: lock window now excludes the read phase (T1). R-V148P0-W1:
   path guard added before P1/P2 prompt-injection surfaces grow (T3). §9.4
   data-hygiene loop closed via the prune CLI (T2). See "Residual closure" below.

### T1 strategy & trade-off (documented per acceptance)

- **Chosen strategy**: compute-before-lock + apply-under-lock (the plan's
  recommended option). `compute_reconcile_diff` carries decisions as a
  `ReconcileDiff` (`Vec<ReconcileOp>` + preserved count); `apply_reconcile_diff`
  executes only the writes. `ReconcileReport` counters derive from the diff via
  `to_report()`, so the dry-run preview and the mutating path can never disagree.
  `reconcile_from_filesystem` is now a thin compute+apply wrapper (dry-run path
  unchanged).
- **Trade-off (accepted, local-first single-writer daemon model)**: under
  concurrent reconcile + mutate from the same client, the diff may be stale by the
  time the lock is re-acquired. Documented in code comments and the commit message.
- **Alternative considered (rejected)**: chunked release/re-acquire between walk
  segments — more complex with no benefit for the single-writer model.
- **V1.48 P4-fix1 guarantee preserved**: the apply-phase error path explicitly
  calls `lock.release().await` before returning `Err`.

### Residual closure

- **R-V148P4-W3** (reconcile lock duration): closable. The slow filesystem walk +
  per-chapter DB reads now run unlocked; the lock is held only for the fast write
  phase. Evidence: `test_reconcile_chapters_read_phase_runs_unlocked`,
  `test_compute_is_read_only_then_apply_writes`, handler tracing `held_ms`.
  (PM to archive the residual row after QC/QA.)
- **R-V148P0-W1** (review-report path guard): closable. Lexical + canonical guard
  rejects traversal/symlink escape before the read. Evidence:
  `load_and_parse_review_report_rejects_path_outside_work_dir`.
  (PM to archive the residual row after QC/QA.)
- Both residual rows live in `status.json` root `residual_findings` under earlier
  plan_ids (`2026-06-16-v1.48-serial-hardening`, `2026-06-16-v1.48-findings-producer`).
  Per assignment, those rows were **not** modified by this plan; PM archives them
  post-QA.

### Scope hygiene

- 12 files changed, all within the plan's declared code-touch set (+ the plan file
  checkboxes). No new top-level crates. No JSON Schema / wire-contract changes.
- No P0/P1/P2 surface modified: the `findings.rs` change is purely **additive**
  (`count_resolved_findings_older_than` + `use sqlx::Row`); the existing
  `prune_resolved_findings_older_than` and the V1.49 P0 lifecycle functions are
  untouched. `narrative_index` and `author-desk` files untouched.
- `#[allow(...)]` used once with inline justification
  (`clippy::too_many_lines` on `try_persist_parsed_findings` — a single match of
  spec-mandated fallback arms; mirrors the `work_chapters` reconcile / `handle_rules_reset`
  precedent).

### Risks / follow-ups

- **Stale-diff trade-off** (T1): documented and accepted for the single-writer
  daemon model. If a future change introduces concurrent writers for the same Work,
  the compute/apply split must be revisited (re-read in apply or atomic claim).
- **Global prune scope** (T2): `prune_resolved_findings_older_than` is global across
  creators. Intentional for local-first single-creator; not changed here (DAO is a
  protected P0 surface).
- **clippy baseline**: local stable clippy (1.93.0) flags pre-existing
  `doc_markdown` pedantic noise in **test modules** of untouched files; it does not
  surface under the CI gate (`cargo clippy ... -- -D warnings`, no `--tests`). Only
  newly-added code was made lint-clean. The single lib-level `doc_markdown` hit in
  new code was fixed (backticks around `word_count`).
- `count_resolved_findings_older_than` uses a runtime `sqlx::query` (SAFETY comment)
  rather than a compile-time macro, to avoid churning the shared `.sqlx/` offline
  cache (DATABASE_URL unset); matches the `work_chapters` precedent (waiver
  R-V140P0-S3).

- Ready for QC tri-review: yes
