---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: "2026-06-11-v1.42-multi-volume"
verdict: "Approve"
generated_at: "2026-06-11"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: grok-build-0.1 (xai/grok-build-0.1)
- Review Perspective: security and correctness risk (focus per mstar-roles parameter table)
- Report Timestamp: 2026-06-11T19:xx:xx+0800 (QC worktree session)

## Scope
- plan_id: 2026-06-11-v1.42-multi-volume
- Review range / Diff basis: merge-base: c249c902 (P0 QA-merge) + tip: HEAD of iteration/v1.42 (929fe5bd) — equivalent to `git diff c249c902...HEAD`
- Working branch (verified): HEAD (detached)
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.42-p1-qc
- Files reviewed: 14 (per diff --stat)
- Commit range: 9 commits (9fefdfbc … 929fe5bd)
- Tools run:
  - git rev-parse --show-toplevel, --abbrev-ref HEAD, log -1, log c249c902..HEAD --oneline, diff c249c902..HEAD --stat
  - cargo test -p nexus-local-db -p nexus-orchestration -p nexus42 -- volume chapter
  - cargo test -p nexus-daemon-runtime --test works_api
  - cargo test -p nexus-orchestration --test novel_project_init
  - cargo clippy -p nexus-local-db -p nexus-orchestration -p nexus42 -p nexus-daemon-runtime -- -D warnings
  - cargo +nightly fmt --all --check
  - Read of migration DDL, work_chapters.rs (DAO), auto_chain.rs (cross-volume evaluator + runtime_lock acquire), works.rs handler (enrich + status), novel_scaffold.rs (multi-volume preset), local-db-schema.md, deferred-features tracker, plan, status.json

## Findings

### 🔴 Critical
- None identified in P1 scope.

### 🟡 Warning
- **W-01 (carry-forward, not P1-introduced)**: Two pre-existing test failures on base commit c249c902 (confirmed per Assignment): `handler_append_inspiration_returns_404_for_unknown` (500 vs 404) and `patch_work_stage_change_is_auditable` (runtime_lock contention in test setup). These are outside P1 diff and were present before the 9 P1 commits. Per Assignment "2 pre-existing test failures on `c249c902` base — confirmed pre-existing".
- **W-02 (known carry-forward, per Assignment + R-V140P4-INFRA pattern)**: `.sqlx/` regeneration deferred for this plan. No schema drift introduced by P1 migration (migration is committed DDL); compile-time query macros for new table paths use documented runtime `sqlx::query` + SAFETY comments per crate AGENTS.md (nexus-local-db, post-migration-cycle table). Not a new correctness defect.

### 🟢 Suggestion
- **S-01**: Migration DDL uses the standard SQLite "rename + recreate + INSERT SELECT" pattern for PK change. Explicit backfill `UPDATE ... SET volume=1 WHERE volume IS NULL` precedes the structural change; data rows are copied verbatim before legacy table drop. No data-loss path visible. Consider adding a one-time migration test harness (e.g., seed pre-migration rows, apply migration, assert volume=1 + PK uniqueness) if future volume-related migrations are anticipated — not required for this ship.
- **S-02**: All volume-aware entry points enforce `volume >= 1` (scaffold validation + DB DEFAULT 1 + DAO params). Single-volume regression path (volume=1 implicit) and cross-volume boundary (next_chapter_volume_aware ORDER BY volume ASC, chapter ASC on active statuses) are covered by hermetic tests (t6_ac1, t6_ac2, t6_ac3, t7c). No new injection surface: user-controlled values (total_volumes, total_planned_chapters, chapter) are validated before any SQL; all queries use bound parameters (runtime queries are for the post-migration table only, with no string concatenation of identifiers).
- **S-03**: Cross-volume auto-chain correctly respects P0 runtime_lock acquire contract (§4.2): `evaluate_after_persist_volume_aware` path calls `acquire_runtime_lock` (holder = schedule) before calling `next_chapter_volume_aware` and enqueuing the next produce schedule. See auto_chain.rs:548 (acquire), 679 (P0 skip of locked Works in supervisor), and P1 T4 evaluator.
- **S-04**: Status API (`/v1/local/works/{id}`) now returns `next_chapter_volume` (in addition to `next_chapter`) via `enrich_with_chapters` + `next_chapter_volume_aware`. DTO and chapter row serialization include `volume`. CLI formatting polish is explicitly P-last (non-goal).
- **S-05**: DF-62 tracker correctly updated to "V1.42 P1 Shipped" with plan reference and commit list. No residual open rows for DF-62 in active tracker.

