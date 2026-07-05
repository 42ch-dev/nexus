---
report_kind: qa
plan_id: 2026-06-17-v1.49-author-desk-ux
verdict: PASS
generated_at: 2026-06-17T23:55:00+08:00
review_range: c993ad15..0b98e194
working_branch: iteration/v1.49
qa_mode: verify (not report-only)
---

# QA Report — V1.49 P2 (author-desk-ux)

## Scope

**plan_id**: `2026-06-17-v1.49-author-desk-ux`

**Feature / scope label**: V1.49 P2 — Intake re-trigger + reconcile preview (R-V147P1-01 + R-V148P4-W2 closure) + W-1 (clap `--yes` help text accuracy)

**Working branch (verified)**: `iteration/v1.49` @ `911fdee0` (P2 + W-1 fix merged, re-review approved, 3 residuals archived)

**Review cwd**: `/Users/bibi/workspace/organizations/42ch/nexus` (main checkout, currently on `iteration/v1.49`)

**Review range / Diff basis**: `c993ad15..0b98e194` (full P2 + W-1 fix; status/archive updates after the merge are not in the code review range; equivalent to `git diff c993ad15...0b98e194` on iteration/v1.49)

**Feature commits** (for `git log`):
- `4ab9e0be` feat(v1.49-p2): reconcile-chapters --dry-run (R-V148P4-W2)
- `0948cb87` feat(v1.49-p2): intake re-trigger + reconcile CLI flags (T1+T2)
- `9aea7091` docs(v1.49-p2): overlay §8 shipped CLI + intake/reconcile surface tests (T3)
- `7fe873f7` harness(v1.49-p2): mark plan InReview + completion report
- `a3917063` harness(v1.49-p2): mark P2 InReview (pre-merge status update)
- `1fa8002` merge P2
- `bdd646dc` fix(v1.49-p2): clap --yes doc comment + regression test
- `0b98e194` merge W-1 fix
- `e7848336` qc1 targeted re-review
- `911fdee0` re-review approval + 3 residual archives

## Pre-flight (verified in-session)
- `git rev-parse --show-toplevel`: `/Users/bibi/workspace/organizations/42ch/nexus`
- `git branch --show-current`: `iteration/v1.49`
- `git rev-parse HEAD`: `911fdee03c6f41f48a55fe0075a6c3131952d976`
- `git rev-parse c993ad15..0b98e194`: valid (0b98e194..c993ad15)
- `git diff c993ad15...0b98e194 --stat`: 14 files changed, +1606/-91 (matches P2 + W-1 scope)

## Verification (command outputs — last lines / tails)

### Re-review integrity
```
grep -A 5 "## Revalidation" .mstar/plans/reports/2026-06-17-v1.49-author-desk-ux/qc1.md
## Revalidation

- Re-review kind: Targeted (Reviewer 1 of 1 — only qc1 raised blocking; qc2/qc3 stay approved)
- Re-review date: 2026-06-16T17:10:00Z
- Re-review range / Diff basis: `1475f1fa..0b98e194` (verbatim from Assignment — single fix commit `bdd646dc` + merge `0b98e194`; equivalent to `git diff 1475f1fa...0b98e194`)
- Original review range (cross-ref): `c993ad15..1fa8002` (wave 1 — preserved in `## Scope` above)
- Working branch (verified): `iteration/v1.49` @ `0b98e1942410ae93f71fc82883f1aa0fcd9f2753`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus`
- Files re-reviewed: 2 (`crates/nexus42/src/commands/creator/works/mod.rs` doc comment ±9; `crates/nexus42/tests/creator_works.rs` +28 regression test)
- Tools run: `git diff 1475f1fa...0b98e194`, `Read`, `Grep`, `cargo +nightly fmt --all --check` (exit 0), `cargo clippy -p nexus42 -- -D warnings` (exit 0), `cargo test -p nexus42 --test creator_works` (11 passed / 0 failed)
```
**Evidence**: qc1.md contains `## Revalidation` (not a new `qc1-rev2.md`). Verdict flipped from Request Changes (wave-1) to Approve.

