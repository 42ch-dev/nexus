---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-06-18-v1.50-auto-chronology"
working_branch: "feature/v1.50-auto-chronology"
review_cwd: "/Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v150-auto-chronology"
review_range: "merge-base eceb22507259b8d7f1f1ffbeacfc3258c4c8059e..44b03171edb3e399c287827af0d17e8254937c74"
verdict: "Request Changes"
generated_at: "2026-06-17T15:36:00Z"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Runtime Model: zhipuai-coding-plan/glm-5.2
- Review Perspective: Architecture coherence and maintainability risk (Reviewer #1)
- Report Timestamp: 2026-06-17T15:36:00Z

## Scope
- plan_id: 2026-06-18-v1.50-auto-chronology
- Review range / Diff basis: merge-base eceb22507259b8d7f1f1ffbeacfc3258c4c8059e..44b03171edb3e399c287827af0d17e8254937c74
- Working branch (verified): feature/v1.50-auto-chronology
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v150-auto-chronology
- Files reviewed: 15
- Commit range (identical to Review range): eceb2250..44b03171 (7 commits)
- Tools run: `git diff`, `git log`, `cargo test -p nexus-orchestration --test auto_chronology_tick`, `cargo test -p nexus42 --test chronology_cli`, `cargo test -p nexus-daemon-runtime --test auto_chronology_task`, `cargo test -p nexus-local-db --test migrations_apply`, `cargo clippy --all -- -D warnings`, `cargo +nightly fmt --all --check`

## Findings

### 🔴 Critical

None.

### 🟡 Warning

#### W-001 — Template render does not substitute Work `title` / `total_planned_chapters` (spec §4.1 step 3 divergence)

**Where**: `crates/nexus-orchestration/src/auto_chronology.rs:436` and `:480`.

Spec `auto-chronology.md` §4.1 step 3 is normative:

> 3. Substitute author info from `Works/<work_ref>/README.md` frontmatter (title, total_planned_chapters, etc.).

`perform_advance` accepts `title` and `total_planned_chapters` parameters and `render_volume_outline` correctly substitutes `{{title}}` / `{{total_planned_chapters}}` in the template. **However, both call sites supply hardcoded placeholders instead of the Work's real values:**

```rust
// advance_auto (line 431-443)
perform_advance(pool, workspace_dir, work_id, &work_ref,
    "(untitled)",   // <-- title hardcoded
    None,           // <-- total_planned_chapters hardcoded
    prev_volume, next_volume, 0, "daemon auto_chronology_tick").await

// advance_manual (line 475-487)
perform_advance(pool, workspace_dir, work_id, &work_ref,
    "(untitled)",   // <-- title hardcoded
    None,           // <-- total_planned_chapters hardcoded
    prev_volume, next_volume, chapter_count.unwrap_or(0), "manual cli advance").await
```

The DAO rows (`WorkAutoChronologyRow` via `list_works_with_auto_chronology`, and the `load_row_for_manual` SELECT) deliberately fetch only the gating/locator columns — they do **not** include `works.title` or `works.total_planned_chapters`, even though both columns exist on the `works` table (see `WorkRecord` in `works.rs:60-105`).

**Observable impact**: every auto-generated outline renders as `# Volume N Outline — (untitled)` with `total_planned_chapters = (unset)`, regardless of the actual Work. The plan's Completion Report claims the template "renders correctly" (plan §Validation, AC §6.5), which is inaccurate against the normative spec text.

**Why Warning, not Suggestion**: this is an undocumented divergence from normative spec text with user-facing output degradation, and the plan's Issues/Risks section documents two other scope decisions but **not** this one — so it reads as an oversight rather than a deliberate Draft-scope reduction.

**Fix (small, localized)**:
1. Add `title: String` and `total_planned_chapters: Option<i32>` to `WorkAutoChronologyRow`.
2. Extend the two SELECT statements (`list_works_with_auto_chronology`, `load_row_for_manual`) to project those columns.
3. Thread `row.title` / `row.total_planned_chapters` into the two `perform_advance` calls in place of `"(untitled)"` / `None`.
4. Add a test asserting the rendered outline contains the Work's real title (the existing `render_substitutes_all_placeholders` unit test only exercises the pure render function with synthetic inputs, not the integration wiring).

### 🟢 Suggestion

#### S-001 — Plan §6 Verification references a non-existent test target

**Where**: `.mstar/plans/2026-06-18-v1.50-auto-chronology.md:78`.

Plan §6 lists `cargo test -p nexus-local-db --test works_auto_chronology_migration` as a verification command, but no such test target exists in `nexus-local-db` (available targets: `migrations_apply`, `works_schedule_migration`, `versions_roundtrip`, …). Coverage is still adequate — the migration is transitively exercised by `migrations_apply` (2 tests, including `all_migrations_apply_to_fresh_db`) and the default-`false` behavior is asserted via `chronology_show_defaults_false` — so this is documentation drift, not a coverage gap. Recommend either correcting the plan's verification line to the actual command (`cargo test -p nexus-local-db --test migrations_apply`) or adding a dedicated `works_auto_chronology_migration` test that asserts the column default on an existing row.

## Source Trace

| Finding ID | Source Type | Source Reference | Confidence |
| --- | --- | --- | --- |
| W-001 | git-diff + doc-rule | `auto_chronology.rs:431-443, 475-487`; `works.rs` `WorkAutoChronologyRow`; spec `auto-chronology.md` §4.1 step 3 | High |
| S-001 | doc-rule | plan `2026-06-18-v1.50-auto-chronology.md:78`; `cargo test -p nexus-local-db --test works_auto_chronology_migration` → no such target | High |

## Architecture & Maintainability Assessment (Reviewer #1 focus)

**Layering — correct abstraction (mirrors `cron_supervisor` precedent).** The reusable advance logic (finish detection, render, atomic write, tx seed, log) lives in `nexus-orchestration::auto_chronology` (`run_one_tick`, `advance_auto`, `advance_manual`, `perform_advance`), and `nexus-daemon-runtime::auto_chronology` is a thin spawn wrapper (`AutoChronologyConfig::from_env`, `spawn_auto_chronology_tick`, `run_one_tick` passthrough). This is the same split used by `cron_supervisor` (orchestration `evaluate_cron_fires` + runtime `spawn_cron_supervisor`) and `stale_findings_watcher`. The boot spawn (`let _chron_handle = spawn_auto_chronology_tick(...)` inside a block scope → handle dropped → task detached for daemon lifetime) is byte-for-byte consistent with `_cron_handle` and `_watcher_handle`. ✓

**Atomic write — matches V1.36 pattern.** `write_outline_atomic` (temp + `sync_all` + `rename`, with best-effort temp cleanup on error) is identical in shape to the established helpers in `rules_history.rs:51`, `narrative_index.rs:695`, `rules_layers.rs:275`, and `completion_lock.rs:74`. ✓

**Migration — correctly ordered, no collision.** `202606180005_works_auto_chronology.sql` follows `202606180004_works_schedule_json_partial_idx.sql`; filename date-stamp matches the migration content; `BOOLEAN NOT NULL DEFAULT 0` preserves existing-Work behavior (opt-in only). ✓

**Crash recovery — sound and tested.** The outline write precedes the DB tx (documented in the module doc comment). A crash after the atomic rename but before tx commit leaves an orphan outline; the next tick's idempotent guard (`outline.exists()` → `AlreadyAdvanced`) skips cleanly without re-seeding chapters. `tick_recovers_cleanly_after_crash_mid_advance` verifies this end-to-end (outline preserved, 0 v2 chapters after recovery). The spec §3.1 wording ("Atomic state.db tx wraps the entire advance") is slightly idealized — the file write is outside the tx — but the implemented idempotent-guard recovery is functionally equivalent and the behavior is accurately documented in code. ✓

**T-B P1 KB-extraction hook — correctly decoupled.** The `extract_kb_candidates_for_review` hook fires from the review role's terminal pipeline in `cron_supervisor` and operates on whatever `work_chapters` rows exist. Auto-chronology creates chapter rows via the standard DAO; the existing review/KB pipeline picks them up with no new coupling. Plan §3 Non-goals explicitly excludes "World KB auto-promotion from new volume (T-B P1 separate concern)", so the loose integration is intended. ✓

**Residuals R-V150P3AUTOCHRONO-01/02 — legitimate, low-severity, do not mask defects.** R-01 (no `total_planned_volumes` column → §3.1 "last planned volume" INFO skip subsumed by the completion-lock gate) and R-02 (auto path seeds zero chapters per spec §4.2 last paragraph) are both accurately characterized, spec-faithful scope decisions, and properly reported for PM registration (dev did not mutate `status.json`, per QC/PM ownership boundary). ✓

**Surgical changes.** 15 files, all directly scope-relevant; no unrelated refactoring. Standalone DAOs are used (no `WORKS_COLUMNS` / `WorkRecord` modification), consistent with the stated V1.50 T-A P0 precedent and the plan's T1 artifact table. ✓

**Clippy / fmt / sqlx conventions.** `cargo clippy --all -- -D warnings` clean; `cargo +nightly fmt --all --check` exit 0. The single `#[allow(clippy::too_many_arguments)]` on `perform_advance` carries the required justification comment (repo rule satisfied). All new `sqlx::query`/`query_as` runtime calls carry `// SAFETY:` comments citing the new-column prepare-cache rationale (consistent with `cron_supervisor` and the R-V140P0-S3 waiver). ✓

## Verification Results

```
cargo test -p nexus-orchestration --test auto_chronology_tick ... 9 passed, 0 failed
cargo test -p nexus42 --test chronology_cli .................... 9 passed, 0 failed
cargo test -p nexus-daemon-runtime --test auto_chronology_task . 2 passed, 0 failed
cargo test -p nexus-local-db --test migrations_apply ........... 2 passed, 0 failed
cargo clippy --all -- -D warnings ............................. clean
cargo +nightly fmt --all --check .............................. exit 0
```

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 1 |
| 🟢 Suggestion | 1 |

**Verdict**: Request Changes

The architecture, layering, atomicity, migration, crash-recovery, and test coverage are all sound and consistent with established repo precedents (`cron_supervisor`, V1.36 atomic write, standalone-DAO pattern). One unresolved Warning (W-001) blocks approval: the normative spec §4.1 step 3 requirement to substitute the Work's `title` / `total_planned_chapters` into the generated outline is not implemented — both advance paths hardcode `"(untitled)"` / `None`. The fix is small (extend two DAO rows + thread two values), localized, and testable. Per the QC gate rule (unresolved Warning → Request Changes), targeted re-review of W-001 is requested once fixed; everything else is Approve-quality.

---

## Revalidation

```yaml
report_kind: qc-revalidation
reviewer: qc-specialist
reviewer_index: 1
plan_id: 2026-06-18-v1.50-auto-chronology
working_branch: feature/v1.50-auto-chronology
review_cwd: /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v150-auto-chronology
review_range: 44b03171..75e6a426
fix_wave_commits:
  - d0770397 (R-V150P3AUTOCHRONO-03 work title/total_chapters)
  - 1abd5a57 (R-V150P3AUTOCHRONO-04 outline atomicity reorder)
  - d310a13b (R-V150P3AUTOCHRONO-05 env-var override)
  - 42ce8bfa (R-V150P3AUTOCHRONO-06 sqlx macros)
  - 75e6a426 (plan completion report)
verdict: Approve
generated_at: 2026-06-17T16:22:17Z
```

**Reviewer**: @qc-specialist (Reviewer #1 — architecture coherence + maintainability)
**Re-review scope**: targeted — verify W-001 (R-V150P3AUTOCHRONO-03) is resolved by fix-wave `44b03171..75e6a426` (5 commits); cross-check that R-04 / R-05 / R-06 do not regress the architecture/layering points raised in the initial review.

### W-001 disposition — RESOLVED ✓

**Evidence** (commit `d0770397`, `crates/nexus-local-db/src/works.rs` + `crates/nexus-orchestration/src/auto_chronology.rs` + template):

1. **DAO row extended** — `WorkAutoChronologyRow` (`works.rs:1655-1670`) now carries `pub title: String` and `pub total_planned_chapters: Option<i32>`, each documented at field level ("Always non-NULL" / "None when the author has not set it"). Doc comment on the struct updated to cite spec §4.1 step 3.
2. **Both SELECTs project the new columns**:
   - `list_works_with_auto_chronology` (`works.rs:1686-1699`) — `query_as!` macro projects `title as "title!"` and `total_planned_chapters as "total_planned_chapters: i32"`.
   - `load_row_for_manual` (`auto_chronology.rs:257-265`) — string SQL extended with `title, total_planned_chapters`.
3. **Both advance paths thread real values** (no more hardcoded placeholders):
   - `advance_auto` (`auto_chronology.rs:458-464`): `&row.title` / `row.total_planned_chapters` (was `"(untitled)"` / `None`).
   - `advance_manual` (`auto_chronology.rs:516-520`): same substitution.
4. **Template wired** — `volume-outline.md.tmpl` references `{{total_planned_chapters}}` in the Arc & Throughline section so the value renders in output (1-line addition).
5. **Regression test** `render_substitutes_work_title_and_total_chapters` (`auto_chronology_tick.rs:155-191`) pre-seeds `title = "The Hobbit"` + `total_planned_chapters = Some(20)`, runs `run_one_tick`, and asserts the rendered outline contains `"The Hobbit"` and `"20"` AND does NOT contain `"(untitled)"` or `"(unset)"`. This is the integration-wiring test the initial review asked for (the existing `render_substitutes_all_placeholders` only exercised the pure render fn with synthetic inputs).

**Test run**:
```
cargo test -p nexus-orchestration --test auto_chronology_tick ... 12 passed; 0 failed
  (incl. render_substitutes_work_title_and_total_chapters ... ok)
```

The fix matches the 4-step plan from W-001's "Fix (small, localized)" verbatim and adds the requested integration test. Spec §4.1 step 3 is now satisfied.

### Architecture / layering cross-check (initial-review points preserved)

The fix-wave touched additional code via R-04 / R-05 / R-06. Verified none regress the four architecture/layering points praised in the initial review:

| Initial-review point | Status under fix-wave |
|---|---|
| **Layering mirrors `cron_supervisor`** (orchestration owns pure logic; daemon-runtime is a thin spawn wrapper) | ✅ Preserved + improved. R-05 extracted `parse_interval_secs` as a pure `Option<&str> -> u64` fn inside `nexus-daemon-runtime::auto_chronology` — production `from_env` reads env then delegates; tests pass values hermetically (no `std::env::set_var`/`remove_var`). Same split as `cron_supervisor`. |
| **Atomic write pattern (V1.36)** (`write_outline_atomic` temp + sync_all + rename) | ✅ Preserved; ordering improved. R-04 moved the DB tx commit to **before** the outline write ("DB first, FS second" — the canonical V1.36 pattern). The `write_outline_atomic` helper itself is unchanged. Module doc + `perform_advance` doc both rewritten to document the new ordering and the post-commit outline-failure recovery path. `tick_recovers_cleanly_after_crash_mid_advance` still passes; new `perform_advance_writes_outline_after_tx_commit` proves chapters persist when the outline write fails post-commit. |
| **Migration ordering** (`202606180005_works_auto_chronology.sql`) | ✅ Untouched by fix-wave. No new migrations; the existing `title` / `total_planned_chapters` columns on `works` (already present) are now read by the DAO. `cargo test -p nexus-local-db --test migrations_apply` still 2 passed. |
| **sqlx conventions** (`// SAFETY:` comments on runtime queries) | ✅ Superseded by stronger convention. R-06 converted the new runtime `query`/`query_as` calls to compile-time `query!` / `query_as!` / `query_scalar!` macros (`set_auto_chronology`, `get_auto_chronology`, `list_works_with_auto_chronology`, `current_volume`, `is_volume_fully_finalized`, `seed_volume_chapters_tx`). The `// SAFETY:` comments are correctly removed (compile-time verification replaces the runtime rationale). `.sqlx/` cache files committed for offline build reproducibility. |

**R-04 `--force` flag** (new surface): `advance_manual` gained `force: bool` param; CLI exposes `--force` (default `false`) with help text "Bypass the idempotent guard and overwrite an existing outline (recovery path…)". Idempotent guard updated to `if !force && outline.exists()`. Daemon path always passes `false` (no `--force` for auto tick). Two regression tests cover both branches. Layering correct — CLI owns flag parsing, orchestration owns the semantics. `chronology_advance_help_documents_force_flag` confirms help text.

**Surgical changes**: 19 files in the fix-wave diff, all directly scope-relevant (fix targets + their tests + `.sqlx` cache + plan/completion docs). No unrelated refactoring.

### New finding (non-blocking)

#### S-002 — `#[allow(clippy::too_many_arguments)]` on `handle_advance` lacks justification comment

**Where**: `crates/nexus42/src/commands/creator/works/chronology.rs:185`.

The fix-wave added `force: bool` to `handle_advance`, pushing arg count to 8 and tripping `clippy::too_many_arguments`. The `#[allow(clippy::too_many_arguments)]` suppressor was added **without** the brief justification comment that repo `AGENTS.md` (§ Development Policy → Clippy) requires: *"Do not suppress with `#[allow(...)]` without a brief justification comment."*

The sibling function in the same fix-wave — `perform_advance` in `nexus-orchestration/src/auto_chronology.rs:289-298` — carries the model comment ("rationale: the 9 Work-specific inputs are read together from one row; packing them into a struct would duplicate the WorkAutoChronologyRow shape…"). Same lint, same pattern, one documented and one not.

**Why Suggestion, not Warning**: clippy passes (the allow is syntactically valid); no behavior impact; pure documentation-hygiene inconsistency. The fix is one line of `// rationale: …` comment above the attribute. Recommend aligning with the orchestration-layer precedent in a follow-up; does not block this plan's approval.

### Verification Results (fix-wave)

```
cargo test -p nexus-orchestration --test auto_chronology_tick ... 12 passed; 0 failed
  (incl. render_substitutes_work_title_and_total_chapters,
         perform_advance_writes_outline_after_tx_commit,
         manual_advance_force_overwrites_existing_outline)
cargo test -p nexus-daemon-runtime --test auto_chronology_task . 3 passed; 0 failed
  (incl. parse_interval_secs_handles_env_values, from_env_uses_default_when_unset)
cargo test -p nexus42 --test chronology_cli .................... 10 passed; 0 failed
  (incl. chronology_advance_help_documents_force_flag)
cargo test -p nexus-local-db --test migrations_apply ........... 2 passed; 0 failed
cargo clippy --all -- -D warnings ............................. clean
cargo +nightly fmt --all --check .............................. exit 0
```

### Summary

| Severity | Open (initial) | After fix-wave |
|---|---|---|
| 🔴 Critical | 0 | 0 |
| 🟡 Warning | 1 (W-001) | **0** — W-001 resolved |
| 🟢 Suggestion | 1 (S-001) | 2 (S-001 doc drift + new S-002 lint-allow comment) |

**Verdict**: **Approve**

W-001 (the sole blocking Warning) is fully resolved with the exact fix pattern proposed in the initial review, plus an integration-wiring regression test that asserts both the positive substitution and the absence of placeholders. The four architecture/layering points praised in the initial review (cron_supervisor layering, V1.36 atomic write, migration ordering, sqlx conventions) are all preserved — and three of them are improved by the companion fix-wave (R-04 strengthens atomicity ordering; R-05 improves test hermeticity via pure-fn extraction; R-06 upgrades runtime queries to compile-time macros). The two open Suggestions (S-001 plan-doc drift, S-002 missing lint-allow comment) are non-blocking documentation-hygiene nits suitable for PM residual registration.
