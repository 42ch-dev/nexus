---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: "2026-06-08-v1.38-multi-chapter-selection-status"
verdict: "Request Changes"
generated_at: "2026-06-08"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: grok-build-0.1 (xai/grok-build-0.1)
- Review Perspective: Security and correctness risk (per role parameters)
- Report Timestamp: 2026-06-08

## Scope
- plan_id: 2026-06-08-v1.38-multi-chapter-selection-status
- Review range / Diff basis: merge-base(3f72b085, HEAD)..HEAD on iteration/v1.38 (commit 2abbaa1a merge(v1.38-p0) brings in feature commit ffeb0adc feat: V1.38 P0 multi-chapter selection and status foundation).
- Working branch (verified): iteration/v1.38
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 10 (per `git diff --stat`)
- Commit range: 3f72b08547ec64ef21ea504743f440f5f946f48..2abbaa1a2662c1cf6941a423322f5accae849cc3 (merge) + ffeb0adcdbda1ebed051bec6179f5718b932d6ab (feature)
- Tools run: git (rev-parse, branch, log, merge-base, diff --stat, diff --name-status, show), Read (plan, spec §4.1/4.5/6/8.1, compass, role ref, implementation files), Grep (SQL, function names, creator_id, current_chapter), Bash (clippy on 4 crates)

## Acceptance Criteria Review

| AC | Statement (plan §6) | Status in diff | Evidence |
|----|---------------------|----------------|----------|
| 1 | A Work with chapters 1 finalized and 2..N not_started selects chapter 2. | Met | `next_chapter` T10.1 test + 3-tier logic (not_started first) in work_chapters.rs:511 |
| 2 | A Work with an existing draft chapter resumes that draft and does not create/select a later chapter. | Met | `test_next_chapter_resumes_draft` (T10.2) + draft tier after not_started/outlined |
| 3 | An `outlined` chapter is not skipped. | Met | `test_next_chapter_outlined_not_skipped` (T10.3) + outlined tier in selection |
| 4 | `current_chapter` updates only on finalize and becomes the just-finalized chapter number. | Met | novel_chapter_transition.rs:221 (T6 comment) + only the "finalized" branch calls `advance_current_chapter`; draft branch removed |
| 5 | Completion fires only when every planned chapter row is `finalized`, `current_chapter >= total_planned_chapters`, and intake is complete. | Met | `is_work_completed` (work_chapters.rs:581) now checks intake, current >= total, row count == total, all finalized + early completed status |
| 6 | `creator run status <work_id>` shows chapter progress, row statuses, and a next action without auto-enqueueing. | Met | CLI rewrite in creator/run.rs (per-chapter table, progress, current/total, next_chapter hint, "user-controlled" language) |
| 7 | Tests cover chapter 2..N selection and one-chapter compatibility. | Met | 7 new hermetic tests (T10.1–T10.7) + V1.36 compat test; all use fresh_pool() |

## Findings

### 🔴 Critical
- None.

### 🟡 Warning
- **W1 (Correctness — selection race window)**: `next_chapter(work_id)` (work_chapters.rs:511) performs three separate non-transactional `SELECT ... ORDER BY chapter ASC LIMIT 1` queries (not_started → outlined → draft). No `BEGIN` / `SELECT FOR UPDATE` / status-claim atomicity. Two concurrent `creator run continue <work_id>` (or two terminals for the same creator) can both observe the same "next" chapter (e.g. both see ch2 as not_started) and both enqueue a `novel-writing` schedule for it before either transition has flipped the row to draft/finalized. The later `novel.chapter_transition` guard will reject a mismatched `from_status`, but the first schedule may have already started outline/draft work on a chapter the second schedule also claimed, or the user sees duplicate "in progress" schedules for the same chapter. Per spec §4.5.2 the query contract is the SSOT for selection; the implementation matches the letter but introduces a concurrency correctness gap for the "user-controlled continue" UX. The single-user "run, wait for completion, run again" path is safe; the race is only under concurrent invocation. (Files: `crates/nexus-local-db/src/work_chapters.rs:511-560`, callers in daemon `get_work` enrichment and CLI continue path.)
  - Recommended: document the single-writer assumption for P0, or add a lightweight "claim" step (e.g. a `try_claim_next_chapter` that does an UPDATE ... WHERE status=... RETURNING chapter inside the same tx) in a follow-up if multi-terminal use is common. Do not treat as auto-chain (still deferred).

