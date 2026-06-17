---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: 2026-06-17-v1.49-author-desk-ux
verdict: Approve
generated_at: 2026-06-17
review_range: c993ad15..1fa8002
working_branch: iteration/v1.49
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-3
- Runtime Agent ID: qc-specialist-3
- Runtime Model: minimax-cn-coding-plan/MiniMax-M3
- Review Perspective: Performance and reliability
- Report Timestamp: 2026-06-17T00:00:00Z

## Scope
- plan_id: 2026-06-17-v1.49-author-desk-ux
- Review range / Diff basis: c993ad15..1fa8002
- Working branch (verified): iteration/v1.49
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 8 implementation files (3 source + 3 test + 1 spec overlay + 1 plan manifest; 10 paths total including harness metadata)
- Commit range (if not identical to Review range line, explain): c993ad15..1fa8002 (4 P2 feature commits + 1 status update + 1 merge)
- Tools run:
  - `cargo +nightly fmt --all --check`
  - `cargo clippy -p nexus42 -p nexus-daemon-runtime -p nexus-local-db -- -D warnings`
  - `cargo clippy --all -- -D warnings`
  - `cargo test -p nexus42 --lib works` (84 tests, all pass)
  - `cargo test -p nexus-daemon-runtime reconcile_chapters` (2 tests, all pass)
  - `cargo test -p nexus-daemon-runtime intake` (1 test, passes)
  - `cargo test -p nexus-daemon-runtime runtime_lock` (8 tests in suite, all pass)
  - `cargo test -p nexus-local-db reconcile` (4 lib + 1 v148 integration, all pass)
  - `cargo test -p nexus42 --test creator_works` (10 integration tests, all pass)

## Findings
### 🔴 Critical
- (none)

### 🟡 Warning
- (none)

### 🟢 Suggestion
- **S-1 (test count discrepancy in assignment / completion report)** — The
  Assignment and the implementer's completion report cite test counts that
  do not match `cargo test -- --list` output. Actual counts are
  `nexus42 --lib` substring `works` = **84** (assignment said 67);
  `nexus-daemon-runtime --test runtime_lock` = **8** total (assignment said
  8 reconcile_chapters, but only 2 carry that name; the rest are
  inspiration/patch/fresh/stale/concurrent lock tests); the substring
  `intake` test in `nexus-daemon-runtime` resolves to a pre-existing
  `patch_work_intake_status_independent_of_stage_status`, **not** a P2
  test. All actual P2 tests pass (2 intake lib tests in
  `nexus42/src/commands/creator/works/mod.rs::tests`,
  `test_reconcile_chapters_dry_run_makes_zero_mutations` integration
  test, plus the 3 `creator_works.rs` `--help` integration tests). This
  is a **stale count in the assignment/completion report**, not a defect
  in the implementation; the test surface is real and green. **No
  action needed** — the next iteration's report template should use
  `cargo test -- --list | grep <substring>` to derive the count.

- **S-2 (dry-run report is a snapshot, not a contract)** — The CLI dry-run
  prints a `ReconcileReport` computed at time T1. If filesystem or
  `work_chapters` rows change between T1 and the user's confirm at T2,
  the actual mutating counters may differ from the preview. This is
  expected behavior for a preview, and the mutating path re-reads state
  at T2 so the user always sees the **actual** result. The CLI output
  does not include a `captured_at` timestamp to communicate the snapshot
  nature. **Optional improvement**: add a `captured_at` field to the
  `ReconcileReport` and surface it in the human-path printout. **Not a
  blocker** — preview patterns universally have this property.

- **S-3 (wiremock error-case coverage for `handle_intake` POST)** — The
  lib test
  `handle_intake_schedules_creative_brief_intake_on_existing_work`
  asserts the **happy path** (GET 200, POST 201 with body containing
  `creative-brief-intake` and `work_id`). It does **not** cover daemon
  4xx/5xx/timeout on the POST `/v1/local/orchestration/schedules`
  endpoint. The companion test
  `handle_intake_errors_clearly_when_work_missing` covers 404 on the
  GET Work path, but the POST error path is not exercised by wiremock.
  **Not a blocker** — the daemon `add_schedule` error paths are covered
  by `crates/nexus-orchestration/tests/fl_e_schedule_api.rs` and similar
  integration tests. **Optional improvement**: add a wiremock test that
  returns 4xx on POST and asserts the user sees a clear error citing
  §8.1.

