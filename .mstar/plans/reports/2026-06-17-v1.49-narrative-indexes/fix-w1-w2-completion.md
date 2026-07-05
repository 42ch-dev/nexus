## Completion Report v2 — P1 fix wave (W-1 + W-2)

- plan_id: 2026-06-17-v1.49-narrative-indexes
- owner: @fullstack-dev
- Working branch used: fix/v1.49-p1-w1-w2-typed-and-allocation
- Worktree path: .worktrees/v1.49-p1-w1-w2-fix
- Base: iteration/v1.49 @ eb75a73d
- Commits:
  - `3f2efc03` fix(orchestration/v1.49-p1): W-1 typed ForeshadowingStatus + W-2 explicit F### token

## Cargo verification (last 5 lines of each)

```
$ cargo +nightly fmt --all --check
(no output — clean)  EXIT=0

$ cargo clippy -p nexus-orchestration -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.15s

$ cargo test -p nexus-orchestration --lib narrative_index
test narrative_index::tests::promote_outline_to_index_does_not_allocate_for_prose_bullets ... ok
test narrative_index::tests::promote_outline_to_index_noop_section_does_not_touch_mtime ... ok

test result: ok. 31 passed; 0 failed; 0 ignored; 0 measured; 603 filtered out; finished in 0.02s

$ cargo test -p nexus-orchestration --test novel_project_init
test t7g_db_failure_rolls_back_filesystem_scaffold ... ok
test t7f_partial_reinit_only_updates_listed_fields ... ok

test result: ok. 22 passed; 0 failed; 0 ignored; 0 measured; finished in 1.18s

$ cargo test -p nexus-orchestration --test e2e_novel_writing
test e2e_chapter_scoped_pipeline_executes ... ok
test e2e_schedule_advance_past_outlining ... ok

test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s

$ cargo test -p nexus-orchestration --test sync_module_works_layout
test test_discover_works_multiple_entries ... ok
test test_discover_works_excludes_readme_outlines_logs ... ok

test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; finished in 0.01s
```

Additional gate (CI-equivalent): `cargo clippy --all -- -D warnings` clean (27.82s); `cargo test -p
nexus-orchestration --lib stage_gates` → 52 passed (includes the 3 `build_preset_input` foreshadowing
summary tests that construct `ForeshadowingRow` with the new enum).

## Acceptance criteria

1. **`ForeshadowingRow.status` is a typed enum** — changed from `String` to `ForeshadowingStatus`
   (`Planned | Buried | PaidOff`). Evidence: `3f2efc03`; tests `foreshadowing_status_display_is_canonical_lowercase`,
   `foreshadowing_status_fromstr_is_case_insensitive`.
2. **`parse_foreshadowing_index` validates status via FromStr, returns structured error** — signature
   changed to `Result<Vec<ForeshadowingRow>, IndexParseError>`; `IndexParseError::InvalidStatus { row_index, value }`.
   Evidence: `3f2efc03`; test `parse_foreshadowing_index_rejects_unknown_status` (asserts row_index=1, value="Payed off").
3. **`serialize_foreshadowing_index` uses Display for canonical output** — `ForeshadowingStatus` impl
   `Display` emits canonical lowercase + underscore; the `{}` format in serialize invokes Display.
   Evidence: `3f2efc03`; test `serialize_then_parse_roundtrip_preserves_known_statuses`.
4. **`extract_inline_f_declarations` requires the F### token** — removed the allocation branch that
   fired on any bullet; only F###-tokened bullets yield declarations. Evidence: `3f2efc03`; test
   `extract_inline_f_declarations_ignores_bullets_without_f_token`.
5. **`promote_outline_to_index` does not allocate for prose bullets** — removed the `None` allocation
   path; defensive guard for `None` via `let Some(id) = decl.id else { continue; }`. Evidence:
   `3f2efc03`; test `promote_outline_to_index_does_not_allocate_for_prose_bullets`.
6. **All new tests pass** — 8 new tests (5 W-1 + 3 W-2), all green in the 31-test narrative_index
   suite. Evidence: `3f2efc03`; lib test result `31 passed; 0 failed`.
7. **All existing tests pass** — 31 narrative_index lib (was 25, 2 removed + 8 added) + 52 stage_gates
   lib + 22 novel_project_init + 11 e2e_novel_writing + 9 sync_module_works_layout. Evidence: cargo
   test outputs above.
