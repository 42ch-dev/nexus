---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-06-13-v1.44-review-master-cli-surface"
verdict: "Request Changes"
generated_at: "2026-06-13"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Runtime Model: volcengine-plan/deepseek-v4-pro
- Review Perspective: Architecture coherence and maintainability risk
- Report Timestamp: 2026-06-13T11:50:00+08:00

## Scope
- plan_id: 2026-06-13-v1.44-review-master-cli-surface
- Review range / Diff basis: 9d471bdc..c54b1aa6
- Working branch (verified): iteration/v1.44
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 7
- Commit range: a7b70ebf..c54b1aa6 (3 feature commits + 1 merge)
- Tools run: cargo clippy -p nexus42 -- -D warnings, cargo test -p nexus42 --test review_master_cli, cargo +nightly fmt --all --check

## Findings

### 🔴 Critical
(None)

### 🟡 Warning

#### W-1: `cli-command-ia.md` not updated despite plan T5 listing it

Plan T5 explicitly lists `cli-command-ia.md` as a target for spec amendments alongside `cli-spec.md`, `novel-workflow-profile.md`, and `novel-quality-loop.md`. However, `cli-command-ia.md` has zero changes in the diff range — it does not mention `review-master` or `audit-chapter` at all. While `cli-spec.md` §6.2D is the authoritative per-command detail (and was correctly updated), `cli-command-ia.md` is the IA rationale Master spec that documents the creator hub principles and command group topology. Omitting the new subcommand from this document creates a spec drift between the IA rationale and the shipped CLI surface.

**Fix**: Add `review-master` to `cli-command-ia.md` §3 (Creator hub principles) or a new row in the command table, documenting its role as the master-decision review entry point distinct from `stage advance --stage review`. Alternatively, if the plan's T5 listing of `cli-command-ia.md` was erroneous, update the plan to reflect the actual scope and document the rationale.

#### W-2: `novel-author-experience.md` not updated despite being a plan primary spec

The plan lists `novel-author-experience.md` as a primary spec, but the file has zero changes in the diff range and does not mention `review-master`. This is a Feature line spec for the author experience; adding a new author-facing CLI command without updating the author experience spec is a maintainability gap — future readers of the spec won't discover the `review-master` command from the author-experience document.

**Fix**: Either update `novel-author-experience.md` to reference the `review-master` command in the appropriate section, or remove it from the plan's primary specs list with a note explaining why it was intentionally excluded from this scope.

#### W-3: Duplicate Work fetch when both `--finding-id` and `--auto-schedule` are used

