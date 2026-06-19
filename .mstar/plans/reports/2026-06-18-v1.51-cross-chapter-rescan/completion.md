# Completion Report v2 — V1.51 T-A P1 Cross-Chapter Rescan

**Agent**: `fullstack-dev` (track=primary)
**Plan**: `2026-06-18-v1.51-cross-chapter-rescan`
**Status**: Done (implementation complete; awaiting PM QC tri-review dispatch)
**Task category**: `logic`
**Working branch**: `feature/v1.51-cross-chapter-rescan` (from `iteration/v1.51` @ `388602d2`)
**Merge target**: `iteration/v1.51`
**Worktree path**: `/Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.51-t-a-p1`

---

## Summary

Shipped `creator kb rescan --work <work_ref>` cross-chapter reconciliation: a
mutually-exclusive work-scoped mode that iterates all chapters in
`Works/<work_ref>/Stories/`, aggregates review-time candidates by
`canonical_name` across chapters, and upserts **once per aggregate** so the
`kb_extract_jobs` DB uniqueness `(creator, canonical_name, world)` collapses N
same-name per-chapter candidates into a single pending row carrying a
`source_chapters:[3,5,7]` provenance array. `--dry-run` surfaces a cross-chapter
reuse summary; the non-dry path acquires the T-B P0 advisory lock
(`Works/<work_ref>/.lock`) before the upsert with the dual exit-code contract
(75/78). **Closes `R-V150KBED-08`.** The V1.50 chapter-scoped path is unchanged
(AC2 non-breaking). A `TODO(T-B P1)` marker is left at the upsert call-site for
the versioned CAS swap.

---

## Artifacts

### New files

| Path | LoC | Purpose |
| --- | --- | --- |
| `crates/nexus42/tests/kb_rescan.rs` | 380 | V1.51 T-A P1 hermetic round-trip: 11 cross-chapter tests (AC1/AC3/AC4/AC5 + error paths). |

### Modified files

| Path | Δ | Change |
| --- | --- | --- |
| `crates/nexus-orchestration/src/quality_loop.rs` | +190 | `AggregatedCandidate` struct + `aggregate_candidates_by_canonical_name` pure fn (group by lowercased canonical_name; merge source_chapters into payload; preserve first-seen case + LLM metadata) + `inject_source_chapters` helper + 8 unit tests. |
| `crates/nexus42/src/commands/creator/kb/rescan.rs` | +560 | `kb_rescan_work` (config entry) + `kb_rescan_work_hermetic` (testable) + `WorkRescanReport` + `CrossChapterReuse` + `extract_per_chapter` + `acquire_work_lock` + `sync_work_candidates` + `preview_work_candidate_outcome` + `delete_pending_for_chapter_work` + `sync_work_kb_rows` + `print_work_report`. Advisory lock integration (T-B P0). `TODO(T-B P1)` CAS marker. |
| `crates/nexus42/src/commands/creator/kb.rs` | +35/-6 | clap `Rescan` variant: `target: Option<String>` + `--work <WORK_REF>` mutually-exclusive flag; dispatch routes to chapter-scoped or work-scoped; `pub use` extended. |
| `.mstar/knowledge/world-kb-runtime-architecture.md` | +84 | §5.5.1 Cross-chapter reconciliation body (Normative): flow, DB-uniqueness collapse rule, dry-run reuse summary, heuristic pathway choice, T-B P0 lock integration, T-B P1 CAS hook. |
| `.mstar/knowledge/specs/cli-spec.md` | +42 | §6.2G V1.51 T-A P1 amendment: `--work` flag table + mutual-exclusivity, author gate, reconciliation, dry-run, advisory lock (exit 75/78), extraction pathway. |
| `.mstar/status.json` | +26 | `R-V150KBED-08` lifecycle `deferred`→`resolved` + closure_evidence + resolution; new residual `R-V151-MERGE-CLIPPY-01` (pre-existing clippy regression, routed to P-last). |
| `.mstar/plans/2026-06-18-v1.51-cross-chapter-rescan.md` | +0/-0 | T1–T7 checkboxes ticked. |

---

## Spec bodies authored (acceptance §6 / plan T1)

| # | Spec path | Body |
| --- | --- | --- |
| 1 | `.mstar/knowledge/world-kb-runtime-architecture.md` §5.5.1 | Cross-chapter reconciliation subsection (flow + reconciliation rules + dry-run summary + pathway + advisory lock + CAS hook). |
| 2 | `.mstar/knowledge/specs/cli-spec.md` §6.2G | V1.51 T-A P1 `--work <work_ref>` amendment (command table + 6 rules). |

