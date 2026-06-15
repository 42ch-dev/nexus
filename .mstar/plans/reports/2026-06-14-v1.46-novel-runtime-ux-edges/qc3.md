---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: "2026-06-14-v1.46-novel-runtime-ux-edges"
verdict: "Approve"
generated_at: "2026-06-15"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-3
- Runtime Agent ID: qc-specialist-3
- Runtime Model: kimi-for-coding/k2p7
- Review Perspective: Performance and reliability risk — `exists()` per-chapter cost, help-intercept startup overhead, manifest load laziness/caching, test suite reliability
- Report Timestamp: 2026-06-15T10:00:00+08:00
- **Note**: Revalidation round (targeted re-review of W-001 fix; qc1/qc2 retain initial-wave Approve)

## Scope
- plan_id: `2026-06-14-v1.46-novel-runtime-ux-edges`
- Review range / Diff basis: `merge-base: ab3312e2 (P1 Done commit, base of P2 work) → tip: 008e6bd8 (P2 merge + plan checkboxes) (5 commits + 1 --no-ff merge + 1 plan-doc = 7 total)` — equivalent `git diff ab3312e2..008e6bd8` or `git show --stat ab3312e2..008e6bd8`
- Working branch (verified): `iteration/v1.46`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus` (project root, checked out at `iteration/v1.46` HEAD `008e6bd8`)
- Files reviewed: 5 (`crates/nexus42/src/commands/creator/works/mod.rs`, `crates/nexus42/src/commands/creator/run.rs`, `crates/nexus42/src/main.rs`, `crates/nexus42/tests/creator_run_preset_help.rs`, `.mstar/plans/2026-06-14-v1.46-novel-runtime-ux-edges.md`)
- Commit range (if not identical to Review range line, explain): same as Review range — `ab3312e2..008e6bd8`
- Tools run: `git diff ab3312e2..008e6bd8 --stat`, `git show --stat ab3312e2..008e6bd8`, `cargo clippy --all -- -D warnings`, `cargo test -p nexus42`, `cargo +nightly fmt --all --check`

## Focus Assessment (PM-mandated performance/reliability questions)

| # | Question | Finding |
| --- | --- | --- |
| 1 | Does per-chapter `exists()` add measurable latency on the `works status` hot path? | **Yes, for large chapter counts.** `print_chapter_table` resolves the workspace directory once, but then iterates every chapter row and calls `Path::exists()` for each configured `body_path` / `outline_path`. Each call is a synchronous blocking filesystem syscall; a 100-chapter work performs up to 200 sequential `exists()` calls. On local SSD this is sub-millisecond, but on slower filesystems (network mounts, encrypted volumes) this can add tens to hundreds of milliseconds to a command whose user-facing contract is a quick status snapshot. See Warning W-001. |
| 2 | Does the argv parse before `Cli::parse()` add startup latency for non-help invocations? | **No.** `extract_run_help_target` is a pure, allocation-light scan over `std::env::args()`; it returns `None` and exits before any manifest I/O for all non-help command shapes. No measurable overhead. |
| 3 | Is the manifest loaded lazily / cached? | **Lazily, not cached.** The manifest is loaded only when (a) the invocation matches `creator run <preset_id> --help/-h`, and (b) the preset declares non-empty `cli_args`. This is the correct lazy scope for a help command. Caching is unnecessary for a one-off help invocation. No concern for normal commands. |
| 4 | Is the test suite reliable? | **Yes.** `cargo test -p nexus42` reports 840 passed (across all test targets). The new e2e smoke tests in `tests/creator_run_preset_help.rs` invoke the real binary via `assert_cmd` and assert on stable string markers; they are deterministic and passed locally. |

## Findings

### 🔴 Critical
*None.*

### 🟡 Warning

#### W-001: T1 adds synchronous, unbounded per-chapter I/O to the `works status` render path
- **Scope**: `crates/nexus42/src/commands/creator/works/mod.rs:1386-1415` (`print_chapter_table` → `chapter_path_missing_hint`)
- **Evidence**: For every chapter row, the code calls `ws_dir.join(p).exists()` for `body_path` and/or `outline_path` (lines 1446-1447). The loop is sequential and the number of calls scales linearly with configured chapter paths. The `ws_dir` is resolved once per table render, but no caching or concurrency is applied to the `exists()` calls.
- **Impact**: On a typical local SSD the cost is negligible, but for multi-volume works with 100+ chapters on slower storage, the command can stall perceptibly. Because `works status` is a high-frequency, user-facing hot path, this degrades the interactive experience in direct proportion to work size.
- **Spec context**: Grill #9 explicitly chose "CLI best-effort `exists()` on chapter paths", so the behavior is intended. The concern is the *absence of mitigation/observability* for the latency tail.
- **Fix options** (pick one or more):
  1. Add a `tracing::debug!` / `tracing::info!` span around the hint loop that records chapter count and elapsed milliseconds, so operators can observe the cost.
  2. Cap or skip the hint when the chapter list exceeds a reasonable threshold (e.g., warn once at the bottom instead of per-row).
  3. Perform the existence checks concurrently via `tokio::task::spawn_blocking` (the function is currently synchronous display code, but the caller is async) if preserving the per-row UX is required.
  4. Document the performance characteristic in `novel-author-experience.md` or the crate `AGENTS.md` so users/authors know the cost scales with chapter count.

### 🟢 Suggestion

#### S-001: T2 help-intercept exit path should flush stdout before `std::process::exit(0)`
- **Scope**: `crates/nexus42/src/commands/creator/run.rs:231-235` (`maybe_print_preset_run_help_and_exit`)
- **Evidence**: The function uses `print!("{}", format_preset_run_help(...))` and then `std::process::exit(0)`. `std::process::exit` bypasses destructors and does not guarantee that Rust's line-buffered stdout `BufWriter` has flushed its final line in all environments (e.g., when stdout is redirected to a pipe or file).
- **Impact**: Low in practice because the emitted help block ends with a newline, which triggers line-buffered flushing on most terminals. Risk is limited to non-terminal consumers.
- **Fix**: Add an explicit flush before exit:
  ```rust
  use std::io::Write;
  let _ = std::io::stdout().flush();
  std::process::exit(0);
  ```

#### S-002: T2 `CapabilityRegistry::with_builtins()` is rebuilt on every help intercept
- **Scope**: `crates/nexus42/src/commands/creator/run.rs:214`
- **Evidence**: Each matching help invocation reconstructs the builtin capability registry from scratch. This is acceptable for a one-off help command, but as more builtins are added the cost will grow.
- **Impact**: Negligible today; future scalability suggestion only.
- **Fix**: Consider a `once_cell::sync::Lazy` static registry if the registry becomes expensive, or leave as-is with a comment noting the lazy-per-help scope.

## Source Trace

| Finding ID | Source Type | Source Reference | Confidence |
|------------|-------------|------------------|------------|
| W-001 | static-analysis + manual-reasoning | `crates/nexus42/src/commands/creator/works/mod.rs` lines 1386-1415, 1443-1459 | High |
| S-001 | manual-reasoning | `crates/nexus42/src/commands/creator/run.rs` lines 231-235 | Medium |
| S-002 | manual-reasoning | `crates/nexus42/src/commands/creator/run.rs` line 214 | Medium |

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 1 |
| 🟢 Suggestion | 2 |

**Verdict**: Request Changes

Rationale: W-001 identifies a real performance/reliability degradation on the `works status` hot path that scales with chapter count. Because the spec mandates the `exists()` behavior, the required fix is not to remove the feature but to add observability, a cap, concurrency, or documentation before the plan is approved. Once W-001 is resolved (or explicitly risk-accepted and tracked as a residual), the plan can move to Approve.

## Validation Evidence

```text
$ git rev-parse --show-toplevel
/Users/bibi/workspace/organizations/42ch/nexus

