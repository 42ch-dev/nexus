---
report_kind: qc-review
reviewer: "@qc-specialist-3"
reviewer_index: 3
focus: performance-reliability
plan_id: 2026-06-10-v1.41-hygiene
verdict: Request Changes
generated_at: 2026-06-11T01:08:07+08:00
review_range: "merge-base: 55689706 → tip: f4d72a86"
working_branch_verified: iteration/v1.41
review_cwd_verified: /Users/bibi/workspace/organizations/42ch/nexus
files_reviewed: 9
tools_run: cargo clippy --all -D warnings, cargo +nightly fmt --all -- --check, cargo test -p nexus-creator-memory -p nexus-orchestration -p nexus-kb -p nexus-moment-context-assembly -p nexus-daemon-runtime, git log/diff/show on 5 P-last commits, manual source review
---

# Code Review Report — V1.41 P-last (qc3)

## Reviewer Metadata
- Reviewer: @qc-specialist-3
- Runtime Agent ID: qc-specialist-3
- Runtime Model: kimi-for-coding/k2p6
- Review Perspective: Performance and reliability risk
- Report Timestamp: 2026-06-11T01:08:07+08:00

## Scope
- plan_id: 2026-06-10-v1.41-hygiene
- Review range / Diff basis: merge-base: 556897061f625c53cd172e2bdb40d509dac61775 → tip: f4d72a86ef88215e9da6f2043fe2f873579f9311
- Working branch (verified): iteration/v1.41
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 9 (review.rs, works.rs, embedded_rules.rs, stage_gates.rs, store.rs, validation.rs, extract.md, world_context.rs, completion-report.md)
- Tools run: cargo clippy --all -D warnings (clean), cargo +nightly fmt --all -- --check (clean), cargo test -p nexus-creator-memory -p nexus-orchestration -p nexus-kb -p nexus-moment-context-assembly -p nexus-daemon-runtime (all pass), git log/diff/show on 5 P-last commits, manual source review

## Findings

### 🔴 Critical
- None.

### 🟡 Warning
- **W-1 (reliability — UTF-8 boundary panic in size guard)**: In `promote_to_long_term` (R-V133P4-06, `crates/nexus-creator-memory/src/review.rs:651`), the size guard truncates `raw_digest` with `&record.raw_digest[..MAX_DIGEST_BYTES]`. Since `raw_digest` is `String` (arbitrary UTF-8 from session content / LLM output), byte slicing `&str[..n]` panics if `n` is not a char boundary. The 256 KiB constant is arbitrary and will frequently cut mid-character for CJK or emoji content. The test uses pure ASCII (`"x".repeat(300 * 1024)`) and masks the bug. A panic in the memory promotion pipeline aborts the async task and leaves the session un-promoted — a runtime reliability regression in the fix itself.
  → Fix: Use `floor_char_boundary` (Rust 1.80+) or scan backward to the nearest `is_char_boundary` before slicing. Add a test with multi-byte content (e.g., 3-byte emoji or CJK) at the 256 KiB boundary. *(Note: qc2 independently identified the same issue as W-01, confirming the finding.)*

### 🟢 Suggestion
- **S-1 (performance/reliability — hardcoded MAX_DIGEST_BYTES)**: `MAX_DIGEST_BYTES` is a hardcoded `const` (256 KiB). Precedent exists in the codebase for env-driven operational limits (`NEXUS_CONTEXT_MAX_FILE_SIZE`, `NEXUS_DB_POOL_MAX_CONNECTIONS`, `NEXUS_DRIFT_LIMIT_MS`). For production flexibility and operator tunability, consider making this runtime-configurable (e.g., `NEXUS_MAX_DIGEST_BYTES`).
- **S-2 (reliability — YAML output validity)**: `to_yaml` (R-V140P2-S4) changed string fields from `{:?}` (Debug, quoted+escaped) to `{}` (Display, raw). While the commit states "LLM consumers handle both formats," the output is also returned via CLI (`crates/nexus42/src/commands/creator/run.rs:862`) where it may be piped to YAML parsers. Unescaped metacharacters (`:`, `"`, `#`, newlines) in user/LLM-controlled fields (`world_name`, `current_timeline`, `name`, `descriptor`) could produce invalid YAML. This is a reliability concern for downstream machine parsing.
  → Consider adding minimal YAML escaping/quoting for strings containing metacharacters, or switch to a serializer like `serde_yaml` if machine parsing is expected.
- **S-3 (reliability — waived residuals audit)**: The 13 waived-with-doc residuals were reviewed for changes in risk profile after P0/P1 changes (completion-lock + runtime-lock). None are newly concerning. The single-user local-first assumption continues to hold for all waived items (R-V133P3-04 DoS vector, R-V140P1-S3 concurrent-uniqueness race, R-V140P2-S1 linear scan, etc.).
- **S-4 (reliability — deferred residuals audit)**: The 7 deferred-to-V1.42 residuals were reviewed for safety-criticality. None are production safety risks: 3 are test-infrastructure only (R-V140P3-S1/S2/S3), 2 are ops-scoped (R-V140P0-S3 sqlx metadata, R-V140P4-INFRA cache refresh), 1 is benchmark (R-V140P1-S6), 1 is E2E harness (R-V140P2-S2). All appropriately deferred.
- **S-5 (reliability — excluded items verification)**: P-last excludes R-V140P4-W2 (PM-accepted waiver) and R-V140P4-INFRA (sqlx-cli) were verified untouched in the 5 P-last commits. Good.

## Source Trace
- Finding W-1: manual code review of `crates/nexus-creator-memory/src/review.rs:641-654` and test `promote_truncates_oversized_raw_digest` (lines 1069-1114). Confirmed `String` type of `raw_digest` at line 122. Verified Rust `str` indexing panic behavior via language docs (`str::index` panics on non-char-boundary).
- Finding S-1: grep for env-driven limits across `crates/` (`NEXUS_CONTEXT_MAX_FILE_SIZE`, `NEXUS_DB_POOL_MAX_CONNECTIONS`, `NEXUS_DRIFT_LIMIT_MS`).
- Finding S-2: diff of `crates/nexus-moment-context-assembly/src/world_context.rs:85-128` and check of CLI consumer at `crates/nexus42/src/commands/creator/run.rs:862`.
- Finding S-3/S-4: cross-check of completion-report.md waiver/defer tables against P0/P1 feature scope (completion_lock, runtime-lock).
- Finding S-5: grep for `R-V140P4-W2` and `R-V140P4-INFRA` across `crates/` (no matches).
- CI cleanliness: `cargo clippy --all -D warnings` (0 warnings), `cargo +nightly fmt --all -- --check` (clean), scoped tests on 5 crates (all pass).

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 1 |
| 🟢 Suggestion | 5 |

**Verdict**: Request Changes

(The UTF-8 boundary panic in the size guard is a runtime reliability defect that must be fixed before closeout. qc2 independently identified the same issue, confirming the finding.)