---

## Residual closure

**`R-V150KBED-08`** — closed.

- **Was**: V1.50 T-B P2 shipped `creator kb rescan <chapter>` chapter-scoped
  only; an entity referenced across N chapters generated N pending candidates
  the author adopted one-by-one. Severity `low`, deferred V1.50 → V1.51 T-A P1.
- **Now**: `creator kb rescan --work <work_ref>` iterates all chapters,
  aggregates by `canonical_name`, and collapses N same-name per-chapter
  candidates into 1 pending row carrying `source_chapters:[3,5,7]`. `--dry-run`
  shows cross-chapter reuse; non-dry acquires the T-B P0 advisory lock (exit
  75/78).
- **status.json patch**: under
  `residual_findings["2026-06-18-v1.50-kb-refreshable-scan"]` →
  `lifecycle: "resolved"`, `closed_at: "2026-06-18"`, `closure_evidence`
  (commit pointers + 27 test names), `resolution { plan_id, commit }`.
- **Evidence tests**:
  `quality_loop::tests::aggregate_*` (8),
  `kb_rescan::cross_chapter_*` (11 — same-entity-collapse, distinct-entities,
  dry-run-reuse-summary, existing-kb-match-refresh, stale-removal,
  idempotent-rerun, lock-contention→E_LOCK, dry-run-under-lock,
  lock-io→E_LOCK_IO, missing-work, cross-author-403),
  `kb_rescan_cli` V1.50 chapter-scoped regression (8, unchanged).

---

## Verification (acceptance §6 — all commands run; output captured)

Run from worktree `feature/v1.51-cross-chapter-rescan` HEAD `65148bb8`.

```text
1. cargo test -p nexus42 --test kb_rescan -- --nocapture          → 11 passed; 0 failed  (AC1/AC3/AC4/AC5)
2. cargo test -p nexus-orchestration --lib quality_loop::tests::aggregate_
                                                                   → 8 passed; 0 failed   (T3 aggregation)
   [plan filter `rescan_cross_chapter` matched nothing; aggregation
    unit tests are `aggregate_*` in quality_loop — used that filter]
3. cargo test -p nexus-local-db --test kb_extract_jobs_upsert      → 6 passed; 0 failed
   cargo test -p nexus-local-db --test kb_extract_jobs_migration   → 12 passed; 0 failed (8 V1.50 + 4 V1.51)

V1.50 chapter-scoped regression (AC2 — non-breaking):
4. cargo test -p nexus42 --test kb_rescan_cli                      → 8 passed; 0 failed
5. cargo test -p nexus-kb --lib extract_sync                       → 7 passed; 0 failed
   [plan filter `rescan_chapter` matched nothing; chapter-scoped delta
    logic lives in nexus-kb extract_sync + kb_rescan_cli — used those]

T-A P0 LLM pathway regression (preserved):
6. cargo test -p nexus-orchestration --lib llm_extract             → 15 passed; 0 failed (11 builtin + 4 task)
7. cargo test -p nexus-orchestration --test novel_review_master    → 3 passed; 0 failed

T-B P0 advisory lock regression (preserved):
8. cargo test -p nexus-local-db --test file_lock                   → 3 passed; 0 failed
9. cargo test -p nexus42 --test cli_lock_contention                → 3 passed; 0 failed
   T-A P0 adopt (lock integration preserved):
   cargo test -p nexus42 --test creator_world_kb_adopt             → 3 passed; 0 failed

Static gates:
10. cargo +nightly fmt --all --check                               → exit 0 (clean)
11. cargo clippy --all -- -D warnings                              → 1 ERROR (PRE-EXISTING, see below)
```

**Clippy gate — 1 pre-existing error (NOT caused by T-A P1).** The CI command
`cargo clippy --all -- -D warnings` fails with exactly one error:
`world/kb.rs:440 kb_adopt too_many_lines (118/100)` — introduced by the T-B P0
advisory-lock block added to `kb_adopt` (merged in `d22075ef`). Verified
pre-existing per `.mstar/AGENTS.md` "Pre-existing claim verification protocol":
`git checkout 388602d2` (clean base) reproduces the same single error before
any T-A P1 changes. T-A P1's own files (`rescan.rs`, `kb.rs`,
`quality_loop.rs`, `tests/kb_rescan.rs`) produce **zero** clippy errors under
the CI command (confirmed: `cargo clippy --all -- -D warnings 2>&1 | grep -E
"rescan.rs|kb.rs|quality_loop.rs"` is empty). Registered as
`R-V151-MERGE-CLIPPY-01` (medium, → P-last WL-A). Not fixed in this plan per
surgical-changes no-piggyback (`kb_adopt` is outside T-A P1 scope).