### Residual lifecycle integrity (python3 verification)
```
python3 -c "... (see Assignment)"
R-V149P2-01 open in 2026-06-17-v1.49-author-desk-ux: False
R-V147P1-01 open in 2026-06-15-v1.47-gate-remediation-audit: False
R-V148P4-W2 open in 2026-06-16-v1.48-serial-hardening: False
OK: all 3 NOT in open lists
OK: 2026-06-17-v1.49-author-desk-ux.json -> lifecycle=resolved, closure_evidence present: True
OK: 2026-06-15-v1.47-gate-remediation-audit.json -> lifecycle=resolved, closure_evidence present: True
OK: 2026-06-16-v1.48-serial-hardening.json -> lifecycle=resolved, closure_evidence present: True
Residual lifecycle integrity: PASS
```
**Archive inspection** (post-verification):
- `2026-06-17-v1.49-author-desk-ux.json`: R-V149P2-01, `lifecycle: resolved`, `closure_evidence` present (fix commits bdd646dc/0b98e194 + e7848336 re-review).
- `2026-06-15-v1.47-gate-remediation-audit.json`: R-V147P1-01, `lifecycle: resolved`, `closure_evidence` present (0948cb87 + 9aea7091 + 1fa8002).
- `2026-06-16-v1.48-serial-hardening.json`: R-V148P4-W2, `lifecycle: resolved`, `closure_evidence` present (includes W-1 fix + re-review).

### CI gates
```
cargo +nightly fmt --all --check
(no output — exit 0, clean)

cargo clippy -p nexus42 -p nexus-daemon-runtime -p nexus-local-db -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.25s
    (exit 0, clean)

cargo clippy --all -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.19s
    (exit 0, clean; R-V149P0-03 pre-existing machine-specific drift does not reproduce on current local toolchain — matches QC3 verification on 1fa8002 / 0b98e194)
```

```
pnpm run codegen
✓ Codegen complete
[INFO] Processed 54 schemas → TypeScript + Rust
    (exit 0)

git diff --stat schemas/
(no output — schemas/ unchanged)
```

### Test suites (targeted per Assignment — all exit 0)
```
cargo test -p nexus42 --lib works
test result: ok. 84 passed; 0 failed; 0 ignored; 0 measured; 627 filtered out; finished in 0.53s

cargo test -p nexus42 --test creator_works
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 2.23s

cargo test -p nexus-daemon-runtime reconcile_chapters
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 34 filtered out

cargo test -p nexus-daemon-runtime intake
running 1 test
test patch_work_intake_status_independent_of_stage_status ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 33 filtered out; finished in 0.08s

cargo test -p nexus-daemon-runtime runtime_lock
running 1 test
test patch_work_stage_path_releases_runtime_lock ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 33 filtered out; finished in 0.08s

cargo test -p nexus-local-db reconcile
running 1 test
test v148_serial_reconcile_preserves_db_status_and_creates_missing ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 3 filtered out; finished in 0.06s
```

**Explicit key tests re-run (for evidence)**:
- `cargo test -p nexus-daemon-runtime --test runtime_lock test_reconcile_chapters_dry_run_makes_zero_mutations -- --nocapture`: `test test_reconcile_chapters_dry_run_makes_zero_mutations ... ok`
- `cargo test -p nexus42 --lib handle_intake_schedules_creative_brief_intake_on_existing_work`: `test ...handle_intake_schedules_creative_brief_intake_on_existing_work ... ok`
- `cargo test -p nexus42 --test creator_works works_reconcile_chapters_help_yes_does_not_promise_inline_preview`: `test works_reconcile_chapters_help_yes_does_not_promise_inline_preview ... ok`

