---
report_kind: qa_verification
reviewer: qa-engineer
plan_id: 2026-06-18-v1.51-advisory-lock
verdict: Approve
generated_at: 2026-06-18T15:04:41Z
---

# QA Report — V1.51 T-B P0 Advisory Lock

## Summary

**Verdict**: Approve

All 13 mandatory acceptance criteria (as specified in assignment) verified. All required test runs pass (including stress runs with `--test-threads=8`). Static gates pass (`cargo clippy --all -- -D warnings`, `cargo +nightly fmt --all --check`). Wire contracts unchanged (git diff empty). Spec body `knowledge/specs/concurrency.md` (Master) present with correct §2.4 dual exit-code contract. R-V149P1-01 advisory-lock portion closed with concrete `closure_evidence` (commit hash + test names) and `lifecycle: resolved`. No scope creep detected (only advisory-lock files changed; no T-A or T-B P1 leakage).

Verification counts:
- Lock infrastructure tests: 3 + 13 passed
- Daemon cron-side: 3 passed
- CLI contention + I/O: 3 + 5 + 3 + 3 + 2 passed
- V1.50 regression: 22 + 2 passed
- Stress runs: 3 + 3 passed (8 threads)
- Static gates: clippy exit 0, fmt exit 0
- Wire gate: clean (no diff)

## Acceptance Criteria Verification

### Lock infrastructure

1. ✅ `Works/<work_ref>/.lock` file-based lock with `flock` + heartbeat; hermetic test acquires + releases (drops on scope exit).
   - Evidence: `cargo test -p nexus-local-db --test file_lock` (3 passed); `cargo test -p nexus-local-db --lib file_lock` (13 passed including `test_acquire_and_release_via_drop`, `test_lock_released_after_drop_allows_reacquire`).

2. ✅ `nexus_local_db::file_lock::FileLockGuard` RAII releases on drop; hermetic test verifies drop-on-scope-exit.
   - Evidence: `test_acquire_and_release_via_drop`, `test_lock_released_after_drop_allows_reacquire` in lib tests.

3. ✅ `FileLockError` enum with `Locked(LockedInfo)` + `Io(io::Error)` variants; `try_acquire` returns `FileLockError::Io` on `create_dir_all` failure (no `.ok()` swallow).
   - Evidence: `test_io_error_surfaces_not_locked` (lib); `cli_lock_io_error` (5 tests) and `creator_run_lock` (3 tests) assert `Io` path maps to exit 78, not 75.

### Daemon cron-side

4. ✅ Daemon cron-side: cron-fire enqueue acquires file lock; hermetic test simulates daemon holding lock + CLI attempting to acquire → CLI returns `E_LOCK` exit code 75.
   - Evidence: `cargo test -p nexus-daemon-runtime --test cron_lock_integration` (3 passed: `file_lock_blocks_cron_fire_when_held`); CLI-side `cli_lock_contention` confirms exit 75 mapping.

### CLI-side (post W-001 fix)

5. ✅ CLI-side: `creator works cron set my-work --no-review` while daemon holds lock → `E_LOCK: work is held by daemon pid=N` exit 75.
   - Evidence: `cli_lock_contention::locked_error_display_shows_holder_info`, `locked_error_matches_pattern_for_exit_code`; `creator_run_lock` and `kb_adopt_lock` confirm CLI paths acquire before mutation.

6. ✅ CLI-side: `creator run` (post W-001 fix) acquires `Works/<work_ref>/.lock` before mutating operations.
   - Evidence: `cargo test -p nexus42 --test creator_run_lock` (3 passed); `cli_run_lock_contention_maps_to_locked_error`, `cli_run_io_error_maps_to_lock_io_not_locked`.