> Note: the plan's verification sketch used estimated test-name filters
> (`rescan_cross_chapter`, `kb_extract_jobs_upsert`, `rescan_chapter`). The
> actual test names/locations are reported above. `kb_extract_jobs_upsert`
> does exist (6 tests); `rescan_cross_chapter`/`rescan_chapter` are not test
> names in the suite — the equivalent coverage is the `aggregate_*` unit tests
> + the `cross_chapter_*` integration tests + `extract_sync` unit tests.

---

## T-B P1 hook

The work-scoped upsert uses the V1.50 non-versioned `upsert_pending_candidate`
today. T-B P1 will add a `version` column + `cas_update` helper to
`kb_extract_jobs`; the single call-site swap is marked with a `TODO(T-B P1)`
comment in `sync_work_candidates` (`crates/nexus42/src/commands/creator/kb/rescan.rs`):

```text
// TODO(T-B P1): swap this non-versioned upsert_pending_candidate call
// for the versioned CAS path (kb_extract_jobs.version column +
// cas_update helper) once T-B P1 ships.
```

The advisory lock acquired in `kb_rescan_work_hermetic` is the cross-process
guard; the future CAS will be the per-row optimistic guard; both compose (lock
before DB, never reversed). See `world-kb-runtime-architecture.md` §5.5.1
"T-B P1 CAS hook".

---

## Acceptance criteria mapping (plan §4 + assignment §Acceptance)

| AC | Status | Evidence |
| --- | --- | --- |
| 1. `--work` performs cross-chapter reconciliation; hermetic asserts aggregation | ✅ | `kb_rescan::cross_chapter_same_entity_collapses_to_one_pending_row`. |
| 2. positional `<chapter>` continues V1.50 chapter-scoped (no breaking) | ✅ | `kb_rescan_cli` 8 tests unchanged; `kb_rescan_hermetic` untouched; clap `target: Option` preserves `Some(...)` path. |
| 3. `--dry-run` shows cross-chapter reuse summary before DB write | ✅ | `cross_chapter_dry_run_shows_reuse_summary_without_writing` + `CrossChapterReuse` in `WorkRescanReport`. |
| 4. 3 chapters same entity → 1 updated row (not 3 pending) | ✅ | `cross_chapter_same_entity_collapses_to_one_pending_row` (1 row, `source_chapters:[1,2,3]`); distinct → 3 rows. |
| 5. Advisory lock: contention → E_LOCK 75; I/O → E_LOCK_IO 78 | ✅ | `cross_chapter_lock_contention_returns_e_lock` + `cross_chapter_lock_io_failure_returns_e_lock_io`; dry-run under contention succeeds (no lock). |
| 6. R-V150KBED-08 closed with evidence | ✅ | `status.json` `lifecycle: resolved` + closure_evidence. |
| 7. 2 spec bodies authored | ✅ | §5.5.1 + §6.2G (this report §"Spec bodies authored"). |
| 8. No destructive schema change; additive only | ✅ | No DB migration; reuses V1.50 `kb_extract_jobs` uniqueness; `proposed_payload` extended with `source_chapters` array (additive JSON key). |
| 9. Wire contracts unchanged | ✅ | No `schemas/` change; local-only Rust + existing SQLite columns. |
| 10. No `#[allow(...)]` without justification | ✅ | `#[allow(clippy::too_many_arguments)]` on `sync_work_candidates` has justification comment (mirrors chapter-scoped `sync_candidates` precedent); `#[allow(clippy::future_not_send)]` mirrors existing CLI helpers; `#![allow(clippy::unwrap_used)]` in test file mirrors `kb_rescan_cli.rs`. |
| 11. No runtime behavior change outside scope | ✅ | Only the `Rescan` dispatch + new work-scoped path; chapter-scoped + all other commands untouched. |

---

## Risks / follow-ups

