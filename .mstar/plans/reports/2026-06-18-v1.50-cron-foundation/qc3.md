---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: "2026-06-18-v1.50-cron-foundation"
working_branch: "feature/v1.50-cron-foundation"
review_cwd: "/Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v150-cron-foundation"
review_range: "merge-base c38fbe1f264b9574b25355d872d20138c1c04e77..a7ea8349260fa7c8cc5be0f586fa9f84d13549ee"
verdict: "Request Changes"
generated_at: "2026-06-18"
---

# Code Review Report — V1.50 T-A P0 cron-foundation (Reviewer #3)

## Reviewer Metadata

- Reviewer: @qc-specialist-3
- Runtime Agent ID: `qc-specialist-3`
- Runtime Model: `MiniMax-M3`
- Review Perspective: **Performance + Reliability** (assigned by PM)
- Report Timestamp: 2026-06-18

## Scope

- plan_id: `2026-06-18-v1.50-cron-foundation`
- Review range / Diff basis: `merge-base c38fbe1f264b9574b25355d872d20138c1c04e77..a7ea8349260fa7c8cc5be0f586fa9f84d13549ee`
- Working branch (verified): `feature/v1.50-cron-foundation`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v150-cron-foundation`
- Files reviewed: 10 (1711 insertions, 8 deletions)
- Commit range (5 commits, identical to Review range):
  - `cdceac31` — feat(nexus-local-db): works.schedule_json column + cron DAO (T1)
  - `a1a67217` — feat(nexus42): creator works cron set/show/list CLI (T2-T5)
  - `867c4c88` — test(nexus42): cron_cli + migration hermetic tests (T6)
  - `11295a34` — style(nexus42): clippy + nightly fmt hygiene
  - `a7ea8349` — harness(v1.50-cron): mark T1-T7 done + Completion Report v2 (T7)
- Tools run:
  - `git rev-parse --show-toplevel` / `git branch --show-current` / `git log -5` (context gate)
  - `git diff --stat` (1711-line surface)
  - `cargo +nightly fmt --all --check` — exit 0, no diff
  - `cargo clippy -p nexus-local-db -p nexus42 -- -D warnings` — clean
  - `cargo test -p nexus-local-db --test works_schedule_migration` — 7/7 passed
  - `cargo test -p nexus42 --test cron_cli` — 8/8 passed
  - `cargo test -p nexus42 --lib commands::creator::works::cron` — 21/21 passed
  - `cargo clippy -p nexus42 --tests -- -D warnings` — **pre-existing failures** in untouched code (see Notes)

## Findings

### 🔴 Critical

*(none)*

### 🟡 Warning

#### W-001 — Spec §3.1 "all-off" rule not enforced (correctness/safety)

- **File / Location**: `crates/nexus42/src/commands/creator/works/cron.rs` :: `apply_set_args` (lines 208–260)
- **Spec reference**: `.mstar/knowledge/specs/novel-writing/cron-staggering.md` §3.1, last line of "Validation" table:
  > "At least one role must remain `enabled: true` unless `--all-off` is passed (CLI rejects empty schedules)."
- **Issue**: The implementation validates each cron/TZ value (AC #5) but does **not** enforce the "at least one role enabled" rule. A user can run:
  ```bash
  nexus42 creator works cron set my-work --no-brainstorm --no-write --no-review
  ```
  and the resulting `schedule_json` will have `enabled: false` for all three roles. The CLI persists the (now empty-from-a-firing-perspective) schedule. The daemon's cron evaluator in T-A P1 will then skip all fires indefinitely for that Work (per spec §4.1/§4.3), and the user has no recovery in the CLI except re-running `set` with role flags.
- **Impact**: Silent data-state that violates the spec; defeats the purpose of "staggering" if a user accidentally disables every role. The spec explicitly calls this out as a CLI rejection.
- **Fix**: After applying the args, count the enabled roles; if zero and `--all-off` was not passed, return a `CliError::Config` with a stable code (e.g. `E_CRON_ALL_ROLES_DISABLED`). Add `--all-off` flag in `CronCommand::Set` clap definition. Cover with a hermetic unit test (`apply_set_args_all_off_without_flag_rejects`).
- **Confidence**: High (spec text is normative; the rule was simply missed).

#### W-002 — Spec §3.2 `show` output missing "Next fire (UTC)" / "Local time" columns (spec completeness)

- **File / Location**: `crates/nexus42/src/commands/creator/works/cron.rs` :: `render_show` (lines 271–299)
- **Spec reference**: `.mstar/knowledge/specs/novel-writing/cron-staggering.md` §3.2 example output:
  ```text
  Role        Cron                Local time           Next fire (UTC)
  brainstorm  0 3,9,15,21 * * *   03:00 / 09:00 / ...   2026-06-19 19:00 UTC
  ```
- **Issue**: The reviewer prompt explicitly flagged this as a concern: *"CLI render: `cron show` next-fire UTC computation does not iterate per-second for years."* The implementation renders only the cron expression + TZ offset and a 3-row `Role / Cron / Enabled` table. The "Local time" and "Next fire (UTC)" columns from the spec are absent. The plan's AC #3 phrases this as "show with TZ display", which is the minimum; the spec is broader.
- **Impact**: Authors cannot see when the next fire will land. The whole point of the per-Work cron config layer is "let authors set the work on autopilot" (spec §1) — and `show` is the primary verification surface. Without next-fire, the user has to compute the time mentally.
- **Fix**:
  - "Local time" column = transform the cron expression into a list of local-time HH:MM strings (parse the hour/minute fields; render in TZ; cap at e.g. 3 entries with `…`).
  - "Next fire (UTC)" column = call `cron::Schedule::from_str(&normalize_cron_fields(expr)).upcoming(Utc).next()` once, render `2026-06-19 19:00 UTC` (or `disabled` when `!enabled`).
  - **Performance guard**: The `upcoming(Utc).next()` call is O(1) per role (the `cron` crate precomputes the field ranges) — it does **not** iterate per-second. The reviewer prompt's worst-case ("does not iterate per-second for years") is satisfiable by simply calling `.next()` once.
- **Confidence**: High. The spec example is explicit; the computation is a single `.next()` per role and the T-A P1 cron evaluator uses the same primitive (spec §4.1).

#### W-003 — `list_works_schedule` is unbounded — workspace-scale performance risk

- **File / Location**: `crates/nexus-local-db/src/works.rs` :: `list_works_schedule` (lines 1425–1450)
- **Spec reference**: Spec §3.3 (no explicit limit; common-sense CLI ergonomics).
- **Issue**: `list_works_schedule` runs `SELECT work_ref, work_id, schedule_json FROM works WHERE creator_id = ? AND workspace_slug = ? ORDER BY updated_at DESC` with **no `LIMIT` clause**. For a workspace with N Works, this is O(N) read. The existing `list_works` API has `WorkListFilters::limit/offset` (default 100) for pagination. The new `cron list` surface has no such bound.
  - For N ≤ ~500 Works (typical single-author use) this is fine.
  - For N ≥ ~5,000 (multi-author workspace, or long-tail creator), the `render_list` output becomes a wall of truncated text (24-char `WORK_REF` column is too narrow for many IDs), and the SELECT + transfer scales linearly.
  - The reviewer prompt's question: *"`cron list` over a workspace with many Works — bounded?"* → **No, it is not bounded today.**
- **Impact**: Latency on `cron list` grows linearly with Works; CLI output becomes unreadable past ~100 Works.
- **Fix**: Accept an optional `limit` parameter on `list_works_schedule` (default 100, matching `list_works`). Pass through `handle_list` via a `--limit <N>` flag. Update the unit test to assert the cap.
- **Confidence**: High. The DAO function is private to this plan; tightening the contract is surgical and consistent with the existing `list_works` precedent.

#### W-004 — TOCTOU between read (`get_schedule_json`) and write (`set_schedule_json`) in `handle_set`

- **File / Location**: `crates/nexus42/src/commands/creator/works/cron.rs` :: `handle_set` (lines 494–532)
- **Issue**: `handle_set` does a two-step:
  1. `get_schedule_json(pool, &work_id)` → base
  2. `apply_set_args(base, ...)` → new schedule
  3. `set_schedule_json(pool, &work_id, blob, now)` → write
  This is a classic read-modify-write without a transaction. Two concurrent CLI invocations against the same Work (e.g. two terminals, or a CLI tool + a future daemon) can race: both read the same base, both validate, both write — last-writer-wins, silently losing the other side's changes.
- **Impact**: Microsecond race window; in single-user CLI use the chance is near zero. The risk grows when T-A P1 introduces a daemon-side cron writer (spec §4) — that writer will be the racing party. The DAO contract is silent on concurrency; if the P1 daemon trusts "I read base, I write new", it can drop schedules.
- **Fix (P0 scope, surgical)**: Add a `#[allow(dead_code)]` doc comment on `handle_set` explicitly noting the race, and defer the transactional `set_schedule_json_tx` to T-A P1 when the daemon writer lands. Alternative: add a `SET schedule_json = ?, updated_at = ? WHERE work_id = ? AND schedule_json = ?` CAS variant on the DAO; `handle_set` retries on mismatch.
- **Confidence**: Medium. The race is real but small at P0. The fix is a doc comment or a deferred P1 task. I will not block merge solely on this, but the durable roadmap should call it out.