$ git branch --show-current
iteration/v1.46

$ git log -1 --oneline
008e6bd8 chore(v1.46-p2): mark plan tasks T1-T4 complete

$ cargo clippy --all -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.24s

$ cargo test -p nexus42
... 840 passed; 0 failed; ...

$ cargo +nightly fmt --all --check
(no output)
```

## Revalidation
- **Round**: targeted re-review (qc-specialist-3 only; qc1/qc2 stay Approve)
- **Review basis**: git diff 008e6bd8..ce58d005 (P2 fix + qc docs); fix-only slice is 70587add..ce58d005 = 1 file (`crates/nexus42/src/commands/creator/works/mod.rs`)
- **Prior findings status**:
  - **W-001** (per-chapter `exists()` latency in `print_chapter_table`): **Resolved in this round** at commit `eca490a2` (merge `ce58d005`). Evidence: `CHAPTER_PATH_HINT_CAP = 50` const bounds the synchronous `Path::exists()` loop; `tracing::info_span!("chapter_path_hints", total_chapters, capped = hint_cap)` records chapter count and effective cap; elapsed-ms log fires when the loop exceeds 100 ms; a `+ N more (paths not checked)` summary line covers chapters beyond the cap; `chapter_path_missing_hint` still checks both `body_path` and `outline_path` for the first `hint_cap` rows, preserving Grill #9 behavior for those rows.
  - **S-001** (stdout flush before `exit(0)`): **Still open — deferred to residual `R-V146P2-QC3-S1`**. Fix round only touched `works/mod.rs`; `run.rs` was not modified.
  - **S-002** (`CapabilityRegistry` rebuilt per help intercept): **Still open — deferred to residual `R-V146P2-QC3-S2`**. Fix round only touched `works/mod.rs`; `run.rs` was not modified.
- **Fix-round regressions**: None. Only `works/mod.rs` changed in `70587add..ce58d005`; no behavior outside the chapter hint path was modified.
- **CI gates** (re-run on `iteration/v1.46` HEAD `ce58d005`):
  - `cargo clippy --all -- -D warnings` — clean
  - `cargo test -p nexus42` — 843 passed, 0 failed (840 baseline + 3 new cap/summary tests)
  - `cargo +nightly fmt --all --check` — clean
- **Open residuals remain tracked**: The 5 low-severity residuals (`R-V146P2-QC2-W`, `R-V146P2-QC1-S1`, `R-V146P2-QC1-S2`, `R-V146P2-QC3-S1`, `R-V146P2-QC3-S2`) are still present in `.mstar/status.json` `residual_findings["2026-06-14-v1.46-novel-runtime-ux-edges"]` and were not closed by this round.
- **Updated verdict**: Approve