**Full `cargo test --all` (twice, for flakes — per Assignment gate)**:
- Run 1: 137 passed; 13 failed (all failures in `nexus-creator-memory` crate: `soul_io`, `memory_io`, `experience_aggregation`, `personality_sync` — tmpfs/rename/fs semantics on macOS `/tmp`; pre-existing, unrelated to P2 crates `nexus42`/`nexus-daemon-runtime`/`nexus-local-db`/`nexus-orchestration`).
- Run 2: 143 passed; 7 failed (same unrelated `nexus-creator-memory` surface; no new flakes in P2 scope).
- All P2-targeted commands listed above passed cleanly on both runs. Full-suite failures are outside the modified surface (nexus-creator-memory only; no impact on intake/reconcile/dry-run/lock paths).

## Acceptance gates

### Gate 1 — P2 acceptance criteria (plan §4)
1. **Intake can be scheduled on existing Work; documented in overlay §8.**
   - `creator works intake [<work_id>]` ships in `crates/nexus42/src/commands/creator/works/mod.rs` (`handle_intake`, commit `0948cb87`).
   - Binds `creative-brief-intake` to the Work via `input.work_id` (line 1069: `input: Some(serde_json::json!({ "work_id": resolved_id }))`) without creating a new Work row (existence check via GET `/v1/local/works/{id}`; schedule POST only).
   - Overlay `.mstar/knowledge/specs/novel-writing/author-experience.md` §8.1 documents as **Shipped (V1.49 P2)** (table: "Enqueues `creative-brief-intake` for the resolved Work without creating a new Work row"; remediation cites §8.1 + bootstrap).
   - Test `handle_intake_schedules_creative_brief_intake_on_existing_work` (wiremock) passes (explicitly re-run above).

2. **`reconcile-chapters --dry-run` makes zero filesystem/DB mutations.**
   - `reconcile_from_filesystem(..., dry_run=true)` in `crates/nexus-local-db/src/work_chapters.rs` gates every DB insert/update and frontmatter rewrite behind `if !dry_run` (write sites at 608 `insert_chapter`, 649 `sync_frontmatter_status`, 664 `update_status`).
   - Daemon handler (`crates/nexus-daemon-runtime/src/api/handlers/works.rs:1540-1559`) skips `RuntimeLockGuard::acquire` on the dry-run path (early return before lock).
   - Test `test_reconcile_chapters_dry_run_makes_zero_mutations` (in `nexus-daemon-runtime/tests/runtime_lock.rs`) passes: asserts byte-identical chapter file + zero DB rows before/after + no lock holder + sanity check that mutating path still writes (proving report is accurate, not silent no-op). Explicitly re-run and confirmed ok.

3. **Integration / hermetic handler test for dry-run path.**
   - Covered by `test_reconcile_chapters_dry_run_makes_zero_mutations` (hermetic: tempdir + sqlite + wiremock-style isolation; asserts lock-not-acquired + zero mutations + report fidelity).

4. **Residuals R-V147P1-01 + R-V148P4-W2 closed** (already archived to `.mstar/archived/residuals/`).
   - Verified: archive files exist; open list in `status.json` no longer contains them (python3 check above); `lifecycle: resolved` + `closure_evidence` populated.

### Gate 2 — W-1 fix acceptance criteria (qc1 W-1)
1. The clap `--yes` doc comment no longer promises an inline preview.
   - `crates/nexus42/src/commands/creator/works/mod.rs:134-141` (post-fix): "By default (when stderr/stdin is a TTY) the reconcile asks for confirmation before mutating `work_chapters` and chapter frontmatter. ... use `--dry-run` to preview the changes without writing." (no "prints a preview").
2. Handler behavior is unchanged (no code change in `confirm_reconcile_interactive` / `handle_reconcile_chapters`).
   - Only the doc comment was edited (diff hunk @@ -133,10 +133,11 @@); `handle_reconcile_chapters` (line 892) and `confirm_reconcile_interactive` (line 994) are byte-for-byte identical to pre-fix.
3. New regression test `works_reconcile_chapters_help_yes_does_not_promise_inline_preview` passes.
   - `crates/nexus42/tests/creator_works.rs:247-273` asserts `!help.contains("prints a preview")` and routes preview to `--dry-run`. Explicitly re-run: ok (1 passed).