### 🟢 Suggestion

#### S-001 — Add partial index on `works(schedule_json) WHERE schedule_json IS NOT NULL` for T-A P1 evaluator

- **File / Location**: `crates/nexus-local-db/migrations/202606180001_works_schedule_json.sql`
- **Issue**: T-A P1 (spec §4.1) will run *"Reads all Works with `schedule_json` non-empty OR `auto_chronology=true`"* on every daemon tick (1-min interval). Without a partial index, this is a full table scan of `works` per tick. Adding a partial index is essentially free at P0 (the column is brand-new) and saves the P1 work.
  ```sql
  CREATE INDEX IF NOT EXISTS works_schedule_json_nn
    ON works(schedule_json) WHERE schedule_json IS NOT NULL AND schedule_json != '';
  ```
- **Impact**: Cheap insurance for the T-A P1 cron evaluator. Negligible insert cost.
- **Confidence**: High (well-known SQLite pattern).
- **Tracking**: durable roadmap → T-A P1 (`feature/v1.50-cron-brainstorm-write`). May also be added now.

#### S-002 — No tracing / structured logging on the cron hot path

- **File / Location**: `cron.rs` :: `handle_set` / `handle_show` / `handle_list` (lines 494–590)
- **Issue**: The success path prints to stdout only. There is no `tracing::info!` on a successful `set` (e.g. `info!(work_id, blob_size, "cron schedule set")`), no `tracing::debug!` on `show`/`list` (e.g. `debug!(work_id, "resolved effective schedule")`). The spec §4.2/§4.3 mention `INFO` and `DEBUG` log levels for the daemon evaluator, but the P0 foundation has no observability layer to grow into.
- **Impact**: When T-A P1 lands, the daemon will need `tracing` for the fire paths — better to introduce the convention at P0.
- **Fix**: Add `tracing` to the workspace deps; emit `info!` on `set` success and `debug!` on `show`/`list` reads. Mirror the precedent from `nexus-daemon-runtime`.
- **Confidence**: Medium. Consistent with the rest of the codebase's `tracing` adoption trajectory.

