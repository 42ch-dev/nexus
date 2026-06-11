---
report_kind: qa-verification
plan_id: 2026-06-10-v1.41-multi-work-switch
verdict: Approve
generated_at: 2026-06-10T20:26:16+08:00
review_range: "merge-base: 55689706 → tip: 9b6627dd"
working_branch_verified: iteration/v1.41
review_cwd_verified: /Users/bibi/workspace/organizations/42ch/nexus
mode: full
---

# QA Verification Report — V1.41 P0 (DF-60 multi-work lifecycle)

## Reviewer Metadata
- Reviewer: @qa-engineer
- Runtime Agent ID: qa-engineer
- Runtime Model: volcengine-plan/ark-code-latest
- Review Perspective: Behavior verification against acceptance criteria
- Report Timestamp: 2026-06-10T20:26:16+08:00

## Scope
- plan_id: 2026-06-10-v1.41-multi-work-switch
- Review range / Diff basis: merge-base: 55689706 → tip: 9b6627dd
- Working branch (verified): iteration/v1.41
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Tools run: cargo test (4 P0 crates), cargo clippy, cargo +nightly fmt --check, AC-targeted hermetic checks, residual register audit, spec amendment grep, workspace regression suite

## Checkout gate

```text
$ git rev-parse --show-toplevel
/Users/bibi/workspace/organizations/42ch/nexus
$ git branch --show-current
iteration/v1.41
$ git rev-parse --verify 9b6627ddba9a1545bf8f0c3c48e0faeb7c73d433
9b6627ddba9a1545bf8f0c3c48e0faeb7c73d433
$ git rev-parse --verify 556897061f625c53cd172e2bdb40d509dac61775
556897061f625c53cd172e2bdb40d509dac61775
```

## Source materials read

- `qc-consolidated.md`: final QC verdict Approve after fix-wave re-review; 12 residuals registered.
- `qc1.md`: initial Request Changes, revalidation Approve for F-001/F-002/F-003/F-005; F-004 deferred as R-V141P0-01.
- `qc2.md`: Approve; six suggestions tracked.
- `qc3.md`: initial Request Changes, revalidation Approve for W1/W2/W3/W4.
- `completion-report.md`: initial implementation + fix-wave closeout.
- Plan §4 acceptance criteria and §5 T1–T9.
- Iteration compass §1.1 rows 2–6.
- Primary spec `novel-multi-work-lifecycle.md`, especially §3.2 DB SSOT amendment.

## Acceptance criteria verification

| AC | Description | Status | Evidence |
|----|-------------|--------|----------|
| AC1 | Completion ceremony writes lock + stops auto-chain on that Work | PASS | `cargo test -p nexus-orchestration --test multi_work_switch -- --nocapture`: `test_completion_two_step_ceremony_writes_lock_and_updates_pool ... ok`; `test_auto_chain_skips_completion_locked_work ... ok`; 3/3 passed. |
| AC2 | Other Works continue concurrently | PASS | Same suite: `test_other_work_continues_after_completion ... ok`; confirms Work B remains active/unlocked after Work A completion. |
| AC3 | `creator works list\|status` replace `creator run list\|status` | PASS | `cargo test -p nexus42 --test command_surface_contract v141_`: `v141_creator_works_subcommands ... ok`; run list/status removal test is in the command-surface contract suite and the v141 subset passed 5/5. |
| AC4 | `creator works use` sets pool `active` on `novel_pool_entries` | PASS | `cargo test -p nexus-daemon-runtime --test multi_work_switch` passed 3/3 for daemon guards; targeted code/SQL smoke verified `POST /v1/local/works/pool` handler exists, `set_pool_active_inner` demotes prior active to queued and upserts target active, and the SQL smoke produced `[('wrk_a', 'queued'), ('wrk_b', 'active')]` with `pool_active_sql_smoke PASS`. |
| AC5 | `run start --from-work` records lineage without reviving completed Work chain | PASS | `cargo test -p nexus42 --test command_surface_contract v141_`: `v141_creator_run_start_from_work_flags ... ok`; `v141_run_start_from_work_accepts_work_id ... ok`. Static smoke verified CLI emits `lineage_from_work_id`, daemon `CreateWorkRequest` contains `lineage_from_work_id`, and new `WorkRecord` persists `lineage_from_work_id: req.lineage_from_work_id`; no source Work mutation path is present in `create_work`. |
| AC6 | Runtime lock rejects concurrent mutate | PASS | `cargo test -p nexus-daemon-runtime --test multi_work_switch`: `test_runtime_lock_rejects_concurrent_mutate ... ok`; suite 3/3 passed. |
| AC7 | Hermetic tests cover happy path + lock conflict | PASS | Orchestration suite covers completion happy path + other-work concurrency + auto-chain skip (3/3); daemon suite covers completion-lock conflict + runtime-lock conflict + post-completion patch block (3/3); CLI command-surface v141 suite covers IA and lineage flags (5/5). |

## CI / static analysis

