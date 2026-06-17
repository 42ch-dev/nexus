---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-06-18-v1.50-kb-refreshable-scan"
working_branch: "feature/v1.50-kb-refreshable-scan"
review_cwd: "/Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v150-kb-refreshable-scan"
review_range: "merge-base c2831fa25ae7732bac1fe1a11a318e5a7b1626b2..e24574ae5f5f6e8186ee87fa1bc3d3acdc5f885c"
verdict: "Approve"
generated_at: "2026-06-17T14:01:28Z"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Runtime Model: zhipuai-coding-plan/glm-5.2
- Review Perspective: Architecture coherence + maintainability
- Report Timestamp: 2026-06-17T14:01:28Z

## Scope
- plan_id: 2026-06-18-v1.50-kb-refreshable-scan
- Review range / Diff basis: merge-base c2831fa25ae7732bac1fe1a11a318e5a7b1626b2..e24574ae5f5f6e8186ee87fa1bc3d3acdc5f885c
- Working branch (verified): feature/v1.50-kb-refreshable-scan
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v150-kb-refreshable-scan
- Files reviewed: 9 (1 plan doc + 4 source + 2 test + 1 migration rename + 1 lib.rs export)
- Commit range (identical to Review range): c2831fa2..e24574ae (6 commits)
- Tools run: `git log`, `git diff --stat`, `git show`, `cargo clippy --all -- -D warnings`, `cargo test -p nexus-local-db --test kb_extract_jobs_upsert`, `cargo test -p nexus42 --test kb_rescan_cli`

## Architecture Coherence Assessment

### Layering (clean)
The three-layer split is coherent and matches the plan §5 task boundaries:

- **`nexus42` CLI** (`commands/creator/kb/rescan.rs`) — orchestration only: arg
  parsing, work resolution, authz, prose read, then delegates to the two
  domain crates. No domain logic leaked into the CLI layer.
- **`nexus_local_db::kb_extract_job`** (T1 DAO) — persistence: idempotent
  upsert / list-for-chapter / delete-pending-for-chapter. Extends the existing
  module rather than spawning a parallel one; reuses the V1.29 `(creator,
  work_entry_id, world_id)` unique surface that V1.50 P1 aliased to
  `canonical_name_guess`.
- **`nexus_kb::extract_sync`** (T3) — pure domain delta (`compute_kb_diff` is
  I/O-free; `diff_and_apply` adds the store write). Correctly lives behind the
  `KbStore` trait, with hermetic `InMemoryKbStore` tests in-module.

`nexus_orchestration::quality_loop::extract_candidates_from_text` is reused
unchanged — no duplicated heuristic.

### `creator kb rescan` placement (coherent)
T-B P0/P1 used the `creator world kb` subgroup (adopt/edit/delete/pending) for
the **promotion** surface. This plan places `rescan` directly under
`creator kb` (alongside `queue-extract` / `extract-status`), not under
`creator world kb`. This is a defensible call: the rescan's **primary** effect
is re-extracting `kb_extract_jobs` candidates (the `creator kb` extract-queue
domain); the confirmed-`KeyBlock` body refresh is a secondary downstream
effect bridging into the world-KB domain. The `Rescan` variant doc-comment
explains the bridging. No command-tree ambiguity results.

### Composite-key constraint surface (clear)
The logical candidate identity for a rescan is
`(source_chapter_id, canonical_name_guess)`; the DB enforces uniqueness on
`(creator_id, work_entry_id=canonical_name_guess, world_id) WHERE status NOT IN
('failed')`. The gap between logical identity and DB uniqueness is **fully
documented** in three places: `UpsertOutcome` rustdoc, `upsert_pending_candidate`
rustdoc, and plan §7 design decision 1. The behavioral consequence (a candidate
re-extracted from a *different* chapter reuses the existing row and refreshes
its `source_chapter_id`) is intentional, tested
(`upsert_never_duplicates_across_chapters_for_same_name`), and prevents
cross-chapter duplication. Constraint surface is clear.

### §5.5 invariant respected
`diff_and_apply` only refreshes `body` of active `KeyBlock`s; it never inserts
or deletes `kb_key_blocks`. `inserted` / `removed` in `KbSyncDiff` are
explicitly advisory and documented as such. This correctly reserves
create/delete for `creator world kb adopt|edit|delete` per entity-scope-model
§5.5.2 (confirmed → terminal). The `is_active_status` helper mirrors the
partial unique index predicate. Clean.

### R-V150KBED-07 / R-V150KBED-08 scope (correctly deferred)
- R-V150KBED-07 (fuller sync surface for V1.51+ LLM extraction) — correctly
  out of scope; §5.5 reserves create/delete for the adopt gate.
