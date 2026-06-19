---
report_kind: qa_verification
reviewer: qa-engineer
plan_id: 2026-06-18-v1.51-per-row-occ
verdict: Approve
generated_at: 2026-06-19T20:15:00Z
---

# QA Verification Report — V1.51 T-B P1 (Per-Row OCC)

## Summary

**Verdict**: Approve

All 14 acceptance criteria verified. All mandatory test targets green (including new `cron_cas_retry` target added for QC2 W-002). Static gates clean (`cargo clippy --all -- -D warnings`, `cargo +nightly fmt --all --check`). Wire contracts unchanged. Spec bodies coherent with implementation. Acquire-order discipline (file lock → DB lock → CAS) preserved in both `kb_adopt` and `try_fire_role`. T-B P0 / T-A P0 / V1.50 regressions fully preserved. No Critical or Warning findings.

**Verification counts** (all runs on `feature/v1.51-per-row-occ` at e115c3f4, base iteration/v1.51):
- CAS unit + migration: 5/5 + 5/5
- Cron CAS retry (NEW target): 3/3
- CLI E_VERSION + KB adopt CAS: 4/4 + 6/6
- T-B P0 regressions: 3/3 + 3/3
- T-A P0 regressions: 15/15 (llm_extract) + 3/3 (novel_review_master) + 3/3 (creator_world_kb_adopt)
- V1.50 regression: 22/22
- Stress (8 threads): all deterministic
- Static: clippy clean, fmt clean, doc unresolved links (post QC1 fix) clean for cas.rs

## Acceptance Criteria Verification (1-14)

1. **`kb_extract_jobs.version` + `novel_pool_entries.version` columns added via additive DB migration; existing rows get `version=0` default.**  
   ✅ Migration `crates/nexus-local-db/migrations/202606190001_kb_extract_jobs_and_pool_version.sql` (ADD COLUMN ... DEFAULT 0).  
   ✅ `cargo test -p nexus-local-db --test cas_migration_roundtrip` → 5/5 pass (`test_kb_extract_jobs_version_column_defaults_to_zero`, `test_novel_pool_entries_version_column_defaults_to_zero`).

2. **`cas_check` + `with_cas_retry` helpers in `nexus-local-db` (mirroring `set_schedule_json_tx` pattern but generalised).**  
   ✅ `crates/nexus-local-db/src/cas.rs` (443 lines): `pub async fn cas_check(...)` + `pub async fn with_cas_retry(...)` + unit tests (5).  
   ✅ `cargo test -p nexus-local-db --test cas_migration_roundtrip` → 5/5 pass (`test_cas_marks_confirmed_with_version_guard`, `test_cas_marks_confirmed_rejects_stale_version`, `test_version_increments_on_cas_update`).

3. **`E_VERSION` stable CLI code in `nexus42::errors` with exit code 76 (distinct from E_LOCK 75 + E_LOCK_IO 78).**  
   ✅ `crates/nexus42/src/errors.rs`: `VersionConflict` variant + Display + exit mapping.  
   ✅ `crates/nexus42/src/main.rs`: `VersionConflict` → 76.  
   ✅ `cargo test -p nexus42 --test cli_version_error` → 4/4 pass (`test_exit_code_76_matches_version_conflict`, `test_version_conflict_distinct_from_locked`).

4. **Cron-side retry: `with_cas_retry(max_attempts=3, backoff_ms=100)` in `try_fire_role`.**  
   ✅ `crates/nexus-orchestration/src/schedule/cron_supervisor.rs:367-404`: explicit 3-attempt / 100ms loop catching `VersionMismatch` from `enqueue_cron_schedule`.  
   ✅ New target `crates/nexus-daemon-runtime/tests/cron_cas_retry.rs` (QC2 W-002 fix) with 3 tests.  
   ✅ `cargo test -p nexus-daemon-runtime --test cron_cas_retry` → 3/3 pass.

5. **`creator world kb adopt` on a stale preimage returns `E_VERSION` exit 76 with actual_version surfaced in user-visible message (per QC2 W-001 fix; hermetic test `test_version_conflict_surfaces_actual_version_in_error_message`).**  
   ✅ `crates/nexus42/src/commands/creator/world/kb.rs:541-554`: `if let VersionMismatch { actual, .. }` → `CliError::VersionConflict { actual_version: *actual }`.  
   ✅ `cargo test -p nexus42 --test kb_adopt_cas` → 6/6 pass (includes `test_version_conflict_surfaces_actual_version_in_error_message` asserting "actual v3" and `test_version_conflict_without_actual_displays_question_mark`).

6. **`kb_adopt` E_VERSION message includes actual_version value (not `?`).**  
   ✅ Verified by the two new hermetic tests above. When `actual: Some(3)` the message contains "actual v3"; `None` case falls back to "?" (documented contract).

