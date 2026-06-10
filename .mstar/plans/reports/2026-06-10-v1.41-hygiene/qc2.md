---
report_kind: qc-review
reviewer: "@qc-specialist-2"
reviewer_index: 2
focus: security-correctness
plan_id: 2026-06-10-v1.41-hygiene
verdict: Request Changes
generated_at: 2026-06-11T11:20:00+08:00
review_range: "merge-base: 55689706 → tip: f4d72a86"
working_branch_verified: iteration/v1.41
review_cwd_verified: /Users/bibi/workspace/organizations/42ch/nexus
files_reviewed: 8
tools_run: cargo clippy --all -D warnings, cargo +nightly fmt --all -- --check, cargo test -p nexus-creator-memory -p nexus-orchestration -p nexus-kb -p nexus-moment-context-assembly -p nexus-daemon-runtime, git log/diff/show on 5 P-last commits, manual source review of review.rs, world_context.rs, works.rs, validation.rs
---

# Code Review Report — V1.41 P-last (qc2)

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: xai/grok-build-0.1
- Review Perspective: Security and correctness risk
- Report Timestamp: 2026-06-11T11:20:00+08:00

## Scope
- plan_id: 2026-06-10-v1.41-hygiene
- Review range / Diff basis: merge-base: 556897061f625c53cd172e2bdb40d509dac61775 → tip: f4d72a86ef88215e9da6f2043fe2f873579f9311 (focus on 5 P-last commits: 90c3f78f, d65851d7, 974c6854, 6041221d, 5d1253ca)
- Working branch (verified): iteration/v1.41
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 8 (review.rs, works.rs, embedded_rules.rs, stage_gates.rs, store.rs, validation.rs, extract.md, world_context.rs + completion-report.md)
- Tools run: cargo clippy --all -D warnings (clean), cargo +nightly fmt --all -- --check (clean), scoped cargo tests on 5 crates (all pass), git log -p / diff / show on P-last commits, manual source reads for truncation, YAML formatting, tracing, and waiver documentation.

## Findings

### 🔴 Critical
- None.

### 🟡 Warning
- **W-01 (correctness — truncation safety)**: In `promote_to_long_term` (R-V133P4-06, `crates/nexus-creator-memory/src/review.rs:644`), the size guard does:
  ```rust
  let raw_digest = if record.raw_digest.len() > MAX_DIGEST_BYTES {
      ...
      &record.raw_digest[..MAX_DIGEST_BYTES]
  ```
  `raw_digest` is `String` (arbitrary UTF-8 from session content / LLM output). Byte slicing `&str[..n]` panics if `n` is not a char boundary. The test uses pure-ASCII `"x".repeat(...)`, which masks the bug. A real digest containing a multi-byte codepoint straddling the 256 KiB boundary will cause a panic inside the safety guard whose purpose is to *prevent* unbounded growth. This is a stability / correctness defect in the fix for the original residual.
  → Fix: use `floor_char_boundary` (Rust 1.80+) or `find char boundary` logic, or truncate via a safe helper before the slice. Add a test with multi-byte content at the boundary.

- **W-02 (correctness — YAML serialization contract)**: `to_yaml` (R-V140P2-S4, `crates/nexus-moment-context-assembly/src/world_context.rs:88-120`) changed all string fields from `{:?}` (Debug) to `{}` (Display). Debug guaranteed valid, quoted, escaped YAML for any content. Display emits raw characters. Fields such as `world_name`, `current_timeline`, `name`, `descriptor` are user/LLM-controlled. Content containing `:`, `"`, newlines, leading spaces, or YAML block indicators can now produce unparseable or ambiguous YAML. The docstring examples and tests assume clean text; no escaping or quoting is performed. This is a regression in the serialization format used for LLM context blocks, even if the stated goal was "cosmetic readability".
  → Re-evaluate: either keep Debug for safety (and document it is for debuggability), implement minimal YAML escaping/quoting for special cases, or switch to a real serializer (e.g. `serde_yaml` or a small emitter that always produces valid YAML). If Display is kept, add explicit validation + tests with metacharacters.

- **W-03 (doc hygiene completeness for waived residuals)**: The 13 waived/deferred items have good centralized notes in the completion report. However, several waived items that rely on "single-user local-first" or "pre-1.0 acceptable" assumptions (e.g. R-V140P1-S3 concurrent-uniqueness race, R-V133P3-04 WorkerUnavailable DoS, R-V140P2-S1 linear scan) have no corresponding short `// WAIVER` or `// SAFETY` comment at the relevant call sites or in the affected modules. Future maintainers (or a post-1.0 security review) will have to hunt the plan history to understand why a race test or DoS consideration was intentionally left unaddressed. The cross-reference comments added for *fixed* items (R-V140P1-S1 drift risk, R-V140P0-S1 400/422) are good examples of what should exist for waived items too.
  → For each waived residual that has a non-obvious invariant or attacker-model assumption, add a one-line code comment at the relevant location referencing the residual ID and the single-user / pre-1.0 rationale. This makes the decision locally auditable without requiring the full plan document.

### 🟢 Suggestion
- **S-01**: The `tracing::warn!` on truncation (R-V133P4-06) and `tracing::info!` on world-binding rejections (R-V140P0-S4) are at the correct levels (warn for operator-visible size event; info for security-relevant authz decisions). No PII leakage concern in the local-first daemon context (creator_id here is the local operator identity; the logged events are rejection paths only, not titles or full digests). Consider whether a future multi-tenant mode would need redaction or hashed IDs — out of scope for this hygiene wave.
- **S-02**: Add a boundary test for the truncation path that uses a multi-byte UTF-8 sequence (e.g. a 3-byte emoji or CJK char) exactly at the 256 KiB cut point. This would have caught W-01 during development.
- **S-03**: The test rename in R-V140P1-S2 (`test_invalid_block_type_via_deserialization` → `test_block_type_enum_rejects_unknown_variant`) is accurate and improves clarity. No finding.
- **S-04**: Excluded residuals (R-V140P4-W2 and R-V140P4-INFRA) are correctly untouched in the 5 P-last commits and are explicitly called out in the completion report. Good.
- **S-05**: All 5 P-last commits are narrowly scoped to the stated residuals. No scope creep or piggy-backed changes observed.

## Source Trace
- Finding W-01: manual code review + test inspection of `crates/nexus-creator-memory/src/review.rs:641-654` and test `promote_truncates_oversized_raw_digest` (lines 1071-1113). Confirmed via `git show 90c3f78f`.
- Finding W-02: diff + source read of `crates/nexus-moment-context-assembly/src/world_context.rs:85-128` (to_yaml) and updated tests (lines 519-536, 804-809). Confirmed via `git show 6041221d`.
- Finding W-03: cross-check of completion-report.md waiver table against the 5 commit patches; absence of WAIVER comments in the modules for R-V140P1-S3, R-V133P3-04, R-V140P2-S1, etc.
- CI cleanliness: `cargo clippy --all -D warnings` (0 warnings), `cargo +nightly fmt --all -- --check` (clean), scoped tests on the 5 crates (149 + 543 + 85 + 43 + 29 tests passed in relevant suites; only pre-existing ignored/doc tests filtered).

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 3 |
| 🟢 Suggestion | 5 |

**Verdict**: Request Changes

(The two correctness issues — unsafe byte truncation in a size guard and loss of YAML validity guarantee — are real regressions in the fixes for R-V133P4-06 and R-V140P2-S4. They must be addressed before the hygiene closeout can be considered complete. The documentation hygiene gap for waived items is a maintainability concern that should be tightened.)

## Revalidation Notes (if targeted re-review)
N/A — initial qc2 review.