`handle_review_master` fetches the Work from the daemon API twice when both flags are active: once at line 849 (for `--finding-id` context assembly) and again at line 963 (for `--auto-schedule` context assembly). The two code paths construct slightly different `input` shapes — the `--finding-id` path includes `body_path` (derived from the specific finding's chapter), while the `--auto-schedule` path does not. This duplication creates a maintainability risk: future changes to the schedule input shape must be kept in sync across two near-identical blocks.

**Fix**: Extract a shared helper that fetches the Work once and returns the common context fields (`work_ref`, `topic`, `world_id`). Each enqueue path can then add its path-specific fields (`body_path` for `--finding-id`, `open_findings` for `--auto-schedule`).

#### W-4: `--auto-schedule` uses global stale count but work-scoped `master_findings`

The `--auto-schedule` path (line 948) queries the global `/v1/local/findings/stale` endpoint for the stale count, but then enqueues a schedule with `master_findings` scoped to the current Work (computed at line 777). If the global stale endpoint returns `stale_count > 0` but none of those stale findings belong to the current Work, the auto-schedule would still fire and enqueue a schedule with potentially empty or irrelevant `master_findings`. This is a logic gap: the stale check is global but the action is work-scoped.

**Fix**: Either scope the stale check to the current Work (e.g., `/v1/local/works/{work_id}/findings/stale`), or document that `--auto-schedule` is intentionally a "global trigger, work-scoped action" with the rationale that the author should review their work whenever any findings are stale anywhere.

### 🟢 Suggestion

#### S-1: Extract enqueue helpers to reduce `handle_review_master` size

The function is ~310 lines with a `#[allow(clippy::too_many_lines)]` suppression. Extracting the `--finding-id` enqueue logic and `--auto-schedule` enqueue logic into separate private helper functions would:
- Remove the clippy suppression
- Address W-3 (deduplicate Work fetch)
- Improve readability and testability of each enqueue path independently

#### S-2: `open_findings` serialized as JSON string in schedule input

The `open_findings` field in the schedule input is serialized via `serde_json::to_string()` (a JSON-encoded string) rather than as a native JSON array. This means the daemon-side preset handler must deserialize this string. If intentional (to pass structured data through a string-typed field), document the convention. If the `input` schema supports nested objects, consider passing it as a proper JSON array to avoid the double-serialization pattern.

#### S-3: Hardcoded API path strings

API paths like `/v1/local/works/{work_id}/findings?status=open&limit=50` and `/v1/local/findings/stale` are string-formatted inline. Consider centralizing these in a constants module or an API path builder to reduce duplication and make endpoint changes easier to audit.

#### S-4: `body_path` asymmetry between `--finding-id` and `--auto-schedule`

The `--finding-id` path includes `body_path` in the schedule input (derived from the finding's chapter), but the `--auto-schedule` path does not. This asymmetry may be intentional (auto-schedule covers all findings, not one chapter), but it is not documented. Add a comment explaining the rationale.

## Source Trace

| Finding ID | Source Type | Source Reference | Confidence |
|------------|-------------|------------------|------------|
| W-1 | git-diff | `git diff 9d471bdc..c54b1aa6 -- .mstar/knowledge/specs/cli-command-ia.md` (empty) | High |
| W-2 | git-diff | `git diff 9d471bdc..c54b1aa6 -- .mstar/knowledge/specs/novel-author-experience.md` (empty) | High |
| W-3 | manual-reasoning | `crates/nexus42/src/commands/creator/run.rs` lines 849, 963 | High |
| W-4 | manual-reasoning | `crates/nexus42/src/commands/creator/run.rs` lines 948–978 | Medium |
| S-1 | manual-reasoning | `crates/nexus42/src/commands/creator/run.rs` line 760 | Medium |
| S-2 | manual-reasoning | `crates/nexus42/src/commands/creator/run.rs` line 877 | Low |
| S-3 | manual-reasoning | `crates/nexus42/src/commands/creator/run.rs` lines 771, 949 | Low |
| S-4 | manual-reasoning | `crates/nexus42/src/commands/creator/run.rs` lines 862–898 vs 977–993 | Low |

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 4 |
| 🟢 Suggestion | 4 |

**Verdict**: Request Changes

### Architecture Coherence Assessment

The `ReviewMaster` variant integrates cleanly into the existing `RunCommand` enum — it follows the same pattern as `AuditChapter` (P0), uses the same `AddScheduleRequest` DTO from `nexus_contracts`, and dispatches through the same `handle_run` match arm. The merge commit `c54b1aa6` shows no conflict artifacts — `ReviewMaster` and `AuditChapter` are correctly ordered as adjacent enum variants, and both handler functions are cleanly separated with section comment dividers. The `cli-spec.md` §6.2D table row was correctly added without conflicting with P0's `audit-chapter` row.

### Maintainability Risk

The primary maintainability concerns are:
1. **Spec drift** (W-1, W-2): Two spec files listed in the plan were not updated, creating gaps between the documented architecture and the shipped CLI surface.
2. **Code duplication** (W-3): The `--finding-id` and `--auto-schedule` paths duplicate Work-fetch and input-construction logic, making future changes harder to keep in sync.
3. **Logic gap** (W-4): The `--auto-schedule` path's global stale check may trigger work-scoped actions for unrelated stale findings.

### Evidence

- `cargo clippy -p nexus42 -- -D warnings` — **PASS** (clean)
- `cargo test -p nexus42 --test review_master_cli` — **PASS** (5/5)
- `cargo +nightly fmt --all --check` — **PASS** (no diffs)
- `git log --oneline 9d471bdc..c54b1aa6` — 3 commits + 1 merge, as expected