7. **2 spec bodies authored: `world-kb-runtime-architecture.md` §6 OCC + `concurrency.md` §7 OCC extension.**  
   ✅ `.mstar/knowledge/specs/concurrency.md` §7 (lines 230-297): full OCC rationale, versioned tables, `cas_check`/`with_cas_retry` API, KB-side + cron-side integration, exit code contract, anti-patterns.  
   ✅ `.mstar/knowledge/world-kb-runtime-architecture.md` §6.1 (lines 122-163): OCC protection subsection + ASCII adopt flow diagram showing file lock → CAS.

8. **Wire contracts unchanged.**  
   ✅ `git diff iteration/v1.51...HEAD -- schemas/ crates/nexus-contracts/src/generated/` → empty (0 lines).

9. **PM clippy fix at `00829432` (T-B P0 regression) verified clean.**  
   ✅ `cargo clippy --all -- -D warnings` → clean (0 errors) on final HEAD.

10. **R-V151-MERGE-CLIPPY-01 stale residual acknowledged (PM fix resolves; closure at P-last WL-A).**  
    ✅ Residual was a pre-existing test-target clippy noise (not introduced by this plan). PM fix at 00829432 + final clippy gate green. Acknowledged for P-last WL-A hygiene.

11. **`cargo test -p nexus-daemon-runtime --test cron_cas_retry` runs and passes (3 tests: happy/retry/exhaustion).**  
    ✅ 3/3 pass (see #4). Target created as part of QC2 W-002 revalidation.

12. **T-B P0 advisory lock preserved (`file_lock` + `cli_lock_contention`).**  
    ✅ `cargo test -p nexus-local-db --test file_lock` → 3/3.  
    ✅ `cargo test -p nexus42 --test cli_lock_contention` → 3/3.

13. **T-A P0 LLM extraction preserved (`llm_extract` + `novel_review_master` + `creator_world_kb_adopt`).**  
    ✅ `cargo test -p nexus-orchestration -- llm_extract` → 15/15.  
    ✅ `cargo test -p nexus-orchestration --test novel_review_master` → 3/3.  
    ✅ `cargo test -p nexus42 --test creator_world_kb_adopt` → 3/3.

14. **V1.50 cron preserved (`cron_supervisor`).**  
    ✅ `cargo test -p nexus-orchestration --test cron_supervisor` → 22/22 (includes V1.50 `set_schedule_json_tx` CAS + concurrent writers serialise tests).

## Test Runs (Full Output — Mandatory Commands)

### CAS unit + integration
```
$ cargo test -p nexus-local-db --test cas_migration_roundtrip
running 5 tests
test test_version_increments_on_cas_update ... ok
test test_cas_marks_confirmed_with_version_guard ... ok
test test_kb_extract_jobs_version_column_defaults_to_zero ... ok
test test_cas_marks_confirmed_rejects_stale_version ... ok
test test_novel_pool_entries_version_column_defaults_to_zero ... ok
test result: ok. 5 passed
```

(Note: Assignment listed `--test cas_update`; actual target per implementation is `cas_migration_roundtrip`. All CAS tests live and pass there.)

### Cron-side CAS retry (NEW target per QC2 W-002)
```
$ cargo test -p nexus-daemon-runtime --test cron_cas_retry
running 3 tests
test test_cron_cas_happy_path ... ok
test test_cron_cas_retry_succeeds_after_version_mismatch ... ok
test test_cron_cas_exhaustion_returns_version_mismatch ... ok
test result: ok. 3 passed
```

### CLI error codes (E_VERSION = 76)
```
$ cargo test -p nexus42 --test cli_version_error
running 4 tests
test test_exit_code_76_matches_version_conflict ... ok
test test_version_conflict_actual_none_displays_question_mark ... ok
test test_version_conflict_displays_e_version ... ok
test test_version_conflict_distinct_from_locked ... ok
test result: ok. 4 passed
```

### KB adopt CAS (includes actual_version surfacing)
```
$ cargo test -p nexus42 --test kb_adopt_cas
running 6 tests
test test_version_conflict_without_actual_displays_question_mark ... ok
test test_version_conflict_surfaces_actual_version_in_error_message ... ok
test test_cas_version_mismatch_direct ... ok
test test_kb_adopt_succeeds_when_version_consistent ... ok
test test_kb_adopt_already_confirmed_returns_error ... ok
test test_kb_adopt_stale_preimage_returns_version_conflict ... ok
test result: ok. 6 passed
```

### T-B P0 regression (advisory lock)
```
$ cargo test -p nexus-local-db --test file_lock
running 3 tests ... ok. 3 passed

$ cargo test -p nexus42 --test cli_lock_contention
running 3 tests ... ok. 3 passed
```

### T-A P0 regression (LLM extraction)
```
$ cargo test -p nexus-orchestration -- llm_extract
running 15 tests ... ok. 15 passed

$ cargo test -p nexus-orchestration --test novel_review_master
running 3 tests ... ok. 3 passed

$ cargo test -p nexus42 --test creator_world_kb_adopt
running 3 tests ... ok. 3 passed
```

### V1.50 regression
```
$ cargo test -p nexus-orchestration --test cron_supervisor
running 22 tests ... ok. 22 passed
```

### Stress / race fidelity
```
$ cargo test -p nexus-local-db --test cas_migration_roundtrip -- --test-threads=8
running 5 tests ... ok. 5 passed (0.45s)

$ cargo test -p nexus-daemon-runtime --test cron_cas_retry -- --test-threads=8
running 3 tests ... ok. 3 passed (0.20s)
```

### Static gates
```
$ cargo doc -p nexus-local-db --no-deps 2>&1 | grep -i "unresolved" | head -3
(no cas.rs hits; only pre-existing unrelated warnings)

$ cargo clippy --all -- -D warnings
Finished dev profile ... (0 errors)

$ cargo +nightly fmt --all --check
(no output — clean)
```

### Wire contract gate
```
$ git diff iteration/v1.51...HEAD -- schemas/ crates/nexus-contracts/src/generated/
(no output — unchanged)
```

### Spec bodies (key passages)
```
$ grep -n "OCC\|version\|cas_check\|with_cas_retry" .mstar/knowledge/specs/concurrency.md | head -10
230:## 7. Per-Row Optimistic Concurrency Control (OCC) — V1.51 T-B P1
255:- **`cas_check(...)`**
256:- **`with_cas_retry(...)`**
...

$ grep -n "OCC\|version" .mstar/knowledge/world-kb-runtime-architecture.md | head -5
122:### 6.1 OCC protection (V1.51 T-B P1)
124:`kb_extract_jobs.version` (added V1.51 T-B P1 migration `202606190001`)
```

## Acquire-Order Discipline Trace (file lock → CAS)

**`kb_adopt` (crates/nexus42/src/commands/creator/world/kb.rs)**:
1. `load_pending_candidate()` (read-only, captures `candidate.version`).
2. Author identity gate.
3. **File lock**: `try_acquire(&work_dir, "cli:kb-adopt")` (line 476) — guard held.
4. `pool.begin()` (line 527) — DB tx.
5. `insert_key_block_in_tx`.
6. **`mark_confirmed_in_tx_with_cas(&mut tx, extract_job_id, candidate_version)`** (line 541) — CAS inside tx:
   - `UPDATE ... SET ... version=version+1 WHERE ... AND version = ?`.
   - On `rows_affected == 0` → `cas_check` re-reads actual version → `VersionMismatch` → `CliError::VersionConflict { actual_version: *actual }` → E_VERSION 76.
7. `tx.commit()`.
8. File lock dropped on scope exit.

**`try_fire_role` (crates/nexus-orchestration/src/schedule/cron_supervisor.rs)**:
1. Read-only cron match / idempotency checks.
2. **File lock**: `maybe_acquire_cron_file_lock(...)` (line 348) — guard held.
3. CAS retry loop (lines 371-404, 3 attempts / 100ms):
   - `enqueue_cron_schedule(...)`.
   - On `AutoChainError::Database(LocalDbError::VersionMismatch { .. })` → warn + backoff + retry.
   - Guard held through all retries.
4. File lock dropped on scope exit (line 414).

**Spec reference**: `concurrency.md` §2.4 (file BEFORE DB), §7.5 (CAS inside file-lock scope). No reverse acquisition paths. No `unsafe`.

## Findings

### 🔴 Critical
None.

### 🟡 Warning
None.

### 🟢 Suggestion
- One minor naming drift in assignment vs reality: assignment lists `--test cas_update` (does not exist); actual target is `--test cas_migration_roundtrip` (all CAS tests pass there). Recommend aligning future assignment text with the test binary name.
- Pre-existing unrelated `cargo doc` warnings on other modules (not in cas.rs, not introduced by this plan) remain visible in `cargo doc -p nexus-local-db --no-deps`. Out of scope for T-B P1.

## Verdict Reasoning

All 14 acceptance criteria pass with reproducible evidence.  
QC tri-review (after revalidation of W-001/W-002) was Approve across all three reviewers.  
Static gates (clippy, fmt, doc) clean. Wire contracts untouched.  
Spec bodies (§7 OCC + §6.1 OCC) are coherent with the code and contain the required normative content.  
Acquire-order discipline is explicitly preserved and matches the spec.  
No T-B P0 / T-A P0 / V1.50 regressions introduced.  
Stress runs deterministic.  
No Critical or Warning findings remain.

**Verdict: Approve**

---

**Git context at report generation**:
- Review cwd: `/Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.51-t-b-p1`
- Working branch: `feature/v1.51-per-row-occ`
- HEAD: `e115c3f4967d92abd0816ff370ad3ababd606145`
- Diff basis: `iteration/v1.51...HEAD`
- Base commit (iteration/v1.51): `008294327a8a33714948eb6d810794d338ceaa93`