- R-V150KBED-08 (cross-chapter dry-run preview fidelity) — correctly out of
  scope; preview is chapter-scoped and documented as faithful on a clean
  per-chapter basis.

Both are low-severity and properly targeted at V1.51+.

### Surgical changes (confirmed)
9 files / +1946 −13. Every touched file maps 1:1 to a plan task (T1–T6) or the
R-V150KBED-06 blocker fix. The 2-line `lib.rs` change is the expected
`pub mod` + re-export. No piggyback refactors, no unrelated formatting churn.

## Findings

### 🔴 Critical
(none)

### 🟡 Warning
- **W-001 — R-V150KBED-06 migration renumber direction conflicts with T-A P2's
  R-V150P2CRONRV-01 (merge hazard).** This branch renumbers
  `202606180002_works_schedule_json_partial_idx.sql` → `...0003_…` (keeping
  T-B P1's `kb_extract_jobs_extend` at `0002`). The sibling T-A P2 branch
  (`feature/v1.50-cron-foundation`, commit `12495be8`) resolves the **same**
  duplicate-version collision in the **opposite** direction: it keeps
  `works_schedule_json_partial_idx` at `0002` and renumbers
  `kb_extract_jobs_extend` → `...0003_…`. Both fixes are individually correct
  and self-consistent (each branch's tests pass), but when PM merges both into
  `iteration/v1.50`, git will surface a rename/rename conflict on the
  `migrations/` directory. PM must pick **one** canonical assignment at
  integration time. Neither direction is functionally preferable (both
  migrations are pure DDL with no internal version literal; per both commit
  messages no DB ever recorded either `0002` cleanly because the collision
  prevented any successful apply, so there is no pre-existing dev-DB hazard).
  **This is a PM merge-coordination item, not a code defect in this branch —
  nothing the implementer changes here can resolve it.** → Register as residual
  `R-V150KBED-06-MERGE` (owner: `@project-manager`; resolve at iteration
  integration by choosing one canonical direction; document the chosen mapping
  in the merge commit).

### 🟢 Suggestion
- **S-001 — `diff_and_apply` re-scans linearly after `compute_kb_diff` already
  built a HashMap.** `compute_kb_diff` builds `active_by_name: HashMap` for
  O(1) lookup, but `diff_and_apply` then does a linear `old_rows.iter().find`
  per updated row (lines 161–164) plus a linear `new_extracted.iter().find`
  per row (lines 168–171), yielding O(n·m) for the apply phase. At KB scale
  (tens of rows per world, bounded by narrative scope) this is negligible; no
  change required for this plan. If V1.51+ raises extraction cardinality,
  consider threading the HashMap from compute into apply. Low confidence /
  non-blocking.

## Source Trace
- Finding W-001
  - Source Type: git-diff + cross-branch inspection
  - Source Reference: `git show 2cbd5498` (this branch, T-B P2 fix) vs
    `git show 12495be8` in `feature/v1.50-cron-foundation` (T-A P2 fix);
    both touch `crates/nexus-local-db/migrations/20260618000{2,3}_*.sql`
    in opposite rename directions.
  - Confidence: High
- Finding S-001
  - Source Type: manual-reasoning
  - Source Reference: `crates/nexus-kb/src/extract_sync.rs:146-187`
  - Confidence: Low

## Validation Evidence
- `cargo clippy --all -- -D warnings` → exit 0 (clean; workspace pedantic+nursery).
- `cargo test -p nexus-local-db --test kb_extract_jobs_upsert` → **6 passed; 0 failed**.
- `cargo test -p nexus42 --test kb_rescan_cli` → **8 passed; 0 failed**.
- `git branch --show-current` → `feature/v1.50-kb-refreshable-scan` (matches Assignment).
- `git log --oneline c2831fa2..HEAD` → 6 commits (matches Assignment).

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 1 |
| 🟢 Suggestion | 1 |

**Verdict**: Approve

The branch is architecturally coherent, cleanly layered, surgically scoped,
and fully verified (clippy clean + 14 new tests green). The single Warning
(W-001) is a cross-branch merge-coordination hazard between this plan's
R-V150KBED-06 fix and T-A P2's R-V150P2CRONRV-01 fix — it is **not** a defect
in the reviewed code and is not resolvable by changing this branch. It is
registered as residual `R-V150KBED-06-MERGE` for `@project-manager` to resolve
at iteration integration (pick one canonical migration-number assignment;
document in the merge commit). Per the "Approve with residuals" allowance
(no open Critical), this verdict is **Approve**.