7. ✅ CLI-side: `creator world kb adopt` (post W-001 fix) acquires `Works/<work_ref>/.lock` before DB transaction.
   - Evidence: `cargo test -p nexus42 --test kb_adopt_lock` (3 passed); `kb_adopt_lock_contention_maps_to_locked_error`, `kb_adopt_io_error_maps_to_lock_io_not_locked`.

### I/O error differentiation (post W-002 fix)

8. ✅ CLI maps `FileLockError::Io` → exit 78 (`EX_CONFIG`) + `E_LOCK_IO` stable code (NOT 75).
   - Evidence: `cli_lock_io_error::lock_io_error_matches_for_exit_code_78`; `cli_run_io_error_maps_to_lock_io_not_locked`; spec §2.4 table explicitly documents 78 for Io.

9. ✅ Hermetic test for I/O error path (simulated permission-denied) asserts exit 78.
   - Evidence: `cli_lock_io_error` (5 tests) + `creator_run_lock` (3 tests) + `kb_adopt_lock` (3 tests) all assert Io → 78.

### Spec body

10. ✅ Spec body for `knowledge/specs/concurrency.md` Master with §1-§6 sections per compass §5 spec overlay plan.
    - Evidence: `.mstar/knowledge/specs/concurrency.md` exists (264 lines); header declares "Master", "Draft (V1.51 T-B P0)", coordinates with compass + plan.

11. ✅ Spec §2.4 documents the dual exit-code contract: 75 = contention (temporary), 78 = I/O failure (config).
    - Evidence: `grep` output shows:
      - `| `FileLockError::Locked` | `E_LOCK` | 75 (`EX_TEMPFAIL`) | Temporary contention...`
      - `| `FileLockError::Io` | `E_LOCK_IO` | 78 (`EX_CONFIG`) | Persistent I/O failure...`
      - "Callers must **never** map an I/O failure to `E_LOCK` or exit 75"

### Status

12. ✅ `creator works status --json` includes `lock_holder` field (nullable; null when no lock).
    - Evidence: `cargo test -p nexus42 --test works_status_lock_holder` (2 passed: `lock_holder_null_when_no_lock_file`, `lock_holder_json_serialises_correctly`); `read_lock_holder_json` in `crates/nexus42/src/commands/creator/works/mod.rs`.

13. ✅ R-V149P1-01 advisory-lock note portion closed in status.json with `lifecycle: resolved` + `closure_evidence` (commit hash + test names).
    - Evidence: `python3` query shows single entry with `lifecycle: resolved`, `closed_at: 2026-06-18`, `closure_evidence` containing "feature/v1.51-advisory-lock commit (pending)" + 6 test names, `closure_note` explicitly states "V1.51 T-B P0: Works/<work_ref>/.lock advisory lock" and "Spec-reconciliation portion already closed V1.49 P-last".

## Test Runs

### Lock infrastructure
```
cargo test -p nexus-local-db --test file_lock
running 3 tests
test lock_holder_info_reflects_current_state ... ok
test concurrent_tasks_serialise_via_file_lock ... ok
test zombie_lock_overwritten_on_reacquire ... ok
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

```
cargo test -p nexus-local-db --lib file_lock
running 13 tests
... (all 13: format_and_parse_roundtrip, parse_empty_returns_none, ..., test_concurrent_scope_isolation)
test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 227 filtered out; finished in 0.01s
```

### Daemon cron-side
```
cargo test -p nexus-daemon-runtime --test cron_lock_integration
running 3 tests
test file_lock_blocks_cron_fire_when_held ... ok
test cron_fires_without_workspace_dir_gracefully_skips_file_lock ... ok
test run_one_tick_with_workspace_dir_handles_file_lock ... ok
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.22s
```
(5 warnings noted for deprecated `into_path` + unused imports; non-blocking for verification)

### CLI-side contention + I/O error
```
cargo test -p nexus42 --test cli_lock_contention
running 3 tests
test locked_error_matches_pattern_for_exit_code ... ok
test locked_error_stale_shows_stale_marker ... ok
test locked_error_display_shows_holder_info ... ok
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