#### S-003 — `NEXUS_TZ` env fallback is undocumented in `creator works cron set --help`

- **File / Location**: `crates/nexus42/src/commands/creator/works/cron.rs` :: `CronCommand::Set` clap definition (lines 391–418); `handle_set` (lines 508–518)
- **Issue**: The implementation reads `NEXUS_TZ` env var as a fallback for `--tz` (spec §3.1 last flag). The clap doc-comment for `--tz` says `("IANA timezone (default: NEXUS_TZ env, fallback UTC)")` — actually it does mention it (good!). However, the env var name is **not** mentioned in the spec file or in any other public docs. Authors discovering this fallback by reading the help text is the only path.
- **Fix**: Add a one-line mention in spec §3.1 or in `docs/` (e.g. `cli-cron-quickstart.md`). Not blocking; cosmetic.
- **Confidence**: Low (docs-only).

#### S-004 — Conflicting flags (`--brainstorm X --no-brainstorm`) silently last-write-wins

- **File / Location**: `cron.rs` :: `apply_set_args` (lines 233–253)
- **Issue**: If a user passes both `--brainstorm "0 9 * * *"` and `--no-brainstorm`, validation succeeds and the result has `cron="0 9 * * *"` but `enabled=false`. The user may not realize `enabled` is false when reading the cron expression. The order in which clap resolves positional/flag args is not always intuitive.
- **Fix**: Detect the conflict and either (a) reject with `E_CRON_CONFLICTING_FLAGS`, or (b) print a `WARN` to stderr. (a) is cleaner; (b) is non-breaking.
- **Confidence**: Low. Edge case.

