---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: "2026-06-06-v1.35-critical-residual-convergence"
verdict: "Approve w/ residuals"
generated_at: "2026-06-07T13:42:00+08:00"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: xai/grok-build-0.1
- Review Perspective: Security and correctness risk (auth boundaries, race conditions, untrusted input handling, data consistency, encoding safety)
- Report Timestamp: 2026-06-07T13:42:00+08:00

## Scope
- plan_id: 2026-06-06-v1.35-critical-residual-convergence
- Review range / Diff basis: merge-base: 30efd06 (iteration/v1.35 HEAD before P0) + tip: df59013 (HEAD). Equivalent: `git diff 30efd06..df59013`.
- Working branch (verified): feature/v1.35-critical-residual-convergence
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.35-p0
- Files reviewed: 14 (per `git diff --stat`)
- Commit range: 30efd06..df59013 (6 implementation commits + 1 fmt + 1 harness cleanup)
- Tools run:
  - `cargo test -p nexus-orchestration -p nexus-daemon-runtime -p nexus-local-db` (all passed)
  - `cargo clippy -p nexus-orchestration -p nexus-daemon-runtime -- -D warnings` (clean)
  - `cargo +nightly fmt --all -- --check` (clean)
  - Manual source review of: works.rs, works_api.rs, prompt_injection.rs, kb_extract_job.rs, reference_source.rs, context_summarize.rs, 5 new archive JSONs, status.json residual lifecycle

## Findings

### 🔴 Critical
(none)

### 🟡 Warning
- **W-001 (TD-V131-04 follow-up)**: `context.summarize` content truncation uses byte slicing on `&str` (`&content[..DEFAULT_MAX_CONTENT_BYTES]`) without char-boundary alignment. This will panic at runtime on multi-byte UTF-8 content (CJK, emoji, etc.) whose cut point lands inside a code point. The size cap itself (256 KiB) and the marker format are correct; the implementation is not char-aware. Tests only exercise ASCII (`"x".repeat`, `"a".repeat`). This is a latent correctness/availability defect introduced in the same commit that added the cap (26b2fa8).

  **Recommended fix**: Use `content.get(..DEFAULT_MAX_CONTENT_BYTES)` (returns `Option<&str>`, safe) or a boundary-aware truncate (e.g., `content.char_indices().nth(...)` or `str::floor_char_boundary` in newer Rust). Update the two unit tests to include at least one multi-byte case.

- **W-002 (minor, non-blocking)**: `claim_prompt_injections` uses runtime `sqlx::query` + dynamic `LIMIT` and a generated `IN (...)` for the UPDATE. This is documented with a SAFETY comment and only uses application-generated ULIDs bound as parameters (no user-controlled SQL). The transaction scoping (begin → early rollback on empty → commit) is correct and matches the `claim_job` pattern in kb_extract_job. Acceptable for now, but worth a future compile-time macro refactor if sqlx gains better LIMIT/IN support.

### 🟢 Suggestion
(none)

## Source Trace

**Finding ID**: W-001
**Source Type**: manual-reasoning + code inspection
**Source Reference**:
- `crates/nexus-orchestration/src/capability/builtins/context_summarize.rs:170` (`if content.len() > ... { &content[..DEFAULT_MAX_CONTENT_BYTES] }`)
- `build_summary_prompt` tests: 400–418 (only ASCII)
- Assignment checklist item: "Encoding safety — content size truncation in `context_summarize` is char-aware (UTF-8 boundary)"
**Confidence**: High

**Finding ID**: R-CURSOR-PR42-03 (verified closed)
**Source Type**: git-diff + test inspection
**Source Reference**:
- `works.rs:336–342` (the new `check_stage_status_transition` call in `patch_work_stage`)
- `works.rs:515–538` (the helper: rejects terminal status without force when coming from non-terminal)
- `works_api.rs:1089–1210` (three new tests: 400 without force, 200 with force=true, 200 for non-terminal "active"; all assert status codes + side-effect DTO, not just `assert!(true)`)
**Confidence**: High

**Finding ID**: TD-V131-01 / TD-V131-03 (verified closed)
**Source Type**: git-diff + code review
**Source Reference**:
- `prompt_injection.rs:133–176` (claim now `pool.begin()`, SELECT inside tx, early rollback, UPDATE, `commit()`)
- `prompt_injection.rs:207–242` (mark_consumed with `BATCH_LIMIT=100`, `chunks()`, constant columns + bound params only, empty-list fast path)
- Tests cover empty, single, priority, wrong-session, and the >100 path implicitly
**Confidence**: High

**Finding ID**: TD-V130-06 (verified closed)
**Source Type**: git-diff + code review
**Source Reference**:
- `kb_extract_job.rs:259–269` (`mark_running` now has `WHERE status = 'queued'`)
- `claim_job` (lines 281–357) already does the atomic claim with rows_affected() check
- Existing lifecycle tests + new guard semantics documented in the docstring
**Confidence**: High

