---
report_kind: qc-consolidated
plan_id: 2026-06-10-v1.41-hygiene
verdict: Request Changes
generated_at: 2026-06-11T11:30:00+08:00
review_range: "merge-base: 55689706 → tip: f4d72a86"
working_branch_verified: iteration/v1.41
review_cwd_verified: /Users/bibi/workspace/organizations/42ch/nexus
reviewers:
  - "@qc-specialist (1, architecture-coherence-maintainability) — Approve (ca2d6db4)"
  - "@qc-specialist-2 (2, security-correctness) — Request Changes (c4b0708b)"
  - "@qc-specialist-3 (3, performance-reliability) — Request Changes (6d72da5c)"
---

# QC Consolidated Gate — V1.41 P-last (Aggressive residual convergence)

## Verdict
**Request Changes** — 0 Critical, 3 Warning (2 cross-validated). Two regressions introduced by the hygiene fixes themselves must be addressed before Approve.

## Roll-up

| Reviewer | Verdict | Critical | Warning | Suggestion |
|----------|---------|----------|---------|------------|
| @qc-specialist (1, architecture) | Approve | 0 | 0 | 2 |
| @qc-specialist-2 (2, security+correctness) | Request Changes | 0 | 3 | 5 |
| @qc-specialist-3 (3, performance+reliability) | Request Changes | 0 | 1 | 5 |
| **Consolidated (initial)** | **Request Changes** | **0** | **3 (2 deduped)** | **12** |

## Cross-validated Warning

- **W-truncation-utf8-panic** (qc2 W-01 / qc3 W-1, **same finding, high confidence**): `promote_to_long_term` (R-V133P4-06 fix) at `crates/nexus-creator-memory/src/review.rs:651` does `&record.raw_digest[..MAX_DIGEST_BYTES]` on a `String`. If the 256 KiB boundary falls mid-UTF-8 char (CJK, emoji, etc.), the slice panics. The hermetic test uses pure ASCII, masking the bug. **This is a runtime crash in the memory promotion pipeline — a reliability regression in the fix for the original R-V133P4-06 residual.**

  **Fix**:
  - Use `floor_char_boundary` (available in stable Rust ≥ 1.79) or a helper that scans to the nearest `is_char_boundary` before slicing.
  - Add a multi-byte UTF-8 test that seeds a `raw_digest` whose byte length puts the boundary mid-character, then asserts the truncation does NOT panic and the result is valid UTF-8.

  Commit as `fix(creator-memory): UTF-8-safe truncation in promote_to_long_term size guard`.

## Other Warnings

- **W-yaml-display-regression** (qc2 W-02): `to_yaml` (R-V140P2-S4 fix) switched all user/LLM-controlled string fields from `{:?}` (Debug) to `{}` (Display) without quoting/escaping. Content with `:`, `"`, newlines, or YAML-significant characters can now produce unparseable/ambiguous YAML in `world_kb_block`. **Fix**:
  - Either: revert to `{:?}` and document it (Debug produces valid escaped YAML), OR
  - Add minimal YAML-string escaping for `:` and `"` in string field formatting, OR
  - Adopt a real YAML emitter crate (probably overkill for this slice).
  - Add a test that seeds a `World` with a name containing `:` and `"` and asserts the resulting YAML parses (use `serde_yaml` round-trip or a hand-rolled parser test).

  Commit as `fix(moment-context-assembly): YAML-safe string serialization in to_yaml`.

- **W-waiver-doc-hygiene-gap** (qc2 W-03): The 13 waived-with-doc residuals have good centralized notes in the completion report, but lack short `// WAIVER` / `// SAFETY` comments at the affected code call sites. Future maintainers (or a post-1.0 review) must hunt plan history. **Fix**:
  - For each of the 13 waived residuals, add a 1-line code comment at the relevant location referencing the residual ID + rationale (similar to the cross-reference comments added for the fixed items).
  - Example: at `nexus-orchestration/src/auto_chain.rs:mark_pool_entry_completed_for_work`, the guard is `WAIVED (pre-1.0 local-first) — see V1.41 P-last R-V140P0-S3`.

  Commit as `chore(harness): add WAIVER/SAFETY comments at call sites for 13 waived residuals`.

## Targeted re-review plan

After fix wave, dispatch **targeted re-review** to **QC2 + QC3** (N=2 in one turn) — both had Warnings. QC1 was Approve.

Reviewer Assignment: `QC re-review: targeted — reviewers: qc-specialist-2, qc-specialist-3`. Each reviewer updates `qc2.md` / `qc3.md` **in place** with `## Revalidation` section. PM consolidates to `qc-consolidated.md` after.

## Suggested residuals (write to status.json after re-review Approve)

| ID | Severity | Source | Decision | Target |
|----|----------|--------|----------|--------|
| R-V141HYG-01 | medium | qc2 W-01 + qc3 W-1 (cross-validated) | accept-with-fix | this fix wave |
| R-V141HYG-02 | low | qc2 W-02 | accept-with-fix | this fix wave |
| R-V141HYG-03 | nit | qc2 W-03 | accept-with-fix | this fix wave |
| R-V141HYG-04 | nit | qc1 S-001 (UTF-8 boundary helper) | defer | V1.42 |
| R-V141HYG-05 | nit | qc1 S-002 (inline waiver markers) | accept | V1.42 (covered by Fix #3) |
| R-V141HYG-06 | nit | qc3 S-1 (env-driven MAX_DIGEST_BYTES) | defer | V1.42 |
| R-V141HYG-07 | nit | qc3 S-2 (YAML escaping) | defer | V1.42 (covered by Fix #2) |

## Summary

| Severity | Count (initial) | Count (after re-review) |
|----------|------------------|------------------------|
| 🔴 Critical | 0 | 0 (no Critical found) |
| 🟡 Warning | 3 (2 cross-validated) | 0 (all resolved in fix wave) |
| 🟢 Suggestion | 12 | 12 (forward-looking; tracked in residuals) |

**Initial verdict**: Request Changes
**Final verdict (after fix-wave re-review)**: TBD
