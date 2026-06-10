---
report_kind: qc-review
reviewer: "@qc-specialist"
reviewer_index: 1
focus: architecture-coherence-maintainability
plan_id: 2026-06-10-v1.41-selection-pool
verdict: Request Changes
generated_at: 2026-06-10T15:30:00+08:00
review_range: "merge-base: 55689706 → tip: 57f573ad"
working_branch_verified: iteration/v1.41
review_cwd_verified: /Users/bibi/workspace/organizations/42ch/nexus
files_reviewed: 11
tools_run: cargo clippy, cargo +nightly fmt --check, cargo test, manual review
---

# Code Review Report — V1.41 P1 (qc1)

## Reviewer Metadata
- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Runtime Model: deepseek-v4-pro
- Review Perspective: Architecture coherence and maintainability risk
- Report Timestamp: 2026-06-10T15:30:00+08:00

## Scope
- plan_id: 2026-06-10-v1.41-selection-pool
- Review range / Diff basis: merge-base: 55689706 → tip: 57f573ad
- Working branch (verified): iteration/v1.41
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 11
- Commit range: 55689706..57f573ad (focus on 7 P1 commits: b3a1f023, dfff13f8, 8066caf6, 78c89aad, b7435629, f81365b6, b96c6576)
- Tools run: cargo clippy, cargo +nightly fmt --check, cargo test, manual review

## Findings
### 🔴 Critical
(None)

### 🟡 Warning
- **W-1: `PoolEntryDto` missing `title` field — CLI display always shows `"?"` for title**
  → `PoolEntryDto` (works.rs:238–245) has 6 fields but omits `title`, which exists in the DAO `PoolEntry` struct (novel_pool_entries.rs:18–35). The CLI `handle_pool_list` (works/mod.rs:536) reads `e.get("title")` but always gets `"?"` because the API response never includes it. Fix: add `pub title: String` to `PoolEntryDto` and populate it in all three `PoolEntryDto` construction sites (list_pool:1331–1341, promote_pool_entry:1374–1381, archive_pool_entry_handler:1399–1406).

- **W-2: `archive_pool_entry_handler` and `archive_inspiration_handler` don't verify creator ownership**
  → Both handlers read `_creator_id` from config (prefix `_` = unused) but pass only `entry_id`/`item_id` to the DAO layer. The DAO functions `archive_pool_entry` (novel_pool_entries.rs:159) and `archive_inspiration` (inspiration_items.rs:219) accept only `entry_id`/`item_id` — no `creator_id` parameter. This means creator A can archive creator B's pool entries and inspiration items. The `promote_pool_entry` handler correctly verifies work ownership via `works::get_work(pool, &creator_id, &req.work_id)` (works.rs:1355–1361). Fix: either (a) add `creator_id` parameter to both DAO archive functions and filter by it, or (b) verify ownership in the handler before calling the DAO.

- **W-3: R-V141P1-N06 (medium) — MD scaffold path resolution not routed through `nexus-home-layout`**
  → `inspiration_items.rs:140` hard-codes `Works/_pool/灵感池/{slug}.md` relative to the passed `workspace_dir`. The daemon handler `add_inspiration` (works.rs:1420–1421) resolves `workspace_dir` from `state.workspace_path()`, which in production is the operational workspace dir under `~/.nexus42/creators/<id>/workspaces/<slug>/` (set via `WorkspaceState`). This means the production path IS correctly resolved through the daemon layer — the MD file ends up under `~/.nexus42/creators/<id>/workspaces/<slug>/Works/_pool/灵感池/`. However, the DAO function `create_inspiration_with_scaffold` accepts a generic `&Path` and has no compile-time guarantee that the caller resolved it correctly. The completion-report.md §6 correctly flags this as medium severity. Fix: document in `inspiration_items.rs` doc comment that `workspace_dir` must be the resolved operational workspace path; or add a `nexus-home-layout` dependency to the DAO crate for compile-time path resolution.

### 🟢 Suggestion
- **S-1: `PoolEntryDto` and `InspirationItemDto` expose `creator_id` in API response**
  → Both DTOs include `creator_id` (works.rs:240, 1669), which is an internal field. The `WorkApiDto` (works.rs:35–93) intentionally omits `creator_id` and `workspace_slug` per the R-V133P1-10 contract. For consistency, consider removing `creator_id` from pool/inspiration DTOs or documenting why it's exposed (e.g., for multi-creator admin tooling).

- **S-2: `PoolEntryDto` construction duplicated 3 times — extract helper**
  → The same `PoolEntryDto { entry_id, creator_id, work_id, status, promoted_at, note }` construction appears in `list_pool` (1331–1341), `promote_pool_entry` (1374–1381), and `archive_pool_entry_handler` (1399–1406). Extract a `From<PoolEntry> for PoolEntryDto` impl or a helper function. Same pattern for `InspirationItemDto` (2 sites: 1470–1482, 1608–1617).

- **S-3: `title_to_slug` for pure-CJK titles produces `"untitled"` — UX could be improved**
  → `title_to_slug("灵感和创意")` returns `"untitled"` because all CJK characters become hyphens, which collapse to empty (inspiration_items.rs:327). The completion-report.md §6 R-V141P1-N04 suggests auto-appending a numeric suffix for collisions, but the deeper issue is that pure-CJK titles produce uninformative slugs. Consider appending a short random suffix (e.g., `npi_` prefix hash) when the slug would be `"untitled"`.

## Source Trace
- Finding ID: W-1
- Source Type: manual-reasoning
- Source Reference: works.rs:238–245 (PoolEntryDto) vs novel_pool_entries.rs:18–35 (PoolEntry), works/mod.rs:536 (CLI title display)
- Confidence: High

- Finding ID: W-2
- Source Type: manual-reasoning
- Source Reference: works.rs:1385–1407 (archive_pool_entry_handler), works.rs:1594–1618 (archive_inspiration_handler), novel_pool_entries.rs:159–179 (archive_pool_entry DAO), inspiration_items.rs:219–234 (archive_inspiration DAO)
- Confidence: High

- Finding ID: W-3
- Source Type: manual-reasoning
- Source Reference: inspiration_items.rs:140 (hard-coded path), works.rs:1420–1421 (daemon resolution), completion-report.md:121 (R-V141P1-N06)
- Confidence: Medium

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 3 |
| 🟢 Suggestion | 3 |

**Verdict**: Request Changes

### Architecture coherence assessment

The P1 design fits V1.41 compass §0.1 row 6 cleanly: `novel_pool_entries` (P0 schema extension) and `inspiration_items` (new table) have well-separated responsibilities. The DAO layer is the right home for both. The `mark_work_completed` hook (auto_chain.rs:279–292) correctly lives in `nexus-orchestration` — it's a cross-cutting orchestration concern, not a pure data-layer operation. The CLI subcommand tree (`PoolAction` → `InspirationAction`) is nested correctly and consistent with P0's `creator works` structure.

Route paths are consistent: P0's `/v1/local/works/pool` (action-based) coexists with P1's RESTful `/v1/local/works/pool/{promote,archive,inspiration,...}`. The dual pattern is acceptable for this iteration but could be unified in a future refactor.

### CI results

- `cargo clippy -p nexus42 -p nexus-daemon-runtime -p nexus-orchestration -p nexus-local-db -- -D warnings` → **clean** (0 warnings, 0 errors)
- `cargo +nightly fmt --all -- --check` → **clean**
- `cargo test -p nexus42 -p nexus-daemon-runtime -p nexus-orchestration -p nexus-local-db` → **all pass** (no flakes encountered)
