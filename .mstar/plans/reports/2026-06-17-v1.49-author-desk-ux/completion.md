# Completion Report v2 — P2 (author-desk-ux)

- **plan_id**: `2026-06-17-v1.49-author-desk-ux`
- **owner**: `@fullstack-dev`
- **Working branch used**: `feature/v1.49-author-desk-ux`
- **Worktree path**: `.worktrees/v1.49-author-desk-ux`
- **Base**: `iteration/v1.49` @ `c993ad15`
- **Commits**:
  - `4ab9e0be` feat(v1.49-p2): reconcile-chapters --dry-run (R-V148P4-W2)
  - `0948cb87` feat(v1.49-p2): intake re-trigger + reconcile CLI flags (T1+T2)
  - `9aea7091` docs(v1.49-p2): overlay §8 shipped CLI + intake/reconcile surface tests (T3)
  - (plan status → InReview + task checkboxes updated, committed with this report)

## Cargo verification (last lines of each command)

```
### cargo +nightly fmt --all --check
(exit 0 — empty output = clean)

### cargo clippy -p nexus42 -p nexus-daemon-runtime -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.27s

### cargo test -p nexus42 works (lib)
test result: ok. 67 passed; 0 failed; 0 ignored; 0 measured; 644 filtered out

### cargo test -p nexus-daemon-runtime reconcile_chapters  (runtime_lock.rs file)
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

### cargo test -p nexus-daemon-runtime intake
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 33 filtered out

### cargo test -p nexus-daemon-runtime runtime_lock
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

Additionally `cargo test -p nexus-local-db reconcile` (lib + integration) passes
(4 lib + 1 `v148_serial_hardening` integration), confirming the
`reconcile_from_filesystem` signature change is consistent across all 8 call
sites.

## Acceptance criteria

1. **Intake can be scheduled on existing Work; documented in overlay §8.** —
   `creator works intake [<work_id>]` ships in `works/mod.rs` (`handle_intake`,
   commit `0948cb87`); binds `creative-brief-intake` to the Work via
   `input.work_id` without a new Work row. The preset declares no gates, so the
   existing schedule-add handler accepts it on any existing Work. Documented in
   `author-experience.md` §8.1 (commit `9aea7091`). Verified by
   `handle_intake_schedules_creative_brief_intake_on_existing_work` (wiremock).
2. **`reconcile-chapters --dry-run` makes zero filesystem/DB mutations.** —
   `reconcile_from_filesystem(..., dry_run=true)` gates every DB insert/update
   and frontmatter rewrite behind `if !dry_run` (commit `4ab9e0be`); the daemon
   handler skips `RuntimeLockGuard::acquire` on the dry-run path. Verified by
   `test_reconcile_chapters_dry_run_makes_zero_mutations` which asserts
   byte-identical chapter file, zero DB rows before/after, and no lock holder —
   then confirms the mutating path still writes (proving the report is accurate,
   not a silent no-op).
3. **Integration / hermetic handler test for dry-run path.** —
   `test_reconcile_chapters_dry_run_makes_zero_mutations` in
   `nexus-daemon-runtime/tests/runtime_lock.rs` (commit `4ab9e0be`).
4. **Residuals R-V147P1-01, R-V148P4-W2 evidence collected (closure by PM).** —
   see "Residual closure" below.

## Residual closure

- **R-V147P1-01** (intake re-trigger on existing Work): closable. Evidence =
  new `Intake` subcommand + `handle_intake_schedules_creative_brief_intake_on_existing_work`
  wiremock test (commit `0948cb87`) + overlay §8.1 update (commit `9aea7091`).
- **R-V148P4-W2** (reconcile preview): closable. Evidence =
  `reconcile_from_filesystem` dry_run + daemon `?dry_run=true` query path +
  `test_reconcile_chapters_dry_run_makes_zero_mutations` (commit `4ab9e0be`) +
  `--dry-run`/`--yes` CLI flags (commit `0948cb87`) + overlay §8.2 update
  (commit `9aea7091`).

Per assignment hard rule, the implementer did **not** edit `status.json`; both
rows remain open in root `residual_findings` under their original plan_ids
(`2026-06-15-v1.47-gate-remediation-audit` and `2026-06-16-v1.48-serial-hardening`).
PM is to archive them to `.mstar/archived/residuals/<plan-id>.json` after the
QA pass.

## Implementation notes / decisions

- **Daemon schedule-add needs no code change for T1.** The `creative-brief-intake`
  preset declares no `gates`, so the existing `add_schedule` handler accepts it
  on any existing Work bound via `input.work_id` (work_id is resolved from
  `input` first, then seed). T1 is therefore CLI-only; the contract is verified
  at the integration boundary by the wiremock test rather than by a new
  daemon-runtime test (the daemon schedule-add path is already covered by
  `fl_e_schedule_api.rs` and is unchanged here).
- **`reconcile_from_filesystem` signature grew a `dry_run: bool`** rather than
  spawning a parallel preview function. All 8 existing call sites (1 daemon
  handler + 6 unit tests in `work_chapters.rs` + 1 in `v148_serial_hardening.rs`)
  pass `false`; counters stay accurate on the dry-run path because the write
  calls are gated while the `+= 1` increments are not.
- **`--yes`/default prompt policy mirrors `works rules reset`**: default prompts
  on TTY stdin, errors on non-TTY without `--yes`; `--dry-run` takes precedence
  over `--yes`. Chosen policy documented in the `handle_reconcile_chapters`
  docstring and overlay §8.2.
- **Driver interaction (§8.1):** intake re-trigger enqueues an independent
  schedule and does not PATCH `driver_schedule_id`, so it does not cancel an
  active FL-E auto-chain driver.

## Risks / follow-ups

- none within this plan's scope. (Reconcile lock-duration optimization remains
  out of scope — P3 / R-V148P4-W3 per plan §3.)

## Handoff

- Branch `feature/v1.49-author-desk-ux` is committed and the worktree is left
  intact for QC to inspect. PM to merge into `iteration/v1.49` and archive the
  two residuals post-QA.

**Ready for QC tri-review**: yes
