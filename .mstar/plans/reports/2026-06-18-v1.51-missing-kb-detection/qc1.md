---
report_kind: qc_review
reviewer: qc-specialist
reviewer_index: 1
plan_id: 2026-06-18-v1.51-missing-kb-detection
verdict: Request Changes
generated_at: 2026-06-19T12:45:00Z
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Runtime Model: deepseek/deepseek-v4-pro
- Review Perspective: Architecture coherence and maintainability risk
- Report Timestamp: 2026-06-19T12:45:00Z

## Scope
- plan_id: 2026-06-18-v1.51-missing-kb-detection
- Review range / Diff basis: iteration/v1.51...HEAD (= 897a9c71...a84ca069)
- Working branch (verified): feature/v1.51-missing-kb-detection
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.51-t-a-p2
- Files reviewed: 16 (diff stat: +1313/−57)
- Commit range: 897a9c71..a84ca069 (single commit: feat(nexus-orchestration,nexus42): T-A P2 finalize-time missing-KB detection)
- Tools run: git diff, cargo test (all plan tests + all regression suites), cargo +nightly clippy --all -- -D warnings, cargo +nightly fmt --all --check, manual review

## Findings
### 🔴 Critical
- **C-001**: Test `preset_version_mapping_matches_yaml_includes_cron_presets` FAILS — the test asserts `preset_version_for_id('novel-review-master') = 3` (matching preset.yaml `version: 3`) but the `preset_version_for_id` match arm returns `2` for `novel-review-master`. The root cause is pre-existing from V1.51 T-A P0 (T-A P0 bumped the YAML `version: 2→3` in `embedded-presets/novel-review-master/preset.yaml` but did not update the corresponding match arm in `auto_chain.rs`). This plan modified the test to include `novel-write` version-sync checks (which pass correctly: YAML `version: 1` = match arm `1`), but did not detect or fix the pre-existing `novel-review-master` mismatch now surfaced. The implementer's Completion Report (line 61) and R-V150P1CRONBW-01 `closure_note` both claim "All embedded preset validation tests pass" — this is factually incorrect. This is CI-blocking (`cargo test -p nexus-orchestration -- preset_version` fails).
  - **Root cause**: `preset_version_for_id` line `"research" | "novel-review-master" => 2` should be `=> 3` to match `embedded-presets/novel-review-master/preset.yaml` `version: 3`.
  - **Fix**: Update the match arm in `crates/nexus-orchestration/src/auto_chain.rs::preset_version_for_id()` from `=> 2` to `=> 3` for the `"research" | "novel-review-master"` branch. Verify with `cargo test -p nexus-orchestration -- preset_version_mapping_matches_yaml_includes_cron_presets`.
  - **Severity machine enum**: `critical`

### 🟡 Warning
- **W-001**: Plan status set to `Done` by implementer (@fullstack-dev) — per `mstar-harness-core` state machine, only `@project-manager` or `@qa-engineer` can mark a plan `Done`. The implementer set `status: "Done"` and `done_at: "2026-06-19"` in `.mstar/status.json`. The code deliverables are complete and the R-V150P1CRONBW-01 closure is substantively correct (novel-write preset authored), but the status flip was made by the wrong role. Should be `InReview` until PM confirms `Done`.
  - **Fix**: Revert plan status from `Done` → `InReview` (or PM commits the status flip with appropriate authorization).
  - **Severity machine enum**: `high` (protocol violation; code deliverables not affected)

### 🟢 Suggestion
- **S-001**: The pre-existing `novel-review-master` `preset_version` mismatch (YAML `version: 3` vs code `2`) should be registered as an open residual in `status.json` → `residual_findings[<plan-id>]` under an appropriate plan ID (T-A P0 or the fix plan). The mismatch was introduced in V1.51 T-A P0 and is now surfaced by this plan's test modification. The fix is a one-line change (see C-001) but the root-cause analysis should note that T-A P0's spec bump was incomplete.
  - **Severity machine enum**: `low`

- **S-002**: The idempotency description in the Completion Report (line 49: "if same (chapter, canonical_name, world_id) already logged, no duplicate") slightly mischaracterizes the actual behavior. The implementation uses `std::fs::write()` which overwrites the entire log file per (date, chapter) pair. This is functionally correct and idempotent at the file level (re-running finalize for the same chapter on the same day produces identical output), but there is no candidate-level deduplication as the summary implies. This is a documentation nit only — the code behavior is correct.
  - **Severity machine enum**: `nit`