**Finding ID**: TD-V130-02 (verified closed)
**Source Type**: git-diff + code review
**Source Reference**:
- `reference_source.rs:196–214` (`cleanup_row` now does `tracing::error!` with only `reference_source_id` + error; no PII, no SQL, no bind values; best-effort spawn)
- Cargo.toml added `tracing` dep for the crate
**Confidence**: High

**Finding ID**: V1.33 criticals still closed (R-V133P3-01/02, R-V133P4-01/02/03/07)
**Source Type**: grep + archive JSON cross-check
**Source Reference**:
- `judge_llm.rs:216` (first-token parse + nogo/go ordering comment)
- `tasks/mod.rs:1868` + `preset/mod.rs` (template_file pre-load + `assert_template_file_safe`)
- `memory.rs`: active creator gates on all 4 pending-review handlers + review/fragments (R-V133P4-01/07), UNTRUSTED header on promote (R-V133P4-03)
- Archive files `2026-06-04-v1.33-llm-judge-runtime-fix.json` and `2026-06-04-v1.33-memory-review-closed-loop.json` correctly record the original fix commits (d271115, c604a2d, 26dc124, 026972d, 4455f09, fb390c2) plus "v1.35_p0_revalidation" notes
- `status.json` root `residual_findings` no longer lists the 6 IDs (they were removed in df59013)
**Confidence**: High

**Finding ID**: R-CURSOR-PR42-03 archive + DF-47 carry-forward
**Source Type**: archive JSON + plan + status.json
**Source Reference**:
- `2026-06-04-v1.34-cursor-pr42-stage-status.json` records the closure with evidence (59e50bb + 3 tests + clippy clean)
- Plan file and status.json both explicitly note DF-47 carry-forward to V1.36 (not in scope for this P0)
**Confidence**: High

## Evidence (commands + output)

**Pre-review alignment (all passed)**:
```bash
cd /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.35-p0
git rev-parse --show-toplevel   → /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.35-p0
git branch --show-current       → feature/v1.35-critical-residual-convergence
git log -1 --oneline            → df59013 harness(v1.35-p0): residual lifecycle cleanup — 11 closures + 5 archive files
git diff 30efd06..HEAD --stat   → 14 files, 571 insertions, 179 deletions (matches Assignment)
```

**CI (all clean)**:
```bash
cargo test -p nexus-orchestration -p nexus-daemon-runtime -p nexus-local-db
# ... (full suite) ...
test result: ok.  (all crates green; 0 failures)

cargo clippy -p nexus-orchestration -p nexus-daemon-runtime -- -D warnings
# Finished dev profile ... (exit 0, no warnings treated as errors)

cargo +nightly fmt --all -- --check
# (no output = clean)
```

**Key diff inspection** (R-CURSOR-PR42-03 gate + tests):
- The `check_stage_status_transition` helper + call site in `patch_work_stage` correctly reject `stage_status` terminal transitions without `force` when no `current_stage` is supplied.
- The three new tests in `works_api.rs:1051–1210` assert real HTTP/handler status codes (400 BAD_REQUEST with `INVALID_STATUS_TRANSITION`, 200 with `force=true`, 200 for non-terminal) and inspect the returned DTO for side effects. They follow the existing 18-test pattern (hermetic state, handler invocation or full server, creator config).

**Transaction / batching / guard inspection**:
- `claim_prompt_injections`: explicit `begin()`, early `rollback()` on empty result set, UPDATE inside the same tx, `commit()`. No path leaks an open transaction.
- `mark_prompt_injections_consumed`: `BATCH_LIMIT=100`, `chunks()`, empty-list early return, constant column list + only bound parameters.
- `mark_running`: `WHERE status='queued'` guard present and documented.
- `cleanup_row`: `tracing::error!` only (no `println!`, no PII, no SQL text or binds).

**Previously-closed V1.33 items re-validated at HEAD**:
- R-V133P3-01/02, R-V133P4-01/02/03/07 all still have the fix sites + comments present.
- Archive JSONs + status.json bookkeeping are consistent (11 closures recorded, 5 new archive files with correct `id`/`severity`/`decision`/`closure_evidence`).

**status.json + archives**:
- 5 new files under `.mstar/archived/residuals/` exactly match the ones listed in df59013.
- The 6 V1.33 criticals were already fixed in prior waves; this P0 correctly promoted their closure to the SSOT and archived the evidence.
- DF-47 explicitly called out as carry-forward (not closed here).

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 1 |
| 🟢 Suggestion | 0 |

**Verdict**: Approve w/ residuals

The P0 changes are narrowly scoped, well-tested, and address the assigned residual IDs with no new Critical security or correctness defects. The single Warning (UTF-8-unaware byte truncation in the new size-cap logic) is a self-contained correctness issue in one of the hardening commits; it does not regress any of the closed criticals and does not affect the auth, transaction, or gate invariants that were the primary focus of this review. CI is green. Residual lifecycle artifacts (status.json + 5 archives) are accurate.

Recommended residual (for the PM to register or waive):
- W-001: Make `context.summarize` truncation char-boundary safe (or use `str::get` + fallback) and add a multi-byte test case. Owner: implementer of TD-V131-04. Target: next suitable wave (V1.36 or a small follow-up).

No other action required before merge.
