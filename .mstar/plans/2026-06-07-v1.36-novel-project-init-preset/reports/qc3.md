---
report_kind: qc_review
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: 2026-06-07-v1.36-novel-project-init-preset
verdict: Request Changes
generated_at: 2026-06-07T19:15:00Z
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-3
- Runtime Agent ID: qc-specialist-3
- Runtime Model: k2p6
- Review Perspective: Performance + reliability risk
- Report Timestamp: 2026-06-07T19:15:00Z

## Scope
- plan_id: 2026-06-07-v1.36-novel-project-init-preset
- Review range / Diff basis: merge-base: iteration/v1.36 (1856258) + tip: feature/v1.36-novel-project-init-preset (2a97858)
- Working branch (verified): feature/v1.36-novel-project-init-preset
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.36-p1-init
- Files reviewed: 30 (1876 insertions, 120 deletions)
- Commit range: 1856258..2a97858
- Tools run: cargo +nightly clippy -p nexus-orchestration -p nexus42 -p nexus-local-db, cargo test (manual binary execution)

## Findings

### 🔴 Critical
*None.*

### 🟡 Warning

**W-1: Template engine divergence — custom string replacement instead of handlebars-rust**
- **Location**: `crates/nexus-orchestration/src/capability/builtins/novel_scaffold.rs:41-51`
- **Issue**: The spec (novel-workflow-profile.md §5.4.2) explicitly states templates are rendered "using `handlebars-rust` (per orchestration-engine.md §7.3)". The implementation uses a custom `render_template()` function that does naive `String::replace` for `{{key}}` placeholders. This diverges from the declared engine, lacks handlebars features (conditionals, loops, partials), and sets a precedent for ad-hoc template parsing across presets.
- **Fix**: Replace `render_template` with the `handlebars` crate (already in `Cargo.toml` as a dependency). Pre-compile templates at capability construction time or use a static registry.
- **Blocks P2?**: No — functional for V1.36 MVP, but should be fixed before P3 novel-writing preset templates (which may need conditionals for world-bound vs worldless Works).

**W-2: `seed_chapters` does not reuse prepared statement; N unbounded**
- **Location**: `crates/nexus-local-db/src/work_chapters.rs:56-87`
- **Issue**: The `seed_chapters` function loops `1..=total_chapters` and calls `sqlx::query(...)` with a hard-coded SQL string on each iteration. While sqlx may cache prepares internally, the function does not explicitly use a prepared statement. More importantly, `total_chapters` is an `i32` with schema `minimum: 1` but no maximum bound — a malicious or buggy caller could pass `i32::MAX` and attempt to insert 2 billion rows in a single transaction.
- **Fix**: (a) Add a `const MAX_PLANNED_CHAPTERS: i32 = 1000` (or per-spec 100) guard at the top of `seed_chapters` and `ScaffoldInput` validation; (b) Use `sqlx::query` with explicit prepared statement reuse or batch insert.
- **Blocks P2?**: Yes — unbounded N is a reliability risk (OOM, transaction timeout, WAL exhaustion on SQLite).

**W-3: No filesystem rollback on partial scaffold failure**
- **Location**: `crates/nexus-orchestration/src/capability/builtins/novel_scaffold.rs:177-270`
- **Issue**: The spec (§5.4) says: "the entire scaffold (mkdir + template copies + work_chapters inserts + works PATCH) must succeed or fail together. If any step fails, the engine rolls back filesystem deletes and DB inserts in a single transaction." The DB uses a transaction (`tx.begin()` … `tx.commit()`), but filesystem operations (`create_dir_all_idem`, `write_file_idem`) are performed *before* the DB transaction and are NOT rolled back if the DB seed or PATCH fails. A failure after file creation leaves orphaned directories/files.
- **Fix**: Either (a) perform all filesystem ops inside the DB transaction with a compensating cleanup on `tx.rollback()`, or (b) write to a temp directory first, then atomically rename on DB commit (more complex). For V1.36, documenting the limitation is acceptable if cleanup is added on error paths.
- **Blocks P2?**: Yes — spec mandates atomicity; current implementation violates the contract.