- **W2 (Plan delivery — T9 surface of missing-file hints)**: Plan T9 and spec §8.1 require the status surface to "warn on missing files without selecting a later DB row". The CLI status rewrite (creator/run.rs) correctly derives the chapter list and `next_chapter` exclusively from DB (`list_chapters` + `next_chapter` via the enriched DTO) and contains a detailed code comment acknowledging on-disk absence. However, the visible output in the reviewed diff emits the per-chapter DB rows + progress + next-action hint but does not appear to emit an explicit "⚠ body file missing on disk for chNN (DB says draft)" line for chapters whose `body_path` is absent from the workspace filesystem. The daemon `reconcile-chapters` path exists for remediation, but the `status` command (the primary user surface) only shows DB truth. This is a partial delivery of the "surface ... hints" requirement in the status UX for this P0 slice. (File: `crates/nexus42/src/commands/creator/run.rs` around the novel-profile status arm; the comment is present but the printf/warn emission for missing on-disk files is not visible in the diff.)

### 🟢 Suggestion
- **S1 (Documentation / residual hygiene)**: The race in W1 and the partial T9 surface in W2 are both bounded (DB remains SSOT; chapter_transition guards protect against double-finalize; no data loss or cross-creator leak). If PM accepts these as known P0 residuals (to be addressed in P1 or a later hardening plan), they should be registered in `status.json` `residual_findings` with severity "medium" (or "low" for the surface hint) and a target milestone. The implementation already correctly avoids the deferred items listed in the compass §1.2 (no auto-chain, no World KB, no quality-loop findings, no multi-volume PK, no platform publish).

- **S2 (Minor test gap)**: The new hermetic tests are strong and use per-test `fresh_pool()`. One edge not explicitly asserted in the new T10 block is "Work exists with `total_planned_chapters=NULL` or 0 chapters seeded" for `is_work_completed` (the code path returns false early, which is correct per the doc comment, but an explicit test case would make the §6.1 guard behavior self-documenting). Existing older tests and the setup helper cover the happy path. Low impact.

- **S3 (DAO layering)**: `next_chapter` and `is_work_completed` are low-level DAOs that take only `work_id`. This is the correct internal pattern (callers such as the daemon handler and CLI schedule path are responsible for creator/workspace scoping, which they do via prior `get_work` / authenticated flows). No cross-creator PII leak is introduced by the diff.

## Source Trace
- Finding W1: `git diff ... work_chapters.rs` (the three `sqlx::query` SELECT blocks without tx); callers via `enrich_with_chapters` and CLI continue wiring.
- Finding W2: `git diff ... creator/run.rs` (novel-profile status arm + the "T9: blocked/missing-file hints" comment block that does not emit a visible warning in the printed output).
- AC verification: direct mapping to `test_next_chapter_*`, `test_completion_*`, `test_one_chapter_v136_compatible` in the same file; `novel_chapter_transition.rs` finalize-only diff; preset.yaml `{{preset.input.chapter}}` + version 5 bump.
- Scope check: `git show ffeb0adc --stat` + full diff against the 10 listed files only; no changes under auto-chain, World KB, findings, multi-volume, platform, or selection-pool paths.

## Diff Scope Check
The diff (10 files, +738/-60) is strictly within the P0 plan scope (T1–T10):
- `work_chapters.rs` (new `next_chapter`, enhanced `is_work_completed`, 7 new tests)
- `nexus-daemon-runtime/.../works.rs` (WorkApiDto enrichment + `enrich_with_chapters`)
- `nexus42/.../creator/run.rs` (status UX rewrite + chapter input on start path)
- `novel_chapter_transition.rs` (draft branch removed; finalize-only `current_chapter` advance)
- `stage_gates.rs` + `preset/mod.rs` (WorkFields + `build_preset_input` for `chapter`)
- `embedded-presets/novel-writing/preset.yaml` (version 4→5; all three chapter:1 sites replaced by `{{preset.input.chapter}}`)
- Two test helper fixes (e2e_novel_writing, fl_e_chain_demo)

**Explicitly untouched (per compass §1.2 and plan Non-goals)**: auto-chain / DF-53, World KB / DF-63, quality-loop findings (DF-64/65/66/67), multi-volume PK migration, platform publish, multi-work switch, selection pool. No residual risk from scope creep.

## CI Gate Evidence
```bash
cargo clippy -p nexus-local-db -p nexus-orchestration -p nexus-daemon-runtime -p nexus42 -- -D warnings
```
Exit status: 0 (clean; "Finished `dev` profile" with no warnings promoted to errors). Full output truncated for brevity; no `-D warnings` violations on the four crates.

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 2 |
| 🟢 Suggestion | 3 |

**Verdict**: Request Changes

(The two Warnings are correctness-oriented and plan-delivery-oriented. They are not data-integrity or injection Criticals, but per the shared QC baseline any unresolved Warning blocks Approve. The implementation is otherwise tight, matches the spec contract, preserves DB-as-SSOT, restricts `current_chapter` correctly, and stays inside the declared deferred boundary.)