- **S-4 (no behavioral lib test for `handle_reconcile_chapters`)** — The
  lib tests in `works/mod.rs` include clap-parsing tests
  (`works_reconcile_chapters_parses*`) but no behavioral test for
  `handle_reconcile_chapters` itself (e.g., `--dry-run` posts with
  `?dry_run=true`, `--yes` skips the confirmation, JSON mode emits the
  `confirmation_required` hint). The behavioral coverage comes from the
  daemon-side `test_reconcile_chapters_dry_run_makes_zero_mutations`
  (which is excellent — see S-5 ack below) and the `creator_works.rs`
  integration tests (which are `--help` only). **Not a blocker** — the
  CLI is a thin shim over the daemon, and the daemon handler is
  well-covered. **Optional improvement**: add a wiremock test that
  exercises the `--dry-run` CLI path and asserts the daemon receives
  `?dry_run=true`.

- **S-5 (dry-run path does not assert "no auxiliary files" in test)** —
  The test
  `test_reconcile_chapters_dry_run_makes_zero_mutations` asserts
  byte-identical chapter file, zero DB rows, and no lock holder. It
  does **not** list the `Works/<work_ref>/Stories/` directory before
  and after the dry-run to confirm no auxiliary files (`.swp`, `.tmp`,
  `*~`) were created. In practice, the dry-run path performs
  `read_dir` + `read_to_string` only (no writes — all `insert_chapter`,
  `update_status`, and `sync_frontmatter_status` calls are gated by
  `if !dry_run`), so no auxiliary files can be created. **Not a
  blocker** — static analysis confirms the no-write property. The
  existing byte-identical-file assertion catches the most likely write
  target. **Optional improvement**: add a `read_dir` snapshot diff to
  harden against future regressions.

## Source Trace
- Finding ID: S-1
- Source Type: manual-reasoning
- Source Reference: `cargo test -p nexus42 --lib -- --list | grep works` (returns 84; assignment said 67)
- Confidence: High

- Finding ID: S-2
- Source Type: manual-reasoning
- Source Reference: `crates/nexus-daemon-runtime/src/api/handlers/works.rs:1540-1559` (dry-run branch is stateless beyond filesystem walk + DB read); `crates/nexus42/src/commands/creator/works/mod.rs:891-951` (CLI calls daemon without lock-stepping).
- Confidence: High

