---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: "2026-06-06-v1.35-critical-residual-convergence"
verdict: "Approve"
generated_at: "2026-06-07T14:30:00+08:00"
revalidation: "targeted — C-QC3-001 (UTF-8 fix only)"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-3
- Runtime Agent ID: qc-specialist-3
- Runtime Model: k2p6
- Review Perspective: Performance, reliability, resource management risk
- Report Timestamp: 2026-06-07T12:30:00+08:00

## Scope
- plan_id: 2026-06-06-v1.35-critical-residual-convergence
- Review range / Diff basis: merge-base: 30efd06 (iteration/v1.35 HEAD before P0) + tip: df59013 (HEAD). Equivalent: git diff 30efd06..df59013.
- Working branch (verified): feature/v1.35-critical-residual-convergence
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.35-p0
- Files reviewed: 6
- Commit range (if not identical to Review range line, explain): 30efd06..df59013 (implementation scope). Current HEAD is 2401ebf which includes qc2 report; only qc2.md differs from df59013.
- Tools run: cargo test, cargo clippy, cargo +nightly fmt --check

## Findings

### 🔴 Critical

- **F-001: `context_summarize.rs` — UTF-8 byte slice truncation can panic on multi-byte character boundary**
  - **Location**: `crates/nexus-orchestration/src/capability/builtins/context_summarize.rs:178`
  - **Issue**: `&content[..DEFAULT_MAX_CONTENT_BYTES]` slices a `&str` by raw byte index. If `DEFAULT_MAX_CONTENT_BYTES` (262,144) falls in the middle of a multi-byte UTF-8 character, this will panic at runtime with a "byte index is not a char boundary" error.
  - **Impact**: Hot path (every LLM summarize call where content exceeds 256 KiB). Any non-ASCII content with a multi-byte character spanning the boundary will crash the capability invocation.
  - **Fix**: Use `content.chars().take(limit).collect::<String>()` or `content.floor_char_boundary(DEFAULT_MAX_CONTENT_BYTES)` to find the nearest valid char boundary before slicing. If using `chars().take()`, the resulting String length in bytes will be ≤ DEFAULT_MAX_CONTENT_BYTES, which satisfies the intent.
  - **Source**: Manual review of diff + Rust `str` slicing semantics.
  - **Confidence**: High

### 🟡 Warning

None.

### 🟢 Suggestion

- **S-001: `prompt_injection.rs` — Consider `BEGIN IMMEDIATE` for write-heavy claim transactions**
  - **Location**: `crates/nexus-local-db/src/prompt_injection.rs:133`
  - **Issue**: `claim_prompt_injections` uses `pool.begin().await?` which starts a DEFERRED transaction (SQLite default). Under high concurrency with multiple workers claiming, DEFERRED transactions may upgrade to write locks late, causing SQLITE_BUSY retries or contention.
  - **Recommendation**: For write-heavy claim paths, `BEGIN IMMEDIATE` fails fast if the write lock is unavailable rather than deferring the lock acquisition until the first write statement. sqlx does not expose this directly, but wrapping the transaction in a `sqlx::query("BEGIN IMMEDIATE").execute(...)` before the SELECT would achieve this.
  - **Source**: Manual review + SQLite transaction semantics.
  - **Confidence**: Medium

- **S-002: `context_summarize.rs` — Total prompt size may exceed downstream LLM client limits**
  - **Location**: `crates/nexus-orchestration/src/capability/builtins/context_summarize.rs:162-195`
  - **Issue**: The 256 KiB cap is applied to `content` only. The final prompt also includes `instructions` (~100 bytes), `trace` (unbounded), and the truncation marker (~50 bytes). A caller passing 256 KiB content + a large trace could exceed the intended total prompt budget.
  - **Recommendation**: Consider applying the cap to `content_display + trace` combined, or document that the 256 KiB limit applies to content alone and downstream LLM clients must tolerate the overhead.
  - **Source**: Manual review of prompt builder composition.
  - **Confidence**: Medium

- **S-003: `kb_extract_job.rs` — `mark_running` return value doesn't indicate transition success**
  - **Location**: `crates/nexus-local-db/src/kb_extract_job.rs:259-268`
  - **Issue**: `mark_running` returns `Ok(())` regardless of whether `rows_affected() == 0` or `1`. Callers cannot distinguish "job was queued and is now running" from "job was already running/done/failed, no change". The `claim_job` function handles this correctly by checking `rows_affected()`, but standalone callers of `mark_running` lose visibility.
  - **Recommendation**: Return `Result<bool, sqlx::Error>` or a dedicated enum indicating whether the transition actually occurred. This improves observability for callers that need to know if they won the claim.
  - **Source**: Manual review of `mark_running` vs `claim_job` patterns.
  - **Confidence**: Low

## Source Trace

### F-001: UTF-8 boundary panic
```rust
// crates/nexus-orchestration/src/capability/builtins/context_summarize.rs:176-181
format!(
    "{}\n\n[truncated at {} bytes — original was {} bytes]",
    &content[..DEFAULT_MAX_CONTENT_BYTES],  // <-- panics if not on char boundary
    DEFAULT_MAX_CONTENT_BYTES,
    content.len()
)
```
- **Source Type**: manual-reasoning
- **Source Reference**: Rust `str` indexing docs; `str::slice_indices` panic condition
- **Confidence**: High

