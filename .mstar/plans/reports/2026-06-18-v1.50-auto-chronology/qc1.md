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
