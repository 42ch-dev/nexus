---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: "2026-06-08-v1.38-novel-writing-parameterization"
verdict: "Approve"
generated_at: "2026-06-08"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: xai/grok-build-0.1
- Review Perspective: Security and correctness risk (path injection, template injection, DB vs. filesystem SSOT, slug validation, chapter label edge cases, race conditions, backward compat, quarantined artifacts, error handling on novel completion, P0/deferred boundary).
- Report Timestamp: 2026-06-08

## Scope
- plan_id: "2026-06-08-v1.38-novel-writing-parameterization"
- Review range / Diff basis: merge-base(8e58890a, HEAD)..HEAD on iteration/v1.38 (commit ad455ec5 merge(v1.38-p1) brings in 5 feature commits).
- Working branch (verified): iteration/v1.38
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 10
- Commit range: 8e58890aac05ac9ffd8d5aefffdcb4d565a21915..ad455ec5498465043642b7fd80d749ab874c42cf (5 feature commits + merge)
- Tools run: git rev-parse, git branch, git merge-base, git diff --stat, git diff --name-status, git show --stat (all 6 commits), Read (plan, spec, stage_gates.rs, preset.yaml, prompts, CLI run.rs, work_chapters.rs), Grep (stage_advance / WorkFields / chapter paths / draft-body|draft-intro / path safety), cargo clippy -p nexus-local-db -p nexus-orchestration -p nexus-daemon-runtime -p nexus42 -- -D warnings (exit 0, clean).

## Acceptance Criteria Review

| AC | Statement (from plan §6) | Status | Evidence |
|----|---------------------------|--------|----------|
| AC1 | `novel-writing` can run for selected chapter 2 and writes/reads `ch02` outline/body paths. | Pass (in scope) | `stage_gates.rs:624-646` tests for chapter 2 + 10; `build_preset_input` serializes `chapter_label="02"`, `outline_path`/`body_path` from DB; CLI extraction from `WorkApiDto.chapters[]` (run.rs:990-1011); preset.yaml v6 + templates consume the vars. |
| AC2 | No implementation creates or requires separate `novel-writing-chapter-N` presets. | Pass | Single `novel-writing` preset (version 6); all chapter context flows via `preset.input.*`; no per-chapter preset duplication in diff or tests. |
| AC3 | Chapter 1 behavior remains compatible through the same selected-chapter input path. | Pass | `build_preset_input` tests (stage_gates.rs:669-674) assert chapter 1 defaults produce valid `ch01` paths; preset gates + templates treat chapter 1 as normal selected chapter. |
| AC4 | Prompt rendering no longer relies on hard-coded chapter 1 values where selected chapter context is available. | Pass | `outline-chapter.md` / `draft-chapter.md` replace `ch0{{chapter}}` literals with `{{outline_path}}` / `{{body_path}}` / `{{chapter_label}}` / `{{slug}}`; preset.yaml state enter actions wire the new vars. |
| AC5 | Finalizing a chapter does not automatically enqueue the next chapter. | Pass (pre-existing; not regressed) | Diff does not touch auto-chain / `next_chapter` enqueue logic (DF-53 remains deferred per plan §8). `stage_advance` for "produce" when `next_chapter=None` still creates schedule (latent, see W-1), but this is P0 behavior unchanged by parameterization. |
| AC6 | Tests cover chapter 2+ rendering/path behavior and one-chapter compatibility. | Pass | New tests in stage_gates.rs:624-728 (chapter2, chapter10 label="10", chapter1 compat, None fields omitted, produce schedule includes context); e2e_novel_writing.rs updated to seed P1 vars; fl_e_chain_demo.rs seeds the 4 new fields. |

## Findings

### 🔴 Critical
None.

### 🟡 Warning
- **W-1 (latent, pre-existing, low impact)**: When `next_chapter` returns `None` (novel completion per P0 selection), CLI `stage_advance` for target "produce" still succeeds, builds `WorkFields` with all chapter fields `None`, and creates a `novel-writing` schedule with empty `preset.input.chapter*` / paths. The preset gates (work_profile=novel, work_ref, intake complete, previous_preset) do not reject it. This makes the "no next chapter" terminal state more visible in the new parameterized world, but does not change the completion gate or auto-enqueue behavior (DF-53 deferred). Not introduced by this diff; surfaced by the new fields. Recommendation: either reject "produce" advance when no next_chapter (at gate or CLI), or document that the schedule will be created but the writer prompt will see missing chapter context and should no-op or surface completion. Severity: Warning (correctness for terminal novel case; not a security regression).

