---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-06-08-v1.38-multi-chapter-selection-status"
verdict: "Approve"
generated_at: "2026-06-08"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Runtime Model: openai/gpt-5.5
- Review Perspective: architecture coherence and maintainability risk
- Report Timestamp: 2026-06-08

## Scope
- plan_id: 2026-06-08-v1.38-multi-chapter-selection-status
- Review range / Diff basis: `merge-base(3f72b085, HEAD)..HEAD` on `iteration/v1.38` (commit `2abbaa1a merge(v1.38-p0)` brings in feature commit `ffeb0adc feat: V1.38 P0 multi-chapter selection and status foundation`).
- Working branch (verified): `iteration/v1.38`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus`
- Files reviewed: 10
- Commit range: `3f72b08547ec64ef21ea5047437f440f5f946f48..HEAD` (`git merge-base 3f72b085 HEAD` resolved to `3f72b08547ec64ef21ea5047437f440f5f946f48`)
- Tools run:
  - `git rev-parse --show-toplevel`
  - `git branch --show-current`
  - `git log --oneline -5`
  - `git merge-base 3f72b085 HEAD`
  - `git diff --stat 3f72b085..HEAD`
  - `git diff --name-status 3f72b085..HEAD`
  - `git show --stat --oneline ffeb0adc`
  - Targeted Read/Grep of plan, spec, and touched Rust/YAML/prompt files
  - `cargo clippy -p nexus-local-db -p nexus-orchestration -p nexus-daemon-runtime -p nexus42 -- -D warnings` — passed
  - `cargo test -p nexus-local-db work_chapters` — passed (17 passed, 0 failed)

## Acceptance Criteria Review
| AC | Requirement | QC1 Result | Evidence |
| --- | --- | --- | --- |
| AC1 | A Work with chapter 1 finalized and 2..N not started selects chapter 2. | Pass | `next_chapter()` selects lowest `not_started`; test `test_next_chapter_selects_lowest_not_started` covers this. |
| AC2 | A Work with an existing draft chapter resumes that draft and does not create/select a later chapter. | **Fail** | `next_chapter()` checks all `not_started` rows before drafts, so ch2 `draft` + ch3 `not_started` selects ch3; test expectation codifies the opposite of the AC. |
| AC3 | An `outlined` chapter is not skipped. | **Fail** | `next_chapter()` checks all `not_started` rows before `outlined`, so ch1 `outlined` + ch2 `not_started` selects ch2; test expectation codifies the skip. |
| AC4 | `current_chapter` updates only on finalize and becomes the just-finalized chapter number. | Pass | `transition_db()` calls `advance_current_chapter()` only when `to_status == "finalized"`; no draft advance found in touched transition path. |
| AC5 | Completion fires only when every planned chapter row is `finalized`, `current_chapter >= total_planned_chapters`, and intake is complete. | Warning | Main non-completed path enforces all three checks, but an early return treats `works.status == 'completed'` as complete without re-checking those conditions. |
| AC6 | `creator run status <work_id>` shows chapter progress, row statuses, and a next action without auto-enqueueing. | Pass with caveat | CLI renders rows/progress/next action and does not enqueue; however hints inherit the incorrect `next_chapter` selection if an earlier draft/outlined row exists. |
| AC7 | Tests cover chapter 2..N selection and one-chapter compatibility. | **Fail** | Tests are hermetic and broad, but the draft-resume and outlined tests assert spec-divergent behavior, so coverage protects the wrong selection semantics. |

## Findings

### 🔴 Critical

- **F-001 — `next_chapter()` skips earlier in-progress chapters in favor of later `not_started` rows, contradicting spec §4.5.2 and plan AC2/AC3.**  
  `crates/nexus-local-db/src/work_chapters.rs:511-560` implements a global priority order of `not_started` → `outlined` → `draft`. That means `ch2=draft, ch3=not_started` selects ch3, and `ch1=outlined, ch2=not_started` selects ch2. The tests at `crates/nexus-local-db/src/work_chapters.rs:1125-1182` and `:1185-1242` explicitly assert this behavior. This contradicts the normative resume/outlined notes: a draft row with no earlier `not_started` row must resume and must not advance to a later chapter until finalized; an outlined chapter must not be skipped in favor of a later chapter. Architecturally, the scheduler and CLI status now share a helper that can advance the user to chapter N+1 while chapter N is unfinished, breaking serial chapter state coherence.  
  **Fix:** Select the lowest chapter among active serial candidates with ordering that preserves chapter order first, e.g. choose the lowest row whose status is in `('not_started', 'outlined', 'draft')`, with finalized/published excluded, or otherwise explicitly block later chapters when any lower chapter is `outlined`/`draft`. Update tests to assert ch2 draft beats ch3 not_started and ch1 outlined beats ch2 not_started.

### 🟡 Warning

- **F-002 — `is_work_completed()` still has a `works.status == 'completed'` early exit that bypasses the new all-three completion criteria.**  
  `crates/nexus-local-db/src/work_chapters.rs:581-628` documents §6.1 but returns `true` at `:595-598` before checking `total_planned_chapters`, `current_chapter`, `intake_status`, row count, or all-finalized rows. The daemon auto-promote path at `crates/nexus-daemon-runtime/src/api/handlers/works.rs:320-365` only calls the helper when the current Work status is not already `completed`, so a stale or incorrectly patched completed status will be treated as terminal without validating chapter rows.  
  **Fix:** Remove or narrow the early exit for novel-profile completion evaluation so §6.1 remains authoritative; if backward compatibility needs a legacy shortcut, keep it outside the novel-profile helper or explicitly document and test the migration exception.

### 🟢 Suggestion

- **F-003 — Prompt path formatting still bakes in `ch0{{chapter}}`, which is fragile for chapter 10+ and leaves an unused ch01-era prompt in the preset bundle.**  
  The YAML state injection correctly uses `{{preset.input.chapter}}`, but prompt templates still format paths as `ch0{{chapter}}` (`crates/nexus-orchestration/embedded-presets/novel-writing/prompts/outline-chapter.md:18-20`, `draft-chapter.md:18-22`, `draft-chapter.md:47-48`). For chapter 10 this yields `ch010`, not the usual `ch10`/`ch<nn>` format. Additionally, `prompts/draft-intro.md:13-16` still references `Stories/{{preset.input.story_ref}}/ch01-introduction.md`; it appears unused by `preset.yaml`, but retaining stale ch01-era prompt content in the embedded preset directory increases future parameterization risk.  
  **Fix:** In P1 or this slice if prompt parameterization is intended now, add a scheduler-supplied formatted chapter label/path (`chapter_label`/`body_path`/`outline_path`) instead of doing string padding in prompt prose, and remove or quarantine unused ch01 prompt artifacts.

## Source Trace

| Finding ID | Source Type | Source Reference | Confidence |
| --- | --- | --- | --- |
| F-001 | manual-reasoning + git-diff | Spec §4.5.2 (`novel-workflow-profile.md:309-328`), plan AC2/AC3 (`2026-06-08-v1.38-multi-chapter-selection-status.md:109-115`), implementation/tests (`work_chapters.rs:511-560`, `:1125-1242`) | High |
| F-002 | manual-reasoning + git-diff | Spec §6.1 (`novel-workflow-profile.md:759-778`), implementation (`work_chapters.rs:581-628`), daemon promotion (`works.rs:320-365`) | High |
| F-003 | grep + manual-reasoning | Prompt templates (`outline-chapter.md:18-20`, `draft-chapter.md:18-22`, `draft-chapter.md:47-48`, `draft-intro.md:13-16`) | Medium |

## Diff Scope Check

Reviewed diff scope from `3f72b085..HEAD` covers exactly the PM-listed 10 files:

| File | Scope assessment |
| --- | --- |
| `crates/nexus-daemon-runtime/src/api/handlers/works.rs` | In-scope Work API enrichment/completion status surface. |
| `crates/nexus-local-db/src/lib.rs` | In-scope re-export of `next_chapter` / completion helpers. |
| `crates/nexus-local-db/src/work_chapters.rs` | In-scope chapter selection/completion/test logic. Contains blocking selection divergence. |
| `crates/nexus-orchestration/embedded-presets/novel-writing/preset.yaml` | In-scope chapter input injection. |
| `crates/nexus-orchestration/src/capability/builtins/novel_chapter_transition.rs` | In-scope `current_chapter` finalize-only update. |
| `crates/nexus-orchestration/src/preset/mod.rs` | In-scope preset version test update. |
| `crates/nexus-orchestration/src/stage_gates.rs` | In-scope schedule input seam for selected chapter. |
| `crates/nexus-orchestration/tests/e2e_novel_writing.rs` | In-scope test fixture adjustment. |
| `crates/nexus-orchestration/tests/fl_e_chain_demo.rs` | In-scope test fixture adjustment. |
| `crates/nexus42/src/commands/creator/run.rs` | In-scope CLI status rendering and schedule input extraction. |

No diff hunks were found touching the explicitly deferred boundaries: auto-chain, World KB implementation, quality loop, multi-volume PK migration, platform publish, multi-work switch, or selection pool. The only deferred-boundary concern is prompt/template parameterization depth, recorded as Suggestion F-003 rather than scope creep because the YAML injection seam is part of this P0 implementation.

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 1 |
| 🟡 Warning | 1 |
| 🟢 Suggestion | 1 |

**Verdict**: Request Changes

## Revalidation (targeted re-review)

### Reviewer Metadata
- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Runtime Model: openai/gpt-5.5
- Review Perspective: Architecture coherence and maintainability risk (targeted re-review)
- Report Timestamp:2026-06-08
- Original verdict: Request Changes (initial tri-review)
- Targeted findings to re-verify: F-001 (Critical), F-002 (Warning), W-1 (Warning)

### Scope (unchanged)
- **plan_id**: `2026-06-08-v1.38-multi-chapter-selection-status`
- **Review range / Diff basis**: `merge-base(3f72b085, HEAD)..HEAD` on `iteration/v1.38` (covers initial feature commit `ffeb0adc` AND the fix-wave commits `f5c8ecc4` and `2dc2c892`, all merged via `2abbaa1a` and `d882a640`).
- **Working branch (verified)**: `iteration/v1.38`
- **Review cwd**: `/Users/bibi/workspace/organizations/42ch/nexus`
- **Reviewer focus**: architecture coherence and maintainability risk.

### F-001 — Re-verdict
- Status: ✅ Resolved
- Evidence: `f5c8ecc4`; `crates/nexus-local-db/src/work_chapters.rs:512-530` now uses one `SELECT MIN(chapter) ... status IN ('not_started', 'outlined', 'draft')` query; `test_next_chapter_resumes_draft` (`:1102-1157`), `test_next_chapter_outlined_not_skipped` (`:1162-1206`), and `test_next_chapter_skips_finalized` (`:1211-1256`) assert lowest-active-chapter semantics. `cargo test -p nexus-local-db work_chapters -- --test-threads=1` passed: 19 passed, 0 failed.
- Notes: The prior priority-tier selection is gone; draft/outlined rows are resumed by lowest chapter number before advancing to later active rows.

### F-002 — Re-verdict
- Status: ✅ Resolved
- Evidence: `f5c8ecc4`; `crates/nexus-local-db/src/work_chapters.rs:552-573` selects `work_profile` and keeps the `status == "completed"` early exit only when `work_profile != "novel"`; `test_completion_novel_profile_needs_full_check` (`:1386-1461`) verifies novel-profile full-check behavior including `status='completed'` with a draft row returning false. `cargo test -p nexus-local-db work_chapters -- --test-threads=1` passed: 19 passed, 0 failed.
- Notes: Novel-profile Works now always fall through to the §6.1 completion criteria; the shortcut remains limited to non-novel backward compatibility.

### W-1 — Re-verdict
- Status: ✅ Resolved
- Evidence: `2dc2c892`; `crates/nexus-local-db/migrations/202606080003_work_chapters_composite_index.sql:4-5` adds `CREATE INDEX IF NOT EXISTS work_chapters_by_work_status_chapter ON work_chapters(work_id, status, chapter)`. Review-range file check shows no `.sqlx/` deltas because the changed lookup uses runtime `sqlx::query` and the fix-wave W-1 commit is migration-only; `SQLX_OFFLINE=true cargo clippy -p nexus-local-db -p nexus-orchestration -p nexus-daemon-runtime -p nexus42 -- -D warnings` passed cleanly.
- Notes: The query pattern now has the intended composite index; offline clippy confirms committed SQLx metadata remains sufficient for compile-time checked queries.

### Re-verdict Summary

| Finding | Original Severity | Status |
|---------|-------------------|--------|
| F-001 | Critical | resolved |
| F-002 | Warning | resolved |
| W-1 | Warning | resolved |

### Updated Verdict

**Verdict**: Approve

Rationale: All three targeted fix-now findings are resolved in the fix wave, with targeted chapter tests passing and the required offline clippy gate clean. No remaining Critical or mandatory Warning issue blocks this targeted re-review; the pre-existing F-003 suggestion remains non-blocking and outside this fix-now scope.

Updated Severity Summary:
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 1 |