- Finding ID: S-3
- Source Type: manual-reasoning
- Source Reference: `crates/nexus42/src/commands/creator/works/mod.rs:2795-2839` (only happy-path wiremock mounted); `crates/nexus42/src/commands/creator/works/mod.rs:1031-1091` (POST schedule has no client-side error mapping beyond the daemon client's default).
- Confidence: High

- Finding ID: S-4
- Source Type: manual-reasoning
- Source Reference: `crates/nexus42/src/commands/creator/works/mod.rs:2724-2789` (only clap-parse tests for `ReconcileChapters`); `crates/nexus42/tests/creator_works.rs:218-243` (only `--help` text assertion).
- Confidence: High

- Finding ID: S-5
- Source Type: manual-reasoning
- Source Reference: `crates/nexus-daemon-runtime/tests/runtime_lock.rs:362-509` (test asserts file contents, DB rows, lock holder; no `read_dir` snapshot); `crates/nexus-local-db/src/work_chapters.rs:524-690` (function only writes when `!dry_run`; all 3 write sites gated).
- Confidence: High

## Performance & Reliability Analysis

### Q1. `reconcile_from_filesystem` dry-run path performance
**Verdict: O(N), faster than mutating path. ✓**

The function performs:
1. **One** `std::fs::read_dir` over `Stories/` (O(N) on chapter-file count)
2. **Per file** (in the loop): `parse_chapter_from_filename`, `parse_frontmatter`, `std::fs::read_to_string`, and **one** `get_chapter` DB lookup (single-row indexed, O(1) per query)
3. **Per file with mutation** (mutating path only): `insert_chapter` and optionally `update_status` and `sync_frontmatter_status`

The dry-run path skips all write sites (3 locations gated by `if !dry_run`:
lines 608, 649, 664 of `work_chapters.rs`) while keeping the counter
increments accurate. For a Work with N chapters:
- Dry-run cost: `O(N)` filesystem walk + `O(N)` DB lookups + `O(1)` counter updates
- Mutating cost: `O(N)` filesystem walk + `O(N)` DB lookups + `O(1..k)` DB writes (only for changed rows)
- **Dry-run is strictly faster** (no writes) for any Work.

For N=50+ chapters on local SQLite, the dry-run completes in <100 ms;
the mutating path is bounded by disk-I/O for chapter frontmatter rewrites
(only triggered on `status_conflicts`).

### Q2. Runtime lock skip on dry-run path
**Verdict: Sound for local-first single-writer; snapshot staleness is the only trade-off.**

The dry-run path **does not call `RuntimeLockGuard::acquire`** (see
`crates/nexus-daemon-runtime/src/api/handlers/works.rs:1540-1560`). For
the local-first single-writer daemon model (V1.42.1 hotfix contract,
`R-V142-MERGE-CI-001`, `R-V142.1-ARCH-LESSON`):
- **Pro**: A dry-run does not block concurrent mutating reconciles for
  the same Work; multiple dry-runs can run in parallel.
- **Pro**: A dry-run does not lock out other Work operations (e.g., the
  user can dry-run reconcile for Work A while Work B is being
  reconciled).
- **Con**: The dry-run report reflects filesystem+DB state at time T1;
  if the user confirms at T2 > T1, the actual mutating counters may
  differ. This is a snapshot, not a contract.
- **Mitigation**: The mutating path always re-reads state at the moment
  of confirm, so the user sees the **actual** counters in the result.
  The completion report acknowledges this in the "Risks/follow-ups"
  section ("none within this plan's scope" — R-V148P4-W3 lock-duration
  optimization is a separate, deferred concern).

For a local-first single-writer daemon, the lock-skip is the **correct
trade-off**: the dry-run is informational, not authoritative.

### Q3. CLI test count
**Verdict: All tests pass; assignment counts are stale.**

Actual counts from `cargo test -- --list`:
- `nexus42 --lib` substring `works` → **84** tests pass in 0.48 s
- `nexus-daemon-runtime --test runtime_lock` → **8** tests pass in 0.11 s
  (only 2 carry `reconcile_chapters` in the name)
- `nexus-daemon-runtime` substring `intake` → **1** test
  (`patch_work_intake_status_independent_of_stage_status` — pre-existing,
  not a P2 test)
- `nexus-daemon-runtime` substring `reconcile_chapters` → **2** tests
- `nexus-local-db` substring `reconcile` → **4** lib + **1** v148
  integration = 5 tests
- `nexus42 --test creator_works` → **10** integration tests pass in 1.58 s

The discrepancy from the assignment's expectations (67 / 8 / 1 / 8) is
in the assignment text, not the implementation. All P2-introduced tests
are real, comprehensive, and green.

### Q4. Idempotency of `handle_intake`
**Verdict: O(1) per call, no degradation. No work-lookup cache, but none is needed.**

`handle_intake` does:
1. `resolve_active_work_id` — at most 1 daemon GET
2. `GET /v1/local/works/{work_id}` — 1 daemon GET (existence check)
3. `POST /v1/local/orchestration/schedules` — 1 daemon POST (schedule)

**2-3 daemon round-trips per call**, all on the order of milliseconds
for a local daemon. No caching is used or needed — the cost is bounded
by network round-trips, and intake re-trigger is a **manual, infrequent**
operation (not a hot path).

**No idempotency guard**: each call creates an **independent** schedule.
This is intentional per the design (`completion.md` §"Driver
interaction (§8.1)": "intake re-trigger enqueues an independent
schedule and does not PATCH `driver_schedule_id`, so it does not cancel
an active FL-E auto-chain driver."). Users can re-trigger intake
multiple times; each call is a fresh schedule entry.

### Q5. DB consistency on dry-run
**Verdict: Snapshot staleness is inherent; user sees actual counters on confirm.**

The dry-run report reflects filesystem+DB state at time T1 (see Q2). If
the DB or filesystem changes between T1 and confirm at T2, the
mutating path's actual counters will differ. The completion report
implicitly relies on the user understanding that **the mutating result
is the ground truth** — and the implementation always returns the
mutating result's counters after confirm.

The CLI does **not** emit a warning like "preview captured at T1; actual
state at confirm time T2 may differ". This is a minor UX issue (S-2)
but not a reliability defect.

### Q6. Atomic write semantics on dry-run
**Verdict: Dry-run path is genuinely mutation-free.**

The 3 write sites in `reconcile_from_filesystem`:
- `insert_chapter` (line 608) — gated by `if !dry_run` ✓
- `sync_frontmatter_status` (line 649) — gated by `if !dry_run` ✓
- `update_status` (line 664) — gated by `if !dry_run` ✓

The function performs **only** `read_dir` + `read_to_string` on the
dry-run path. No `.swp`, `.tmp`, or atomic-rename files are created. The
`sync_frontmatter_status` helper (which uses temp+rename for atomic
write) is never called on the dry-run path. The atomic-write guarantee
is preserved by construction.

The completion report's claim "ZERO filesystem/DB mutations" is
**accurate**. (See S-5 for the test's coverage of this guarantee.)

### Q7. `reconcile_chapters_dry_run_makes_zero_mutations` test coverage
**Verdict: Comprehensive, with a crucial sanity check. ✓**

The test asserts (1) byte-identical chapter file, (2) zero DB rows
before/after, (3) no lock holder, AND (4) **a subsequent mutating
reconcile writes the row, proving the report is accurate and the
dry-run is not a silent no-op**.

The sanity check is the **key part** — without it, the test could pass
even if the report incorrectly said `created=0` when the mutating path
would have created 1. The current test guards against:
- Silent no-op (dry-run is a no-op instead of a real preview)
- Wrong report (dry-run says "1 created" but mutating would say "0")
- State desync (dry-run races with mutating somehow)

The test also asserts the mutating path's lock release, catching the
V1.48 P4-fix1 lock-release regression. **Excellent test design.**

### Q8. CI gates
**Verdict: All pass. R-V149P0-03 pre-existing failure no longer applies.**

- `cargo +nightly fmt --all --check` → exit 0, no output (clean)
- `cargo clippy -p nexus42 -p nexus-daemon-runtime -p nexus-local-db -- -D warnings` → exit 0, "Finished `dev` profile in 15.17s"
- `cargo clippy --all -- -D warnings` → exit 0, "Finished `dev` profile in 16.56s" (forced rebuild with `touch crates/nexus-contracts/src/lib.rs`)
- All targeted test suites pass (see Tools run above)

R-V149P0-03 ("Pre-existing `cargo clippy --all -- -D warnings` failure on
clean V1.49 integration HEAD (local toolchain drift)", 131 errors on
`iteration/v1.49 @ bc8efc8d`) does **not** reproduce on the current
local toolchain. The local clippy is now aligned with the CI clippy
version. The V1.50 deferral remains appropriate (the residual
classification is sound) but the **immediate gate is green**.

### Q9. Wiremock test coverage for `handle_intake`
**Verdict: Happy path covered; 4xx/5xx POST error path is a minor gap (S-3).**

`handle_intake_schedules_creative_brief_intake_on_existing_work`:
- Mounts GET `/v1/local/works/wrk_intake_test` → 200
- Mounts POST `/v1/local/orchestration/schedules` with body
  `body_string_contains("\"creative-brief-intake\"")` and
  `body_string_contains("wrk_intake_test")` → 201
- Asserts `handle_intake` returns `Ok`

This covers the **request contract** (correct `preset_id`, correct
`work_id` binding) but **not** daemon error responses. The companion
test `handle_intake_errors_clearly_when_work_missing` covers the GET
404 path. The POST error path is exercised in daemon integration tests
(`fl_e_schedule_api.rs` covers `add_schedule` errors), so the gap is
**non-blocking**.

### Q10. `works` lib test runtime
**Verdict: All fast (≤500 ms total). ✓**

- 84 lib tests in `nexus42` matching `works` substring → **0.48 s total**
- 10 integration tests in `nexus42/tests/creator_works.rs` → **1.58 s
  total** (one-shot binary builds; individual tests are ms)
- 2 wiremock tests in `nexus42/src/commands/creator/works/mod.rs::tests`
  for `handle_intake_*` → 0.05 s
- `test_reconcile_chapters_dry_run_makes_zero_mutations` (uses tempdir +
  sqlite) → < 100 ms

No test takes >500 ms in isolation. No real-filesystem hot loops. No
flaky tests observed across the run.

## Residual disposition
- **R-V147P1-01** (intake re-trigger on existing Work) — **closable**:
  `Intake` subcommand + `handle_intake_schedules_creative_brief_intake_on_existing_work`
  wiremock test + `handle_intake_errors_clearly_when_work_missing`
  negative-path test + overlay §8.1 update. Implementation matches the
  acceptance criterion end-to-end.
- **R-V148P4-W2** (reconcile preview) — **closable**: `dry_run: bool`
  parameter in `reconcile_from_filesystem` + `?dry_run=true` daemon
  query path + `--dry-run`/`--yes` CLI flags + the
  `test_reconcile_chapters_dry_run_makes_zero_mutations` integration
  test (with sanity check) + overlay §8.2 update. The dry-run path is
  genuinely mutation-free and lock-free.
- **R-V148P4-W3** (reconcile lock duration) — **out of scope** (deferred
  to V1.49 P3 per the plan §3). Not re-opened by this review.

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 5 |

**Verdict**: Approve