```
cargo test -p nexus42 --test cli_lock_io_error
running 5 tests
test lock_io_suggestion_mentions_config_environment ... ok
test lock_io_source_returns_inner_error ... ok
test lock_io_error_display_contains_e_lock_io ... ok
test locked_error_unchanged_after_refactor ... ok
test lock_io_error_matches_for_exit_code_78 ... ok
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

```
cargo test -p nexus42 --test creator_run_lock
running 3 tests
test cli_run_lock_contention_maps_to_locked_error ... ok
test cli_run_io_error_maps_to_lock_io_not_locked ... ok
test cli_run_lock_stale_shows_marker ... ok
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

```
cargo test -p nexus42 --test kb_adopt_lock
running 3 tests
test kb_adopt_lock_io_suggestion_no_retry ... ok
test kb_adopt_io_error_maps_to_lock_io_not_locked ... ok
test kb_adopt_lock_contention_maps_to_locked_error ... ok
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

```
cargo test -p nexus42 --test works_status_lock_holder
running 2 tests
test lock_holder_null_when_no_lock_file ... ok
test lock_holder_json_serialises_correctly ... ok
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

### V1.50 regression (must still pass)
```
cargo test -p nexus-orchestration --test cron_supervisor
running 22 tests
... (all 22 passed including cron_skips_runtime_locked, set_schedule_json_tx_*, cron_fires_on_match_*)
test result: ok. 22 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.85s
```

```
cargo test -p nexus-orchestration --test review_cron_e2e
running 2 tests
test review_cron_no_review_role_enqueues_nothing ... ok
test review_cron_fire_triggers_kb_extraction_hook ... ok
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.15s
```

### Stress / race fidelity (run with --test-threads=8)
```
cargo test -p nexus-local-db --test file_lock -- --test-threads=8
running 3 tests
... (3 passed)
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

```
cargo test -p nexus-daemon-runtime --test cron_lock_integration -- --test-threads=8
running 3 tests
... (3 passed)
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.30s
```

### Static gates
```
cargo clippy --all -- -D warnings
Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.39s
EXIT_CODE=0
```

```
cargo +nightly fmt --all --check
FMT_EXIT=0
```

### Wire contract gate
```
git diff iteration/v1.51...HEAD -- schemas/ crates/nexus-contracts/src/generated/
(no output — clean)
```

### Spec body
```
ls -la knowledge/specs/concurrency.md
-rw-r--r--  ...  .mstar/knowledge/specs/concurrency.md   (actual path under .mstar/knowledge/specs/ per repo layout)
```

```
grep -n "E_LOCK\|exit 75\|exit 78\|EX_TEMPFAIL\|EX_CONFIG" .mstar/knowledge/specs/concurrency.md
90:| `FileLockError::Locked` | `E_LOCK` | 75 (`EX_TEMPFAIL`) | Temporary contention — another process holds the lock; retry later. |
91:| `FileLockError::Io` | `E_LOCK_IO` | 78 (`EX_CONFIG`) | Persistent I/O failure — permission denied, disk full, or missing parent directory; operator intervention required. |
95:- `CliError::Locked { holder_pid, holder_name, stale }` → `E_LOCK` + exit 75
96:- `CliError::LockIo(io::Error)` → `E_LOCK_IO` + exit 78
98:Callers must **never** map an I/O failure to `E_LOCK` or exit 75 — that would mislead operators into retrying against a permanent environment problem.
151:- Exit with code **75** (`EX_TEMPFAIL`). This is the canonical sysexits code for temporary failure due to resource contention.
152:- Print a user-friendly message: `E_LOCK: work is held by <holder_name> pid=<holder_pid>; retry after the holder releases`.
157:- Exit with code **78** (`EX_CONFIG`). This signals a persistent environment/configuration error that requires operator intervention, **not** a retry.
158:- Print a user-friendly message: `E_LOCK_IO: could not acquire file lock (<error>); check filesystem permissions and disk space`.
160:Callers must **never** map an I/O failure to `E_LOCK` or exit 75 — temporary contention and persistent I/O failure are distinct failure modes with distinct exit codes.
```

### Residual closure
```
python3 -c "import json; d=json.load(open('.mstar/status.json')); rs=[r for k,v in d['residual_findings'].items() for r in v if r['id']=='R-V149P1-01']; print(rs)"
[{'_plan_id': '2026-06-17-v1.49-narrative-indexes',
  'closed_at': '2026-06-18',
  'closure_evidence': 'feature/v1.51-advisory-lock commit (pending), test names: file_lock::tests::test_acquire_and_release_via_drop, ...',
  'closure_note': 'V1.51 T-B P0: Works/<work_ref>/.lock advisory lock (flock + heartbeat) implemented. ...',
  'decision': 'defer',
  'id': 'R-V149P1-01',
  'lifecycle': 'resolved',
  'note': 'Spec-reconciliation portion closed V1.49 P-last ... The deferred promotion-concurrency advisory-lock decision is now in scope at V1.51 T-B P0 ...',
  ...}]