### 🟢 Suggestion
- **S-1**: `chapter_label` formatting is `format!("{ch_num:02}")`. For chapter ≥100 this yields "100", "101" (not fixed-width "0100"). Tests explicitly assert chapter 10 → "10" (not "010"). If future novels require fixed-width labels or right-aligned padding, this should be documented in the spec (§4.5.6) or made a preset input option. Current scope accepts the 2-digit zero-pad for 1-99; no correctness break for P1.
- **S-2**: CLI extraction of the selected chapter row (run.rs:993-996) does a linear `chapters.iter().find(...)`. For novels with hundreds of chapters this is negligible, but a small map or comment noting "O(n) scan over chapters array (n typically <100)" would be clearer. Not a performance or correctness risk.
- **S-3 (defense-in-depth)**: The `body_path` / `outline_path` values that reach the prompt are trusted because they originate from `work_chapters` (seeded with sanitized `work_ref` in `novel_scaffold` + `seed_chapters`). There is no runtime re-validation at prompt render time that the path still lives under `Works/<work_ref>/`. The engine never performs filesystem writes from these strings (they are output instructions to the LLM), and `assert_template_file_safe` / `validate_path_safety` / `canonicalize_within` protect preset asset paths, not runtime chapter paths. Adding a belt-and-suspenders check in `build_preset_input` or a preset gate (if desired) would be pure defense-in-depth; current trust boundary is acceptable.
- **S-4**: Quarantined `draft-body.md` / `draft-intro.md` in `prompts/_deprecated/` are confirmed unused by the current `novel-writing` preset (grep found zero references). User-installed presets could in theory reference them, but the quarantine + preset version bump + test file-count update is the documented intent. No breakage for shipped embedded preset.

## Source Trace
- **W-1 (completion schedule with empty chapter fields)**: manual reasoning + code read of run.rs:983-1011 (next_chapter + chapters lookup → default None), stage_gates.rs:689-699 (None fields omitted from input), preset.yaml:32-58 (gates do not check chapter presence), plan §8 DF-53 and AC5.
- **Path sourcing / injection surface**: stage_gates.rs:72-125 (`build_preset_input` only inserts strings from WorkFields), run.rs:988-1024 (extraction from `WorkApiDto.chapters[]` snapshot), work_chapters.rs:60-62 + 107-109 (seed constructs from sanitized work_ref), preset loader:351 (`assert_template_file_safe`), preset_gates.rs:131 (`canonicalize_within`), novel_scaffold_sanitize.rs:63 (work_ref validation).
- **chapter_label zero-pad + ch≥100 edge**: stage_gates.rs:992 (format!("{ch_num:02}")), tests 655-658 (chapter 10 → "10"), 624-646 (chapter 2), spec §4.5.6.
- **Quarantine + version bump**: preset.yaml:17 (version: 6), mod.rs:232 (assert), preset/mod.rs:232 (test), git mv of draft-body/intro to _deprecated/.
- **No P0 / deferred boundary**: git diff --name-status shows only orchestration preset wiring, CLI extraction, prompt templates, tests; work_chapters.rs changes are in prior P0 plan; no auto-chain, World KB, quality loop, multi-volume, or platform publish touched.
- **CI gate**: `cargo clippy -p nexus-local-db -p nexus-orchestration -p nexus-daemon-runtime -p nexus42 -- -D warnings` → exit 0, clean (dev profile, 0.20s).

## Diff Scope Check
- **P0 boundary**: PASS — diff does not touch P0-only files (selection logic, `next_chapter` query, `is_work_completed`, daemon works enrichment, or the composite index migration). Those were delivered and QC'd in the prior P0 plan.
- **Deferred boundary (DF-53/63/64-67, multi-volume, platform publish, auto-chain, quality loop, World KB)**: PASS — none of these are implemented or touched in this diff. Plan §8 explicitly lists them as deferred.
- **Files in scope (10)**: embedded-presets/novel-writing/preset.yaml, prompts/outline-chapter.md, prompts/draft-chapter.md, prompts/_deprecated/{draft-body,draft-intro}.md, crates/nexus-orchestration/src/preset/mod.rs, src/stage_gates.rs, tests/e2e_novel_writing.rs, tests/fl_e_chain_demo.rs, crates/nexus42/src/commands/creator/run.rs. Matches `git diff --stat`.

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 1 (latent, pre-existing, low impact; not a regression from this change) |
| 🟢 Suggestion | 4 |

**Verdict**: Approve

## Revalidation Notes (if targeted re-review)
N/A — initial wave.
