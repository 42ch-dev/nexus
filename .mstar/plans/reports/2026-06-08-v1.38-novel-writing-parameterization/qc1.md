---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-06-08-v1.38-novel-writing-parameterization"
verdict: "Approve"
generated_at: "2026-06-08"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Runtime Model: volcengine-plan/glm-5.1
- Review Perspective: Architecture coherence and maintainability risk
- Report Timestamp: 2026-06-08T18:30:00+08:00

## Scope
- plan_id: `2026-06-08-v1.38-novel-writing-parameterization`
- Review range / Diff basis: `merge-base(8e58890a, HEAD)..HEAD` on `iteration/v1.38`
- Working branch (verified): `iteration/v1.38`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus`
- Files reviewed: 10
- Commit range: `8e58890a..ad455ec5` (5 feature commits + 1 merge)
- Tools run: `git diff`, `git show`, `cargo clippy -p nexus-local-db -p nexus-orchestration -p nexus-daemon-runtime -p nexus42 -- -D warnings` (PASS), `cargo test -p nexus-orchestration stage_gates` (37/37 PASS), `git grep` for `ch0{{chapter}}`, version assertions, P0 boundary files, deferred-scope items

## Acceptance Criteria Review

| AC | Description | Status | Evidence |
|----|-------------|--------|----------|
| AC1 | `novel-writing` can run for selected chapter 2 and writes/reads `ch02` outline/body paths | ✅ PASS | `build_preset_input_chapter2_includes_all_context_fields` test confirms ch02 paths; `schedule_for_produce_chapter2_includes_all_context` proves schedule round-trip |
| AC2 | No implementation creates or requires separate `novel-writing-chapter-N` presets | ✅ PASS | `git grep` for `novel-writing-chapter` returns zero hits; `embedded-presets/` contains single `novel-writing/` |
| AC3 | Chapter 1 behavior remains compatible through the same selected-chapter input path | ✅ PASS | `build_preset_input_chapter1_compat` test; template defaults (`default: "01"`, `default: "ch01"`) preserve ch01 rendering |
| AC4 | Prompt rendering no longer relies on hard-coded chapter 1 values where selected chapter context is available | ✅ PASS | `git grep 'ch0{{chapter}}'` in current source files returns zero hits (only in `.mstar/` historical QC reports) |
| AC5 | Finalizing a chapter does not automatically enqueue the next chapter | ✅ PASS | No auto-chain logic added; `preset.yaml` `done` state is terminal; no `on_complete` trigger |
| AC6 | Tests cover chapter 2+ rendering/path behavior and one-chapter compatibility | ✅ PASS | 5 new tests: chapter2 context, chapter10 label, chapter1 compat, None-omission, produce-schedule chapter2 |

## Findings

### 🔴 Critical

(None.)

### 🟡 Warning

**W-1: CLI `stage_advance` silently degrades to `None` chapter context when `chapters[]` is missing or chapter row absent — `outline_path` and `body_path` are `required: true` in templates but not guaranteed at render time.**

- **File**: `crates/nexus42/src/commands/creator/run.rs:990-1011`
- **Issue**: The `.and_then()` chain returns `None` (via `unwrap_or_default()`) for all four new fields when either (a) `resp.get("chapters")` is missing / not an array, or (b) no chapter row matches `next_chapter`. In that case, `outline_path` and `body_path` are `None` and omitted from `build_preset_input`. Both prompt templates declare these as `required: true`, so template rendering would fail or produce a broken prompt with empty path variables.
- **Impact**: Any daemon response that omits the `chapters` array (e.g., non-novel work profile, pre-P0 work, or a schema regression) silently produces a schedule with missing required template variables. This was not the case before P1 because the old templates used `ch0{{chapter}}` which always resolved from the numeric `chapter` field.
- **Fix recommendation**: Either (a) validate that `outline_path` and `body_path` are present before proceeding with schedule creation and return a user-facing error, or (b) change template vars to `required: false` with sensible defaults (but this defeats the purpose of DB-driven paths). Option (a) is architecturally cleaner — fail fast at the CLI boundary rather than at template render time.
- **Confidence**: High

**W-2: `chapter_label` formatting is computed independently in CLI (`run.rs`) and test helper (`stage_gates.rs`) — no shared formatting function.**

- **File**: `crates/nexus42/src/commands/creator/run.rs:992` vs `crates/nexus-orchestration/src/stage_gates.rs:616`
- **Issue**: Both locations use `format!("{ch_num:02}")` to compute the zero-padded chapter label, but there is no shared constant or function. If the label format ever changes (e.g., to non-zero-padded or a different width), both sites must be updated in lockstep with no compiler enforcement.
- **Impact**: Low likelihood of divergence today, but violates the "single truth source" principle the repo emphasizes for DTOs and wire types. The formatting logic is a domain invariant that should be centralized.
- **Fix recommendation**: Extract a `fn chapter_label(chapter: i32) -> String` helper (or a `const` format string) in `stage_gates.rs` and import it in `run.rs`, or place it in a shared utility. This makes the `chapter_label` computation a single-authoritative site.
- **Confidence**: High

### 🟢 Suggestion

**S-1: Frontmatter field documentation was removed from `draft-chapter.md` without replacement.**

- **File**: `crates/nexus-orchestration/embedded-presets/novel-writing/prompts/draft-chapter.md`
- **Issue**: The old template contained a bulleted list explaining each frontmatter field (`title`, `chapter`, `status`, `word_count`, `world_refs`). The new template drops this entirely. While the frontmatter example block remains, the LLM may produce inconsistent frontmatter without explicit field descriptions — especially `status` must be `draft` and `world_refs` should be `[]` for worldless works.
- **Fix recommendation**: Consider restoring a compact 1-2 line summary of required frontmatter fields, or at minimum a comment noting `status: draft` is required on initial creation.
- **Confidence**: Medium

**S-2: `_deprecated/` prompt files are still embedded in the binary via `include_dir!`.**

- **File**: `crates/nexus-orchestration/embedded-presets/novel-writing/prompts/_deprecated/`
- **Issue**: The `_deprecated/` subdirectory containing `draft-body.md` and `draft-intro.md` is still included in the binary at compile time via the `include_dir!` macro. While the preset test correctly adjusts the file count from 14 to 12 (counting only non-deprecated files), the deprecated files still occupy binary space and are discoverable via the embedded filesystem API.
- **Fix recommendation**: In a future cleanup, either (a) move `_deprecated/` outside the `embedded-presets/` tree entirely (e.g., to a `docs/` or `archived/` directory), or (b) configure `include_dir!` to exclude `_deprecated/` patterns. Not blocking for this plan — the files are not referenced by `preset.yaml` — but should be tracked as a minor tech debt item.
- **Confidence**: High

**S-3: `outline_path` and `body_path` are declared `required: true` in prompt templates but have no defaults — chapter1-only callers that skip the CLI `stage_advance` path have no fallback.**

- **File**: `crates/nexus-orchestration/embedded-presets/novel-writing/prompts/outline-chapter.md:9`, `draft-chapter.md:9-10`
- **Issue**: If a caller (e.g., e2e test, daemon, or future API) constructs `preset.input` directly without going through the CLI's `stage_advance` (which populates these fields from DB), the `required: true` constraint means template rendering will fail. The e2e test `seed_novel_writing_preset_input()` now correctly sets all fields, so this is not a current bug — but the template contract is stricter than before P1.
- **Fix recommendation**: Document the expanded `preset.input` contract clearly in `preset.yaml` header comments (already partially done via the input variable list at the top). Consider adding a runtime validation in the schedule builder that warns when required template vars are missing from `preset.input`.
- **Confidence**: Medium

## Source Trace

| Finding ID | Source Type | Source Reference | Confidence |
|------------|-------------|------------------|------------|
| W-1 | manual-reasoning + git-diff | `run.rs:990-1011`, template `required: true` declarations | High |
| W-2 | git-diff | `run.rs:992` vs `stage_gates.rs:616` | High |
| S-1 | git-diff | `draft-chapter.md` old vs new | Medium |
| S-2 | manual-reasoning + filesystem | `_deprecated/` dir in `embedded-presets/` | High |
| S-3 | manual-reasoning + git-diff | Template var declarations, `preset.yaml` input list | Medium |

## Diff Scope Check

| Boundary | Status | Evidence |
|----------|--------|----------|
| P0 files (`work_chapters.rs`, `novel_chapter_transition.rs`, `WorkApiDto` enrichment, `is_work_completed`) | ✅ NOT TOUCHED | `git diff 8e58890a..HEAD --name-only` contains no P0-only file paths |
| Deferred scope (auto-chain, World KB, quality loop, multi-volume PK, platform publish, multi-work switch, selection pool) | ✅ NOT TOUCHED | `git grep` for deferred keywords in diff files returns only pre-existing auto-chain comments (not part of this diff); no new logic added |
| P0 `chapter` field | ✅ PRESERVED | `WorkFields.chapter` field and `build_preset_input` chapter serialization unchanged; new fields are additive `Option<…>` |

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 2 |
| 🟢 Suggestion | 3 |

**Verdict**: Request Changes

Two Warning-level findings remain unresolved:

1. **W-1** (CLI silent degradation): When the daemon response lacks a `chapters` array or the selected chapter row is absent, `outline_path` and `body_path` become `None` and are silently omitted from preset input. Since both prompt templates declare these as `required: true`, template rendering will fail at runtime with no user-facing explanation. This is a regression from the pre-P1 state where `ch0{{chapter}}` always resolved from the numeric `chapter` field.

2. **W-2** (duplicated label formatting): `chapter_label` computation is duplicated across CLI and orchestration with no shared source. If the format changes, both sites must be updated with no compile-time enforcement.

## Revalidation (targeted re-review)

### Reviewer Metadata
- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Runtime Model: volcengine-plan/deepseek-v4-pro
- Review Perspective: Architecture coherence and maintainability risk (targeted re-review)
- Report Timestamp: 2026-06-08
- Original verdict: Request Changes (initial tri-review)
- Targeted findings to re-verify: W-1 (Warning), W-2 (Warning)

### W-1 — Re-verdict
- Status: ✅ Resolved
- Evidence:
  - `validate_produce_chapter_context()` defined at `crates/nexus42/src/commands/creator/run.rs:890-909` (commit `612b81d9`).
  - Called at `run.rs:1042-1049` after `.and_then()` chain but before `WorkFields` construction.
  - Error message: `"novel-writing schedule requires chapter context (outline_path, body_path).\nThe daemon response is missing chapters[] or the selected chapter row.\nHint: re-run \`nexus42 creator run status {work_id}\` to inspect,\nor re-seed the work via \`nexus42 creator run start --init-preset novel-project-init.\""` — includes remediation hint.
  - Fires only when `target_stage=="produce" && next_chapter.is_some() && outline_path.is_none() && body_path.is_none()`.
  - Does NOT fire when `next_chapter=None` (novel-completion) or for non-produce stages.
  - 4 new tests (commit `612b81d9`): `stage_advance_produce_chapter_missing_chapter_array_returns_error`, `validate_produce_ok_when_chapter_context_present`, `validate_skips_when_next_chapter_is_none`, `validate_skips_for_non_produce_stage`.
  - `cargo test -p nexus42 --lib -- stage_advance_produce validate_produce validate_skips`: **4/4 PASS**.
  - `SQLX_OFFLINE=true cargo clippy -p nexus-local-db -p nexus-orchestration -p nexus-daemon-runtime -p nexus42 -- -D warnings`: **PASS**.
- Notes: Fix is architecturally clean — fail-fast at CLI boundary with actionable error before template rendering. `next_chapter=None` (novel-completion) is correctly preserved as a latent residual (R-V138P1-01).

### W-2 — Re-verdict
- Status: ✅ Resolved
- Evidence:
  - `pub fn chapter_label(chapter: i32) -> String` defined at `crates/nexus-orchestration/src/stage_gates.rs:24-26` (commit `ba912fe1`).
  - Used at `run.rs:1021` via `stage_gates::chapter_label(ch_num)` (imported at `run.rs:15`).
  - Used at `stage_gates.rs:630` in test helper `chapter_work_fields()`.
  - No remaining `format!("{:02}")` for chapter labels in `run.rs` (confirmed via grep).
  - Unit test `chapter_label_formats_zero_padded_for_1_to_99` confirms 1→"01", 9→"09", 10→"10", 99→"99", 100→"100".
  - `cargo test -p nexus-orchestration --lib stage_gates`: **38/38 PASS** (includes new test).
  - `SQLX_OFFLINE=true cargo clippy -p nexus-local-db -p nexus-orchestration -p nexus-daemon-runtime -p nexus42 -- -D warnings`: **PASS**.
- Notes: Single-authoritative formatting site in `stage_gates.rs`; both call sites import from it. Doc comment documents the format contract clearly.

### Re-verdict Summary

| Finding | Original Severity | Status |
|---------|-------------------|--------|
| W-1 | Warning | ✅ Resolved |
| W-2 | Warning | ✅ Resolved |

### Updated Verdict

**Verdict**: Approve

Rationale: Both targeted findings (W-1, W-2) are fully resolved with clean architectural fixes, adequate test coverage, and passing clippy. No new issues introduced. The original S-1, S-2, S-3 suggestions remain non-blocking and are unchanged from the initial review.

Updated Severity Summary:
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 3 |