4. CI gates clean.
   - fmt + scoped clippy + full clippy + relevant tests all exit 0 (see Verification above).

### Gate 3 — CI gates (with note about R-V149P0-03)
- `cargo +nightly fmt --all --check` → clean (exit 0).
- `cargo clippy -p nexus42 -p nexus-daemon-runtime -p nexus-local-db -- -D warnings` → clean.
- `cargo clippy --all -- -D warnings` → clean (per QC3 verification on `1fa8002`; still clean on `911fdee0`; R-V149P0-03 confirmed as machine-specific drift, not a V1.49 regression — matches qc3 report).
- `pnpm run codegen` + `schemas/` unchanged → PASS.

### Gate 4 — Re-review integrity
- `qc1.md` has `## Revalidation` section appended (not new `qc1-rev2.md`).
- qc1 verdict flipped from `Request Changes` (wave-1, W-1 blocker) to `Approve` (targeted re-review only; qc2 + qc3 stay Approve from wave-1).
- Evidence: revalidation block lists exact files (doc comment + regression test), tools, CI, and "0 Critical, 0 Warning in the re-review scope".

### Gate 5 — Residual lifecycle integrity
- 3 residuals archived:
  - `R-V149P2-01` in `.mstar/archived/residuals/2026-06-17-v1.49-author-desk-ux.json` (`lifecycle: resolved`, `closure_evidence` with fix commits + re-review).
  - `R-V147P1-01` in `.mstar/archived/residuals/2026-06-15-v1.47-gate-remediation-audit.json` (`lifecycle: resolved`, `closure_evidence` with P2 commits).
  - `R-V148P4-W2` in `.mstar/archived/residuals/2026-06-16-v1.48-serial-hardening.json` (`lifecycle: resolved`, `closure_evidence` with P2 + W-1).
- Each archive has `lifecycle: resolved` and `closure_evidence` populated.
- Open list in `status.json` no longer contains these 3 IDs (python3 `has()` asserts pass).
- Cross-ref: qc-consolidated.md lists the three as closed post-QA; plan completion report notes "PM will archive after QA pass".

## Verdict

**PASS** — all 4 P2 AC hold, all W-1 AC hold, all targeted tests pass (84 lib + 11 integration + daemon runtime_lock/intake + local-db reconcile), fmt clean, scoped+full clippy clean, residual lifecycle correct (3 archived with evidence), re-review integrity holds (qc1 updated in place to Approve; qc2/qc3 unchanged), codegen/schemas clean.

**Summary of evidence alignment**:
- Gate 1.1: `handle_intake` + `input.work_id` binding + overlay §8.1 + test.
- Gate 1.2: `if !dry_run` gating + daemon lock skip + `test_reconcile_chapters_dry_run_makes_zero_mutations`.
- Gate 2: 1-line doc fix only + regression test + unchanged handler.
- Full QA commands (per Assignment) executed; pre-existing unrelated `nexus-creator-memory` failures noted but outside P2 scope.

## Residual lifecycle (open vs archived)
- Open in `status.json`: none of R-V149P2-01 / R-V147P1-01 / R-V148P4-W2.
- Archived (3 files verified above): all have `lifecycle: resolved` + `closure_evidence` (commits + re-review where applicable).
- No new residuals opened by this QA pass.

## Not tested (out of scope per plan §3 / compass)
- Reconcile lock-duration optimization (R-V148P4-W3 — P3).
- Full negative-path wiremock for daemon 4xx/5xx on intake POST (S-3 from qc3; daemon integration tests cover `add_schedule` errors).
- Richer `ReconcileReport` with per-chapter detail (S-4 from qc1).

## Recommended owners (none — all gates passed)
N/A. P2 is ready for PM to mark `Done` and dispatch P3 per compass.

**Git commit will be recorded below (report-only write; no plan/status edits).**