## Source Trace (selected)

| Finding | Source Type | Source Reference | Confidence |
| --- | --- | --- | --- |
| W-001 | doc-rule | `.mstar/knowledge/specs/novel-writing/cron-staggering.md` §3.1 Validation table | High |
| W-002 | doc-rule | `.mstar/knowledge/specs/novel-writing/cron-staggering.md` §3.2 example output | High |
| W-003 | manual-reasoning | `list_works_schedule` SQL at `works.rs:1432` (no `LIMIT`) | High |
| W-004 | manual-reasoning | `handle_set` at `cron.rs:494-532` (read-then-write, no tx) | Medium |
| S-001 | doc-rule | spec §4.1 (1-min tick scans Works) | High |
| S-002 | consistency | `tracing` precedent elsewhere in repo | Medium |

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 4 |
| 🟢 Suggestion | 4 |

**Verdict**: **Request Changes**

**Rationale**:
- W-001 is a clear normative spec deviation (§3.1 "all-off" rule) — the spec explicitly requires the CLI to reject an all-disabled schedule; the implementation does not. This is a 1-line validation + 1 test fix.
- W-002 is a clear spec completeness gap (§3.2 next-fire column) — the reviewer prompt explicitly flagged this as a concern. The fix is bounded (single `.next()` per role, no per-second iteration).
- W-003 is a clear performance concern for the P0 surface (unbounded `list_works_schedule`) — the spec doesn't bound it explicitly, but the existing `list_works` precedent does. Adding `--limit` is a 5-line contract tightening.
- W-004 is a minor reliability concern that is acceptable at P0 (single-user CLI, microsecond race window) but should be tracked as a durable roadmap item for T-A P1.

The four Suggestions are not blocking and may be tracked as residual findings (S-001 in the durable roadmap → T-A P1; S-002/S-003/S-004 as low-priority follow-ups).

## Notes

### Pre-existing clippy failures (NOT introduced by this plan)

Running `cargo clippy -p nexus42 --tests -- -D warnings` exposes 27 errors, but **none of them are in the files or hunks touched by this plan**. I verified the diff for the two highest-signal errors:

- `crates/nexus42/src/commands/creator/works/mod.rs:3132` — `let _ = result;` (a `let_underscore_future` warning). The diff at this file shows only the addition of `pub mod cron;` (line 24), the new `Cron` enum variant (lines 185–191), and the dispatch arm (line 428). Line 3132 is in pre-existing test code for the `Start` / `Create` rejected subcommands.
- `crates/nexus42/src/commands/system/mod.rs:384` — `match_same_arms` warning. The file is not touched by the diff at all.

The plan-level clippy gate used in the plan's verification section is `cargo clippy -p nexus-local-db -p nexus42 -- -D warnings` (no `--tests` flag), which is clean. I confirmed the gate is the right scope (lib + bins, not tests). **No new clippy violations are introduced by T-A P0.**

### V1.49 R-V149P1-02 tracing-registry flake — N/A

The new hermetic tests (`tests/cron_cli.rs`, `tests/works_schedule_migration.rs`, the unit tests in `cron.rs`/`works.rs`) do not initialize any `tracing` subscriber; they exercise the DAO + pure functions + `assert_cmd` `--help` text only. The flake is not reproducible on this plan's surface.