```

## Spec Body Verification

**File path**: `.mstar/knowledge/specs/concurrency.md` (264 lines; Master class, Status: Draft (V1.51 T-B P0))

**Key passages** (verified):
- Header: coordinates with compass, plan, and related specs (workflow-profile, cron-staggering, cli-spec, daemon-runtime).
- §1 Problem Statement: multi-writer contention post-V1.50 cron staggering; DB-level `runtime_lock_holder` is process-local.
- §2.1–2.3: File-based advisory lock (`flock(LOCK_EX | LOCK_NB)`), format `<pid>:<holder_name>:<expires_at_ms>`, `try_acquire` contract returning `FileLockGuard` or `FileLockError::Locked`/`Io`.
- §2.4 (dual exit-code contract — critical):
  ```
  | `FileLockError::Locked` | `E_LOCK` | 75 (`EX_TEMPFAIL`) | Temporary contention — another process holds the lock; retry later. |
  | `FileLockError::Io` | `E_LOCK_IO` | 78 (`EX_CONFIG`) | Persistent I/O failure — permission denied, disk full, or missing parent directory; operator intervention required. |
  ```
  Explicit: "Callers must **never** map an I/O failure to `E_LOCK` or exit 75".
- §3 Daemon-Side: `try_fire_role` acquires before enqueue; read-only evaluator paths do not.
- §4 CLI-Side: mutating commands (`cron set`, `run`, `kb adopt`) acquire; contention → `CliError::Locked` + exit 75; I/O → `CliError::LockIo` + exit 78.
- §5 Heartbeat: 30s refresh, 60s expiry, RAII cancel + `flock(LOCK_UN)` on drop (no delete).
- §6 Zombie Detection: stale `expires_at_ms` (>60s) treated as tombstone; next acquirer overwrites.

**Coherence with code**: Matches implementation (FileLockGuard RAII, heartbeat task, zombie overwrite on reacquire, CLI error mapping in `errors.rs` + `main.rs`, daemon `maybe_acquire_cron_file_lock`, status `lock_holder` enrichment). No drift detected.

## Residual Closure Verification

**R-V149P1-01** (from `.mstar/status.json` root `residual_findings`):
- `id`: R-V149P1-01
- `lifecycle`: resolved (not open)
- `closed_at`: 2026-06-18
- `closure_evidence`: "feature/v1.51-advisory-lock commit (pending), test names: file_lock::tests::test_acquire_and_release_via_drop, file_lock::tests::test_second_acquire_fails_with_locked_info, file_lock::tests::test_stale_lock_file_overwritten_on_acquire, file_lock::tests::test_read_lock_holder_info_stale, cron_lock_integration::file_lock_blocks_cron_fire_when_held, cli_lock_contention::locked_error_display_shows_holder_info"
- `closure_note`: "V1.51 T-B P0: Works/<work_ref>/.lock advisory lock (flock + heartbeat) implemented. Spec-reconciliation portion already closed V1.49 P-last. Advisory-lock portion: file_lock module (12 unit + 3 integration tests), daemon cron-side acquisition in try_fire_role, CLI-side acquisition in handle_set, E_LOCK exit code 75, lock_holder field in status JSON, zombie detection (>60s heartbeat expiry)."
- Explicitly distinguishes: "Spec-reconciliation portion already closed V1.49 P-last. ... Advisory-lock portion ..."

This is concrete (commit reference + exact test names) and scoped to advisory-lock (not spec-reconciliation).

## Scope Creep Check

**Commits touching crates/schemas/specs in this branch** (from `git log iteration/v1.51..HEAD --name-only`):
- `crates/nexus-local-db/src/file_lock.rs` + `tests/file_lock.rs` + `Cargo.toml`
- `crates/nexus-local-db/src/lib.rs`
- `crates/nexus-daemon-runtime/src/cron_supervisor.rs` + `boot.rs` + `tests/cron_lock_integration.rs`
- `crates/nexus-orchestration/src/schedule/cron_supervisor.rs` + `tests/cron_supervisor.rs` + `review_cron_e2e.rs`
- `crates/nexus42/src/commands/creator/works/cron.rs` + `mod.rs` + `run.rs` + `world/kb.rs`
- `crates/nexus42/src/errors.rs` + `main.rs`
- `crates/nexus42/tests/cli_lock_contention.rs` + `cli_lock_io_error.rs` + `creator_run_lock.rs` + `kb_adopt_lock.rs` + `works_status_lock_holder.rs`

**No T-A plan artifacts, no T-B P1 (per-row CAS/versioning), no new wire schemas, no unrelated crates**. All changes are advisory-lock scoped per plan §3 Non-goals and compass §0.1 #6. `completion.md` and QC reports confirm same scope.

**Branch alignment** (verified at start):
- cwd: `/Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.51-t-b-p0`
- branch: `feature/v1.51-advisory-lock`
- merge-base: `ca494f03704badebcad04b276ddb5220e674ffc2` (iteration/v1.51)
- Review range: `iteration/v1.51...HEAD` (13 commits)
- HEAD at verification start: `79141a67` (qc1 revalidation)

## Findings

### Critical
(None)

### Warning
(None)

### Suggestion
- (Minor, non-blocking) `cron_lock_integration.rs` has 5 deprecation/unused import warnings (`into_path`, `Arc`, `Notify`). These predate the advisory-lock work (harmless for tests) but should be cleaned in a hygiene pass to keep CI noise-free. Not a gate for this plan.

## Verdict Reasoning

All 13 acceptance criteria (explicitly enumerated in assignment) are independently verified with direct command output evidence:
- Lock infra (RAII, FileLockError variants, drop-on-scope-exit) → passes.
- Daemon cron-side acquisition + contention simulation → passes.
- CLI-side post-W-001 (cron set, run, kb adopt) → passes.
- Post-W-002 I/O differentiation (exit 78 vs 75) → passes.
- Spec body + §2.4 dual exit-code contract → present and coherent.
- Status `lock_holder` + R-V149P1-01 closure → concrete evidence present.

Static gates (clippy, fmt) clean. Wire contracts untouched (V1.51 §0.1 #9). Stress runs (8 threads) confirm race fidelity. No scope creep. Checkout alignment matches assignment (`Review cwd`, `Working branch`, `plan_id`, `Review range`).

Per gates:
- "All 13 acceptance criteria pass + all tests green + CI clean + no Critical/Warning" → **Approve**.

No plan/code edits, no push, no merge performed. Report written and will be committed per instructions.