```text
$ cargo test -p nexus42 -p nexus-daemon-runtime -p nexus-orchestration -p nexus-local-db 2>&1 | tail -50
...
test result: ok. 15 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.14s
...
Doc-tests nexus_daemon_runtime: ok. 1 passed; 0 failed; 1 ignored
Doc-tests nexus_local_db: ok. 2 passed; 0 failed
Doc-tests nexus_orchestration: ok. 1 passed; 0 failed; 3 ignored
Doc-tests nexus42: ok. 1 passed; 0 failed; 1 ignored

$ cargo clippy -p nexus42 -p nexus-daemon-runtime -p nexus-orchestration -p nexus-local-db -- -D warnings 2>&1 | tail -20
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.26s

$ cargo +nightly fmt --all -- --check 2>&1 | tail -5
(no output)
```

## AC-targeted command evidence

```text
$ cargo test -p nexus-orchestration --test multi_work_switch -- --nocapture 2>&1 | tail -40
running 3 tests
test test_other_work_continues_after_completion ... ok
test test_auto_chain_skips_completion_locked_work ... ok
test test_completion_two_step_ceremony_writes_lock_and_updates_pool ... ok
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.12s

$ cargo test -p nexus42 --test command_surface_contract v141_ 2>&1 | tail -30
running 5 tests
test v141_resume_reopen_without_reason_rejects ... ok
test v141_creator_run_resume_reopen_flags ... ok
test v141_creator_works_subcommands ... ok
test v141_creator_run_start_from_work_flags ... ok
test v141_run_start_from_work_accepts_work_id ... ok
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 43 filtered out; finished in 1.56s

$ cargo test -p nexus-daemon-runtime --test multi_work_switch 2>&1 | tail -40
running 3 tests
test test_runtime_lock_rejects_concurrent_mutate ... ok
test test_completion_ceremony_blocks_subsequent_patch ... ok
test test_completion_lock_blocks_mutation ... ok
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.11s

$ python3 AC4 pool-active SQL smoke
[('wrk_a', 'queued'), ('wrk_b', 'active')]
pool_active_sql_smoke PASS

$ python3 AC5 lineage static smoke
cli_from_work_json True
cli_set_default_json True
request_has_lineage True
record_persists_lineage True
pool_handler_route_target True
pool_transaction_sql True
```

Note: the first pool SQL smoke attempt failed with `sqlite3.OperationalError: cannot start a transaction within a transaction` because Python sqlite opened an implicit transaction for seed inserts before the explicit `BEGIN`. Root cause was the smoke harness, not product code. Rerun after committing seed rows before `BEGIN` passed as shown above.

## Residual register audit

```text
count 12
ids R-V141P0-01,R-V141P0-02,R-V141P0-03,R-V141P0-04,R-V141P0-05,R-V141P0-06,R-V141P0-07,R-V141P0-08,R-V141P0-09,R-V141P0-10,R-V141P0-11,R-V141P0-12
missing_fields none
severity_ok True []
decision_ok True []
```

- Total residuals registered: 12
- Canonical fields present: yes (`id`, `title`, `severity`, `source`, `scope`, `decision`, `owner`, `target` present on all 12)
- Severity enum compliance: yes (all in `critical|high|medium|low|nit`)
- Decision enum compliance: yes (all in `defer|accept|risk-accepted|accept-with-fix`)
- Specific notes: R-V141P0-01 is intentionally reclassified as `high` for stale runtime lock recovery; R-V141P0-07 is `accept-with-fix` and points to the fix wave; R-V141P0-10 captures pre-existing `repeated_sweeps_remain_stable` flakiness and no flake appeared during QA's canonical or workspace test runs.

## Spec amendment verification

- File: `.mstar/knowledge/specs/novel-multi-work-lifecycle.md` §3.2
- DB SSOT declaration present: yes
- File as derived artifact declared: yes

```text
$ grep -n "completion_locked_at\|authoritative\|derived artifact" .mstar/knowledge/specs/novel-multi-work-lifecycle.md
80:1. Removes file + clears `works.completion_locked_at`.
86:DB column `works.completion_locked_at` is the authoritative lock state. The `.completion-lock.json` file is a derived artifact for cross-tool observation. The supervisor gates ticks on the DB column. If the file exists but the DB column is NULL, the supervisor treats the work as unlocked. If the file is missing but the DB column is set, the supervisor treats the work as locked.
```

## Regressions

- Any test that passed before and now fails: no
- Broader regression command run:

```text
$ cargo test --workspace 2>&1 | tail -30
...
Doc-tests nexus_orchestration: ok. 1 passed; 0 failed; 3 ignored
Doc-tests nexus42: ok. 1 passed; 0 failed; 1 ignored
```

The workspace suite completed successfully in this QA run; no `repeated_sweeps_remain_stable` flake was observed.

## Findings (if any)

### Critical
(none)

### Warning
(none)

### Suggestion
- Add a future true CLI-to-daemon integration test for `creator works use` / `completion-lock release` as already tracked by R-V141P0-04. Current QA accepted code-level and SQL smoke evidence for AC4 because the route/handler and transactional pool SQL are present and canonical Rust suites are clean.

## Verdict

**Approve**

**Rationale**: All 7 P0 acceptance criteria are satisfied by hermetic Rust tests and targeted QA smoke checks; scoped CI, clippy, fmt, and the broader workspace test suite are clean. The residual register contains the expected 12 entries with canonical fields/enums, and the F-005 spec amendment correctly declares DB `completion_locked_at` as SSOT with `.completion-lock.json` as derived artifact.