### Migration runtime cost — confirmed safe

`ALTER TABLE works ADD COLUMN schedule_json TEXT` (no DEFAULT, nullable) is a schema-only operation in SQLite (no row rewrite). It runs in O(1) regardless of `works` row count. The migration file is well-commented (spec linkage, NULL = defaults semantics, INSERT column-list invariant in `works.rs`). No backfill is required.

### DAO atomicity — confirmed

`set_schedule_json` is a single `UPDATE` statement; SQLite guarantees atomicity (the column is updated in full or not at all). The `to_json_string` call in `handle_set` happens before the SQL is sent; if serialization fails, no SQL runs. There is no "partial write" path.

### Verifier evidence

```text
$ cargo +nightly fmt --all --check
# exit 0

$ cargo clippy -p nexus-local-db -p nexus42 -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.22s

$ cargo test -p nexus-local-db --test works_schedule_migration
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured

$ cargo test -p nexus42 --test cron_cli
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured

$ cargo test -p nexus42 --lib commands::creator::works::cron
test result: ok. 21 passed; 0 failed; 0 ignored; 0 measured
```

36/36 tests pass. Clippy and nightly-fmt both clean for the plan's surface.

## Residual Findings (for PM to register in `status.json` after fixes)

Pending PM assignment — these are the items that will remain open after W-001 / W-002 / W-003 are fixed:

| ID | Title | Severity | Source | Decision | Owner | Tracking |
| --- | --- | --- | --- | --- | --- | --- |
| R-V150P0-S1 | T-A P1 partial index on `works(schedule_json) WHERE schedule_json IS NOT NULL` (S-001) | low (suggestion, deferred) | qc3 S-001 | defer → T-A P1 | `@fullstack-dev` (T-A P1) | `.mstar/iterations/v1.50-…-compass-v1.md` |
| R-V150P0-S2 | `tracing` adoption on the cron hot path (S-002) | low (suggestion, deferred) | qc3 S-002 | defer → T-A P1 (alongside evaluator) | `@fullstack-dev` (T-A P1) | durable roadmap |
| R-V150P0-S3 | `NEXUS_TZ` env var documentation in `docs/` (S-003) | low (suggestion, deferred) | qc3 S-003 | defer → docs sweep | `@product-manager` or `@writing-specialist` | durable roadmap |
| R-V150P0-S4 | `set` flag conflict detection (`--brainstorm X --no-brainstorm`) (S-004) | low (suggestion) | qc3 S-004 | defer → UX pass | `@fullstack-dev` | durable roadmap |
| R-V150P0-W1 | `handle_set` read-modify-write TOCTOU window (W-004) | medium (warning, deferred) | qc3 W-004 | defer → T-A P1 (daemon writer race) | `@fullstack-dev` (T-A P1) | durable roadmap |

(PM may collapse R-V150P0-S1 / R-V150P0-S2 / R-V150P0-W1 into a single T-A P1 plan-level residual if the T-A P1 plan owns them all.)

## Files inspected

- `crates/nexus-local-db/migrations/202606180001_works_schedule_json.sql` (29 lines, full read)
- `crates/nexus-local-db/src/works.rs` (lines 1–1974, focus on 1306–1451 cron DAO + tests; full read)
- `crates/nexus42/src/commands/creator/works/cron.rs` (800 lines, full read)
- `crates/nexus42/src/commands/creator/works/mod.rs` (focus on lines 1–440 dispatch + Cron variant; diff inspected)
- `crates/nexus42/tests/cron_cli.rs` (306 lines, full read)
- `crates/nexus-local-db/tests/works_schedule_migration.rs` (259 lines, full read)
- `Cargo.toml` and `crates/nexus42/Cargo.toml` (diff inspected)
- `.mstar/knowledge/specs/novel-writing/cron-staggering.md` (lines 1–200, full read)
- `.mstar/plans/2026-06-18-v1.50-cron-foundation.md` (full read)