### S-001: DEFERRED transaction for write-heavy claims
```rust
// crates/nexus-local-db/src/prompt_injection.rs:133
let mut tx = pool.begin().await?;
```
- **Source Type**: manual-reasoning
- **Source Reference**: SQLite transaction semantics; sqlx `Pool::begin` behavior
- **Confidence**: Medium

### S-002: Total prompt size cap
```rust
// crates/nexus-orchestration/src/capability/builtins/context_summarize.rs:186
let mut prompt = format!("{instructions}\n\n--- Content ---\n{content_display}");
```
- **Source Type**: manual-reasoning
- **Source Reference**: Prompt builder composition logic
- **Confidence**: Medium

### S-003: mark_running opaque return
```rust
// crates/nexus-local-db/src/kb_extract_job.rs:259-268
pub async fn mark_running(pool: &SqlitePool, job_id: &str) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"UPDATE kb_extract_jobs SET status = 'running', started_at = datetime('now') WHERE job_id = ? AND status = 'queued'"#,
        job_id,
    )
    .execute(pool)
    .await?;
    Ok(())  // <-- doesn't expose rows_affected
}
```
- **Source Type**: manual-reasoning
- **Source Reference**: `kb_extract_job.rs` function signature
- **Confidence**: Low

## Evidence

### Test results
```bash
$ cd /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.35-p0
$ cargo test -p nexus-orchestration -p nexus-daemon-runtime -p nexus-local-db
# ... all tests passed (nexus-local-db: 135, nexus-orchestration: 512, nexus-daemon-runtime: 2 doc-tests)
```

### Clippy
```bash
$ cargo clippy -p nexus-orchestration -p nexus-daemon-runtime -- -D warnings
    Finished `dev` profile [unoptimized + debug info] target(s) in 0.23s
# No warnings
```

### Format check
```bash
$ cargo +nightly fmt --all -- --check
# No output (clean)
```

### Status.json residual count verification
```
metadata.tech_debt_summary.total_open: 28
Actual count of items in residual_findings arrays:
  - 2026-06-04-v1.33-llm-judge-runtime-fix: 2
  - 2026-06-04-v1.33-memory-review-closed-loop: 3
  - 2026-06-04-v1.33-work-model-and-creator-run: 3
  - v1.31-post-qc-tech-debt: 5
  - v1.30-post-qc-tech-debt: 9
  - 2026-06-04-v1.34-fl-e-run-intents-and-stages: 5
  - 2026-06-04-v1.34-agent-tool-implementation: 1
  - 2026-06-04-v1.34-cursor-pr42-stage-status: 0
  Total: 28 ✓
```

## Revalidation

### What was re-reviewed

Targeted re-review of **C-QC3-001 (Critical)** — TD-V131-04 multi-byte UTF-8 panic on `&content[..DEFAULT_MAX_CONTENT_BYTES]` in `context_summarize.rs`. This was the sole blocking finding from the initial QC #3 review.

### C-QC3-001 status: **RESOLVED**

**Evidence:**

1. **`truncate_to_char_boundary()` helper exists** at `crates/nexus-orchestration/src/capability/builtins/context_summarize.rs:164-173`.

2. **Helper uses `str::is_char_boundary`** to walk backwards to a valid UTF-8 boundary (line 169). The loop decrements `idx` at most 4 times (max UTF-8 encoding length), giving O(1) / O(k) where k ≤ 4 performance — confirmed by code inspection:
   ```rust
   fn truncate_to_char_boundary(s: &str, max_bytes: usize) -> &str {
       if s.len() <= max_bytes {
           return s;
       }
       let mut idx = max_bytes;
       while idx > 0 && !s.is_char_boundary(idx) {
           idx -= 1;
       }
       &s[..idx]
   }
   ```

3. **`build_summary_prompt` no longer uses direct byte slicing** — replaced with `truncate_to_char_boundary(content, DEFAULT_MAX_CONTENT_BYTES)` at line 195.

4. **Regression tests added and passing** (18 total tests, was 15 + 3 new):
   - `build_summary_prompt_truncates_multibyte_utf8_without_panic` — 256 KiB of 3-byte CJK chars with cap at non-multiple-of-3 boundary; does not panic
   - `build_summary_prompt_truncates_at_clean_char_boundary` — verifies no truncation when under cap with multi-byte chars
   - `build_summary_prompt_truncates_mid_cjk_char` — prepends ASCII padding so cap lands mid-3-byte-CJK-char; truncates safely

5. **All existing `context_summarize` tests still pass** — 18 passed, 0 failed.

6. **Clippy clean** — `cargo clippy -p nexus-orchestration -- -D warnings` passes with no warnings.

### New findings

None. No new issues introduced by the fix.

### Performance verification

The `truncate_to_char_boundary` helper is hot-path safe:
- Early return when `s.len() <= max_bytes` (common case for content under 256 KiB)
- Backward walk bounded by 4 iterations (max UTF-8 scalar byte length)
- No allocation, no linear scan of the entire string
- Total complexity: O(1) amortized

### Updated Verdict

**Approve** — C-QC3-001 is fully resolved. The UTF-8 boundary panic risk is eliminated with a bounded, allocation-free helper and comprehensive regression tests. The 3 non-blocking Suggestions (S-001, S-002, S-003) from the initial review remain deferred to a follow-up wave.

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 3 (deferred) |

**Verdict**: Approve

C-QC3-001 (Critical — UTF-8 boundary panic) is fully resolved. No new issues introduced. The fix is minimal, correct, and well-tested.