## Source Trace
- **Migration safety (backfill + PK atomicity)**: crates/nexus-local-db/migrations/202606110001_v142_multi_volume_pk.sql (Steps 1–4: UPDATE NULL→1, RENAME, CREATE with new PK (work_id, volume, chapter) + DEFAULT 1, INSERT SELECT, DROP legacy, recreate indexes). Matches local-db-schema.md V1.42 amendment and novel-workflow-profile §4.5.4.
- **DAO volume-aware contract**: crates/nexus-local-db/src/work_chapters.rs (seed_chapters_multi_volume / _tx, list_chapters ORDER BY volume,chapter, next_chapter_volume_aware, get_chapter_by_volume, insert_chapter with volume param, update_status volume-aware). All new paths default single-volume to volume=1.
- **Cross-volume auto-chain + runtime_lock**: crates/nexus-orchestration/src/auto_chain.rs (ChainAction::NextChapter now carries next_volume; evaluate_after_persist_volume_aware uses next_chapter_volume_aware; acquire_runtime_lock before decision at 548–572; P0 skip at 679). Respects P0 §4.2 production acquire contract.
- **Status data plane**: crates/nexus-daemon-runtime/src/api/handlers/works.rs (WorkApiDto adds next_chapter_volume; enrich_with_chapters calls list_chapters + next_chapter_volume_aware; serializes volume in chapter rows).
- **Init preset + validation + scaffold**: crates/nexus-orchestration/src/capability/builtins/novel_scaffold.rs (ScaffoldInput total_volumes default=1; validation >=1 && <= total_planned_chapters; multi-volume outline "volume-N-outline.md" + per-volume chapter dirs; seed_chapters_multi_volume_tx when >1 volume).
- **Hermetic coverage**: crates/nexus-orchestration/tests/novel_project_init.rs (t6_ac1 single-volume regression defaults volume=1; t6_ac2 two-volume happy path; t6_ac3 status API volume-aware rows; t7c work_chapters rows seeded correctly for multi-volume).
- **Tracker / status**: .mstar/knowledge/deferred-features-cross-version-tracker.md (DF-62 row → "V1.42 P1 Shipped"); .mstar/status.json (plan InReview, notes record PM merge + QC dispatch).
- **Pre-existing failures**: Confirmed on base via test runs on detached HEAD at 929fe5bd (post all P1 merges); not in P1 diff.

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 2 (both carry-forward / explicitly pre-existing or known-deferred per Assignment) |
| 🟢 Suggestion | 5 (non-blocking improvements / positive observations) |

**Verdict**: Approve

All P1 security and correctness acceptance criteria are met:
- Existing Works continue as single-volume (volume=1 backfill + implicit default) with no user action.
- New multi-volume Works: init captures volume count + per-volume chapters; seeds correctly; auto-chain crosses volume boundary when prior volume finalized.
- Status API returns volume-aware chapter rows + next_chapter_volume.
- No new SQL injection, path traversal, or unvalidated volume/chapter inputs.
- Cross-volume auto-chain path acquires runtime_lock per P0 contract.
- Hermetic tests pass for the new multi-volume ACs (22/22 in novel_project_init); clippy and nightly fmt clean.
- DF-62 shipped via T7 (tracker updated).

The two pre-existing test failures and .sqlx/ deferral are out-of-scope for this P1 review (documented carry-forwards). No new blocking findings under security/correctness lens.