1. **R-V151-MERGE-CLIPPY-01 (pre-existing clippy regression, medium → P-last).**
   `cargo clippy --all -- -D warnings` fails on `iteration/v1.51` HEAD
   `388602d2` with a single error (`kb_adopt too_many_lines 118/100` from the
   T-B P0 lock block). Pre-existing-claim verified per `.mstar/AGENTS.md`
   protocol. Not fixed here per surgical-changes; routed to P-last WL-A. QC
   may adjudicate via the documented base SHA + reproduce command.
2. **Heuristic pathway in work-scoped rescan (by design).** Work-scoped rescan
   uses `extract_candidates_from_text` (heuristic), identical to the V1.50
   chapter-scoped path, so the two modes agree on the same prose. The
   `canonical_name` grouping key is the T-A P0 first-class field. Wiring the
   `nexus.llm.extract` LLM pathway into rescan is out of scope (LLM extraction
   is review-time/finalize-time; rescan is a sync tool). The aggregation is
   pathway-agnostic, so a future plan can swap the extractor without touching
   reconciliation. No residual opened — documented in §5.5.1.
3. **`source_chapters` provenance in `proposed_payload` (not a dedicated
   column).** Cross-chapter provenance is recorded as a `source_chapters` JSON
   array inside the existing `proposed_payload` (additive key; no migration).
   The dedicated `source_chapter_id` column holds the lowest referencing
   chapter (DB uniqueness needs a single value). If queryable/sortable
   cross-chapter provenance becomes needed, a future plan can add a dedicated
   column — out of scope here. No residual opened.
4. **`KbCandidate` dedup cap (`MAX_CANDIDATES_PER_PASS = 20`) is per-chapter.**
   Across N chapters the aggregate unique-name count is unbounded, but each
   chapter is capped at 20 (prevents flooding). Acceptable at V1.51 scale.
5. **Stale cleanup matches on `work_id` + `canonical_name` (not
   `source_chapter_id`).** The new `delete_pending_for_chapter_work` removes a
   stale pending candidate when its name vanished from ALL chapters of the work
   (distinct from the chapter-scoped `delete_pending_for_chapter` which is
   per-chapter). This is the correct work-scoped semantics; documented in §5.5.1.
6. **No platform integration.** Unchanged from V1.51 §1.5 (paused, local-only).

---

## Git context

- **Branch**: `feature/v1.51-cross-chapter-rescan`
- **Worktree**: `/Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.51-t-a-p1`
- **Diff basis**: `iteration/v1.51` (`388602d2`)
- **Tip**: `65148bb8`

```text
$ git log --oneline iteration/v1.51..HEAD
65148bb8 harness(v1.51-t-a-p1): close R-V150KBED-08 + register pre-existing clippy regression
1ca91a9f feat(nexus42): V1.51 T-A P1 — creator kb rescan --work cross-chapter reconciliation
42d5329a feat(nexus-orchestration): V1.51 T-A P1 — cross-chapter candidate aggregation
d5df2844 docs(specs): V1.51 T-A P1 — §5.5.1 cross-chapter reconciliation + cli-spec §6.2G --work flag body
```

4 commits, per-task-ID granularity (T1 specs / T3 aggregation / T2+T4+T5+T6+AC5
CLI+DB+dry-run+lock+tests / T7 residual). 7 files changed, +947/-29 (approx).

**Not merged into `iteration/v1.51`** — that is PM responsibility after QC
tri-review (qc1 architecture + qc2 security/correctness + qc3 perf/reliability)
+ QA verification.

---

## Handoff

To `@project-manager`: implementation complete on
`feature/v1.51-cross-chapter-rescan`; ready for QC tri-review dispatch.
Suggested `Review cwd` / `Worktree path`: this worktree
(`/Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.51-t-a-p1`).
`plan_id`: `2026-06-18-v1.51-cross-chapter-rescan`. Suggested `Review range` /
`Diff basis`: `merge-base: iteration/v1.51` + `tip: HEAD` (i.e.
`git diff iteration/v1.51...HEAD`).

**One item for PM attention before QC:** `R-V151-MERGE-CLIPPY-01` — the
`cargo clippy --all -- -D warnings` CI gate is red on the iteration base due to
a single pre-existing T-B P0 error (`kb_adopt too_many_lines`). Either route a
1-line hygiene fix before QC (extract the lock block from `kb_adopt`) or
QC-adjudicate via the documented pre-existing-claim evidence. T-A P1's own code
is clippy-clean.