8. **CI gates clean** — `cargo +nightly fmt --all --check` (clean) + `cargo clippy -p
   nexus-orchestration -- -D warnings` (clean) + `cargo clippy --all -- -D warnings` (clean, CI
   equivalent). Evidence: `3f2efc03`; command outputs above.
9. **Re-export from lib.rs if part of public API** — `narrative_index` is `pub mod` in `lib.rs:11`;
   `ForeshadowingStatus` + `IndexParseError` + `ForeshadowingStatusError` are `pub` items accessible
   via `nexus_orchestration::narrative_index::ForeshadowingStatus`. No additional re-export needed
   (no external-crate consumer exists per grep; module-path access is the established pattern).

## Design decisions

- **Case-insensitive FromStr policy**: `ForeshadowingStatus::from_str` accepts mixed casing
  (`PLANNED`, `Buried`, `PAID_OFF`) and surrounding whitespace, but requires the canonical underscore
  form (`paid_off`, not `paidoff` / `paid-off`). This tolerates author typos while keeping the wire
  vocabulary closed. `Display` always emits canonical lowercase + underscore for a stable round-trip.
  Documented in the enum docstring; locked by `foreshadowing_status_fromstr_is_case_insensitive`.
- **`read_foreshadowing_summary` graceful degradation**: the function returns `Option<String>` and is
  used for best-effort prompt injection. An invalid status cell (parse error) is logged at `warn!` and
  the summary is omitted (`None`) so prompt injection degrades gracefully rather than surfacing a
  corrupt index to the model. The old empty-status → "planned" default is removed (the parser now
  guarantees valid status; empty cells are a parse error).
- **`FDeclaration.id` stays `Option<String>`**: extraction now only yields `Some`, but the struct type
  is kept as `Option` to minimize struct-signature churn. The promotion loop has a defensive `let
  Some(id) = decl.id else { continue; }` guard with a comment documenting that it is unreachable for
  well-formed extraction output. Clippy does not flag this.
- **No serde impls added**: `ForeshadowingRow` does not derive `Serialize`/`Deserialize` anywhere in
  the codebase (grep-verified); the status field flows through Markdown table serialization only. Per
  the assignment ("impl serde if ForeshadowingRow derives serde"), serde is skipped.
- **W-1 + W-2 in one commit**: the changes are tightly coupled (the parse signature change from W-1
  propagates into the promotion code that W-2 also modifies — same function, same diff hunks). A
  single commit avoids artificially splitting intermingled hunks.

## Residual closure

- **R-V149P1-03** (medium) — `ForeshadowingRow.status` untyped → typed enum migration (qc1 W-1):
  **fixed in this wave** (`3f2efc03`). DO NOT archive; PM owns closure after targeted re-review passes.
- **R-V149P1-04** (medium) — `extract_inline_f_declarations` / `promote_outline_to_index`
  over-allocation → require `F###` token (qc1 W-2): **fixed in this wave** (`3f2efc03`). DO NOT
  archive; PM owns closure after targeted re-review passes.

## Risks / follow-ups

- **Behavior change — prose bullets no longer allocate F### ids**: any existing author workflow that
  relied on writing `- description` bullets (without an `F###` token) to auto-allocate ids will stop
  working. The prompt contract (`outline-chapter.md`) already instructs authors to use the `F###:`
  form, so the impact is limited to undocumented usage. This is the intended W-2 fix.
- **Parse error on hand-edited files with invalid status**: a manually-edited `foreshadowing.md` with
  a typo'd status (e.g. `Payed off`) will now cause `parse_foreshadowing_index` to return `Err`. In
  `read_foreshadowing_summary`, this is handled gracefully (warn + None). In `promote_outline_to_index`,
  the error propagates as `anyhow::Error` and is logged at `warn!` by the caller
  (`promote_foreshadowing_for_schedule`), which is non-fatal per the best-effort contract.
- **`next_f_id` no longer called in promotion**: `next_f_id` is now only used by its own unit test.
  This is a pre-existing condition (analogous to `next_e_id` noted in qc1 S-5); left as `pub` since
  the V1.50 E###/F### CRUD writer may consume it.

## Ready for targeted QC re-review: yes

- Reviewer: `@qc-specialist` (N=1; only qc1 raised blocking findings).
- qc2 + qc3 stay approved per `mstar-review-qc` default (targeted re-review after fix).
- Review cwd: `.worktrees/v1.49-p1-w1-w2-fix`
- Working branch: `fix/v1.49-p1-w1-w2-typed-and-allocation`
- Review range: `eb75a73d..3f2efc03` (the fix commit)
- plan_id: `2026-06-17-v1.49-narrative-indexes`
