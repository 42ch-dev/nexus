---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: "2026-06-14-v1.46-novel-runtime-ux-edges"
verdict: "Approve"
generated_at: "2026-06-15"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: grok-build-0.1 (xai/grok-build-0.1)
- Review Perspective: Security and correctness risk
- Report Timestamp: 2026-06-15T02:30:00Z

## Scope
- plan_id: 2026-06-14-v1.46-novel-runtime-ux-edges
- Review range / Diff basis: merge-base: ab3312e2 (P1 Done commit, base of P2 work) → tip: 008e6bd8 (P2 merge + plan checkboxes) (5 commits + 1 --no-ff merge + 1 plan-doc = 7 total) — equivalent `git diff ab3312e2..008e6bd8` or `git show --stat ab3312e2..008e6bd8`
- Working branch (verified): iteration/v1.46
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus (project root, checked out at `iteration/v1.46` HEAD `008e6bd8`)
- Files reviewed: 5 (works/mod.rs, run.rs, main.rs, tests/creator_run_preset_help.rs, plan file)
- Commit range: ab3312e2..008e6bd8 (7 total commits in range)
- Tools run: `cargo clippy --all -- -D warnings`, `cargo test -p nexus42`, `cargo +nightly fmt --all --check`, `git diff ab3312e2..008e6bd8 --stat`, `git show --stat ab3312e2..008e6bd8`, manual source review of T1/T2/T3 artifacts

## Findings

### 🔴 Critical
(none)

### 🟡 Warning
- **T2: Manifest `description` / `cli_args[*].description` rendered verbatim in `format_preset_run_help` without control/ANSI sanitization.**  
  `format_preset_run_help` (run.rs:274) interpolates `description` (preset-level) and per-arg `description` directly via `writeln!` into the help block, then `print!` + `exit(0)`. No call to `sanitize_for_terminal` (or equivalent) is made on these strings, unlike the explicit sanitization applied to chapter titles, hints, and work_id in T1 (`works/mod.rs:1505`, `print_chapter_table`).  
  Origin: local user-controlled preset YAML (loaded via `resolve_preset` / baked-in manifests). Not remote/untrusted input.  
  Practical risk: low (user can already influence terminal via their own preset files and can run arbitrary commands). However, this is inconsistent with the defense-in-depth sanitization introduced in the same plan for on-disk chapter hints and leaves a (theoretical) vector for crafted local presets to emit ANSI escapes or control sequences in `--help` output.  
  → **Fix**: apply `sanitize_for_terminal` (or a small shared helper) to `description` and `arg.description` before interpolation in the formatter, or document the intentional omission with a short justification comment.  
  Source: manual review of `format_preset_run_help` + cross-check against T1 sanitization site.

### 🟢 Suggestion
- **T1 `exists()` failure mode is safe and observable, with explicit contract test.**  
  `chapter_path_missing_hint` (works/mod.rs:1443) relies on `Path::exists()` which swallows permission/IO errors as `false`. The dedicated test `chapter_path_missing_hint_exists_failure_is_silent` (works/mod.rs:2476) pins exactly this behavior and confirms that "missing on disk" + `works reconcile-chapters` hint is emitted. The remediation is the same action whether the file is absent or unreadable; the daemon reconcile remains authoritative (per plan AC and Grill #9). No action required; the test + inline comment make the best-effort contract clear and auditable.  
  Source: `works/mod.rs:1446` (the `!ws_dir.join(p).exists()` calls), T1 unit tests, plan §4 AC1.

- **T2 help intercept argv parser has comprehensive edge-case coverage; no bypass risk identified in reviewed scope.**  
  `extract_run_help_target` (run.rs:249) is pure and correctly requires: `creator` token present and immediately followed by `run`, a non-dash preset token after `run`, and `--help`/`-h` appearing *after* the preset token.  
  Unit tests cover: basic case, work_id before help, short `-h`, bare `creator run --help` (None), help-before-preset (None), no-help (None), non-run subcommand (None), no-creator token (None).  
  Binary smoke tests (`tests/creator_run_preset_help.rs`) additionally prove the full path (argv → intercept in `main()` before `Cli::parse()` → manifest load → enriched help → exit(0)) for the three first-slice presets and the fall-through for `novel-writing` (no cli_args).  
  The "help before preset" case correctly does not fire, matching clap's positional ordering. No false-positive paths that would steal help from clap were found in the reviewed cases.  
  Source: run.rs:249 (parser), 1261–1358 (unit tests), tests/creator_run_preset_help.rs:34–117 (e2e), main.rs:14 (pre-parse call site).

- **T3 e2e tests are deterministic and cover the ACs.**  
  The 5 tests in `creator_run_preset_help.rs` are pure help-text assertions against the real `nexus42` binary (via `assert_cmd`). They depend only on the presence of the three target presets (and one no-cli_args preset) in the orchestration registry — which are part of the shipped/baked-in preset data for this crate. No workspace state, network, timing, or tempdir dependencies. All 5 passed cleanly on the review checkout.  
  Source: tests/creator_run_preset_help.rs (full file), `cargo test -p nexus42 --test creator_run_preset_help` execution (5/5 ok).

- **No injection or untrusted-input-to-privileged-operation surfaces in scope.**  
  T1 renders chapter metadata from daemon JSON (with sanitization on display). T2 help path loads manifests from local disk (user-controlled preset files) and renders structured text; the preset_id token from argv is used only as a lookup key and in the header line. No raw user-controlled path or arbitrary string reaches a shell, format string, or privileged API in a dangerous way. The changes are confined to local CLI UX surfaces (status table hints + `--help` enrichment).  
  Source: diff review of works/mod.rs (print path), run.rs (manifest load + format path), main.rs (intercept wiring).

## Source Trace
- Finding ID: W-1 (Warning above)
- Source Type: manual-reasoning + cross-module consistency check
- Source Reference: `format_preset_run_help` (run.rs:274–336) vs `sanitize_for_terminal` (works/mod.rs:1505) + call sites in `print_chapter_table`
- Confidence: High

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 1 |
| 🟢 Suggestion | 4 |

**Verdict**: Approve

## Notes
- All CI gates passed on the review checkout: `cargo +nightly fmt --all --check` (clean), `cargo clippy --all -- -D warnings` (clean), `cargo test -p nexus42` (full suite passed; the 5 new T3 binary smoke tests passed deterministically).
- Scope discipline: review limited to `ab3312e2..008e6bd8`. No edits to implementation; only report authored.
- The single Warning is a low-practical-risk consistency item (local preset YAML only). Core ACs (on-disk hints observable + correct remediation, help intercept for the three presets without false-positive bypass, deterministic tests) are met with safe, observable failure modes.