**W-4: `NovelProjectScaffold::run()` logs scaffold completion before verifying DB success**
- **Location**: `crates/nexus-orchestration/src/capability/builtins/novel_scaffold.rs:228-248`
- **Issue**: The `info!` logs for "chapters seeded" and "works patched" appear *inside* the success arms of the DB operations, which is correct. However, there is no log for "scaffold started" or a summary log at the very end, making it harder to correlate failures in distributed tracing. More importantly, if `pool` is `None`, the function silently skips DB operations and returns `chapters_seeded: 0` with no warning log — callers may not realize the DB was untouched.
- **Fix**: Add `tracing::info!(work_id, "novel.project_scaffold: starting");` at the top, and a `tracing::warn!` when `pool` is `None` (test-only path is fine, but production registry always provides a pool — still worth logging defensively).
- **Blocks P2?**: No — observability gap, not functional.

### 🟢 Suggestion

**S-1: Template cache for embedded presets**
- **Location**: `crates/nexus-orchestration/src/preset/mod.rs:207-214`
- **Issue**: `read_embedded_template` reads from the `include_dir!` static map and returns a fresh `String` on every call. For the scaffold capability (4 templates × 2 calls each = 8 allocations per init), this is negligible, but as more presets adopt templates, repeated allocation becomes avoidable overhead.
- **Fix**: Consider a `Lazy<HashMap<...>>` or `DashMap` cache for deserialized template strings, keyed by `(preset_id, template_path)`. Measure first — likely only matters at scale.

**S-2: Add `total_planned_chapters` upper bound to CLI arg validation**
- **Location**: `crates/nexus42/src/commands/creator/run.rs`
- **Issue**: The CLI accepts `--init-preset novel-project-init` but the `total_planned_chapters` value comes from the grill-me (ACP conversation), not a CLI arg. Still, any future CLI path that accepts chapter count directly should validate the upper bound.
- **Fix**: If a CLI flag for chapters is added later, enforce the same `MAX_PLANNED_CHAPTERS` constant used in `seed_chapters`.

## Source Trace

| Finding ID | Source Type | Source Reference | Confidence |
|---|---|---|---|
| W-1 | manual-reasoning | `novel_scaffold.rs:41-51`, `novel-workflow-profile.md §5.4.2` | High |
| W-2 | manual-reasoning | `work_chapters.rs:56-87`, JSON schema `minimum: 1` (no maximum) | High |
| W-3 | manual-reasoning | `novel_scaffold.rs:177-270`, `novel-workflow-profile.md §5.4` | High |
| W-4 | manual-reasoning | `novel_scaffold.rs:228-248` | Medium |
| S-1 | manual-reasoning | `preset/mod.rs:207-214` | Low |
| S-2 | manual-reasoning | `run.rs` CLI args | Low |

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 4 |
| 🟢 Suggestion | 2 |

**Verdict**: Request Changes

The implementation is functionally correct and well-tested (8 hermetic tests pass in ~0.2s), but three warnings block P2 readiness:

1. **W-2 (unbounded N)** is a clear reliability risk — `seed_chapters` must cap `total_planned_chapters`.
2. **W-3 (no FS rollback)** violates the spec's atomicity contract for the scaffold protocol.
3. **W-1 (template engine)** diverges from the declared `handlebars-rust` requirement. While it works for simple `{{key}}` substitution today, it creates technical debt before P3 templates need conditionals.

W-4 (logging gap) and S-1/S-2 are non-blocking but should be addressed in the fix round or documented as residual.

**Real commit hash**: 2a97858

## Next dispatch
Targeted re-review expected after W-1, W-2, W-3 fixes. PM should assign reviewer #3 (this seat) for re-validation of performance/reliability fixes.