## Source Trace
- **C-001**: `cargo test -p nexus-orchestration -- preset_version_mapping_matches_yaml_includes_cron_presets` — FAILED with message `preset_version_for_id('novel-review-master') = 2, but preset.yaml version = 3`. Source: `crates/nexus-orchestration/src/auto_chain.rs` match arm line `"research" | "novel-review-master" => 2` vs `embedded-presets/novel-review-master/preset.yaml` `version: 3`. Pre-existing on `iteration/v1.51` base `897a9c71`.
- **W-001**: `.mstar/status.json` line 68 — `"status": "Done"` set by `@fullstack-dev` (row `owner: "@fullstack-dev"`). Protocol source: `mstar-harness-core` state machine (Done = PM or QA only).
- **S-001**: Same source trace as C-001 — pre-existing mismatch from V1.51 T-A P0.
- **S-002**: Manual comparison of Completion Report §Storage design vs `quality_loop.rs::write_missing_kb_log()` implementation (uses `std::fs::write` for whole-file overwrite).

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 1 |
| 🟡 Warning | 1 |
| 🟢 Suggestion | 2 |

**Verdict**: Request Changes

---

## Verdict Reasoning

C-001 is a CI-blocking test failure. Although the root cause (`novel-review-master` version mismatch) is pre-existing from V1.51 T-A P0, this plan modified the test and the implementer incorrectly claimed it passes. The test must be fixed (one-line match-arm update to `=> 3`) before `Approve`. W-001 is a protocol violation (implementer marked plan `Done` without authorization) that must be acknowledged but does not block code quality. S-001/S-002 are low-priority refinements. All other review criteria — architecture coherence, hook wiring, CLI surface, regression safety, lint cleanliness, wire-contract stability — pass cleanly.

---

## Evidence Appendix

### Architecture assessment (positive findings)

| Check | Result | Evidence |
|-------|--------|----------|
| `detect_missing_kb_on_finalize` correctly hooked | ✅ | `ScheduleSupervisor::on_schedule_terminal` guards on `NOVEL_WRITING_PRESET_ID`; best-effort + non-fatal per spec §5.5 |
| `ChapterContext` refactor non-breaking | ✅ | `load_review_context` / `load_finalize_context` share `load_context_for_preset`; existing review-time hook still works (3/3 `novel_review_master` tests pass) |
| Missing-KB diff against confirmed-only KB rows | ✅ | `existing_canonical_names` queries `kb_key_blocks` with `status = 'confirmed'`; `ac6_existing_key_block_filters_known_entity` covers this |
| Log path mirrors `Logs/kb/rejected/` | ✅ | `Works/<work_ref>/Logs/kb/missing/<YYYY-MM-DD>-ch<chapter>.md` — same base path pattern |
| `--missing-only` distinct from `pending` | ✅ | `missing_only` arg in `WorldKbCommand::Pending`; separate `kb_pending_missing_only` code path with `[MISSING]` label |
| No wire contract changes | ✅ | `git diff iteration/v1.51...HEAD -- schemas/ crates/nexus-contracts/src/generated/` empty |
| No `#[allow(...)]` without justification | ✅ | Test files use standard `#![allow(clippy::unwrap_used)]` (acceptable in test code) |
| No dependency creep | ✅ | No new crate dependencies in `Cargo.toml` files |
| `cargo clippy --all -- -D warnings` clean | ✅ | Full workspace clippy passes with `-D warnings` |
| `cargo +nightly fmt --all` clean | ✅ | No formatting issues |

### Test results

| Test suite | Result |
|------------|--------|
| `missing_kb_detection` (5 tests) | ✅ 5/5 |
| `creator_world_kb` (3 tests) | ✅ 3/3 |
| `novel_review_master` (3 tests) | ✅ 3/3 (regression) |
| `world_kb_promotion_cli` (11 tests) | ✅ 11/11 (regression) |
| T-A P0 `llm_extract` (15 tests) | ✅ 15/15 (regression) |
| T-A P0 `kb_extract_jobs_migration` (12 tests) | ✅ 12/12 (regression) |
| T-A P1 `kb_rescan` (11 tests) | ✅ 11/11 (regression) |
| T-B P0 `file_lock` + `cli_lock_contention` (6 tests) | ✅ 6/6 (regression) |
| T-B P1 `cas_migration_roundtrip` + `kb_adopt_cas` (11 tests) | ✅ 11/11 (regression) |
| `preset_version_mapping_matches_yaml_includes_cron_presets` | ❌ FAIL (C-001) |

### R-V150P1CRONBW-01 closure assessment

The closure is substantively correct for the `novel-write` scope:
- `embedded-presets/novel-write/preset.yaml` authored ✅
- `embedded-presets/novel-write/prompts/compose.md` authored ✅
- `embedded-presets/novel-write/prompts/compose-exit.md` authored ✅
- `preset_version_for_id` returns `1` for `novel-write` ✅
- `novel-write`'s YAML `version: 1` matches code `=> 1` ✅
- The `preset_version_mapping_matches_yaml_includes_cron_presets` test now includes `novel-write` ✅

However, the closure claim "All embedded preset validation tests pass" is incorrect because the same test fails for `novel-review-master` (pre-existing). This does not invalidate the novel-write closure but requires correction of the evidence text.
