---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-06-06-v1.35-fl-e-ux-polish"
verdict: "Request Changes"
generated_at: "2026-06-06T18:22:04Z"
---

# Code Review Report

## Reviewer Metadata

- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Runtime Model: openai/gpt-5.5
- Review Perspective: Architecture coherence and maintainability risk
- Report Timestamp: 2026-06-06T18:22:04Z

## Scope

- plan_id: 2026-06-06-v1.35-fl-e-ux-polish
- Review range / Diff basis: merge-base: ef085b9 (iteration/v1.35 HEAD after P3) + tip: 371cda0 (current HEAD). Equivalent: git diff ef085b9..371cda0.
- Working branch (verified): feature/v1.35-fl-e-ux-polish
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.35-p4
- Files reviewed: 5
- Commit range: ef085b9..371cda0
- Tools run:
  - `git rev-parse --show-toplevel`
  - `git branch --show-current`
  - `git log -1 --oneline`
  - `git diff ef085b9..HEAD --stat`
  - `git diff ef085b9..371cda0 -- <assigned files>`
  - `gitnexus_detect_changes(scope=compare, base_ref=ef085b9)` (returned no mapped symbols)
  - `gitnexus_impact(target=handle_run, direction=upstream, includeTests=true)` (LOW risk; direct caller `commands::creator::run`)
  - `cargo test -p nexus42`
  - `cargo test -p nexus-daemon-runtime`
  - `cargo clippy -p nexus42 -p nexus-daemon-runtime -- -D warnings`
  - `cargo +nightly fmt --all -- --check`
  - `./target/debug/nexus42 creator run start --help`
  - `./target/debug/nexus42 creator run start --idea "qc parse smoke" --chain-novel-writing=false`

## Findings

### 🔴 Critical

- **C-1: Documented opt-out is not accepted by the CLI parser.** The help text and spec both tell users to opt out with `--chain-novel-writing=false`, but the shipped clap configuration keeps `chain_novel_writing` as a plain boolean flag (`#[arg(long, default_value_t = true)]`). That flag shape rejects explicit values, so the documented opt-out exits at argument parsing before any daemon call:

  ```text
  $ ./target/debug/nexus42 creator run start --idea "qc parse smoke" --chain-novel-writing=false
  error: unexpected value 'false' for '--chain-novel-writing' found; no more were expected
  ```

  This violates the assignment requirements "Opt-out preserved" and "Help text accuracy", and the only new test checks that help contains `Default true`; it does not exercise the opt-out path. Fix by making the flag accept explicit booleans (or by introducing a clap-native negated flag such as `--no-chain-novel-writing`) and adding command-surface coverage for the accepted opt-out syntax.

### 🟡 Warning

- **W-1: The `chain_novel_writing` value is ignored on the normal intake path, so the default-change semantics remain ambiguous.** For the non-`--skip-intake` path, `handle_run` now prints the same `creator run stage advance` hint regardless of whether `chain_novel_writing` is true or false (lines 273-281), while `creator-workflow.md` says `creator run start` "chains intake → produce by default" and the help says it will "automatically chain into the production stage." Since the code only prints a manual stage-advance hint after scheduling intake, the implementation, help, and spec disagree about whether P4 shipped automatic chaining or merely improved the next-step hint. Fix by choosing one architecture contract and aligning code + docs + tests: either implement actual default chaining/next-stage behavior, or describe this as an explicit manual advance hint rather than automatic chaining.

### 🟢 Suggestion

- **S-1: Add a focused regression test for the human output hint.** The new test protects only the help text. Add a command-surface or hermetic CLI test that verifies the post-intake human message uses `nexus42 creator run stage advance <work_id> --stage produce` and contains no `daemon schedule add` guidance, so the creator-centric UX path remains protected.

## Source Trace

- Finding ID: C-1
  - Source Type: manual smoke command + git-diff
  - Source Reference: `crates/nexus42/src/commands/creator/run.rs:36-40`; `./target/debug/nexus42 creator run start --idea "qc parse smoke" --chain-novel-writing=false`
  - Confidence: High
- Finding ID: W-1
  - Source Type: manual-reasoning + git-diff + spec review
  - Source Reference: `crates/nexus42/src/commands/creator/run.rs:273-281`; `.mstar/knowledge/specs/creator-workflow.md:26`; help output from `./target/debug/nexus42 creator run start --help`
  - Confidence: High
- Finding ID: S-1
  - Source Type: test-coverage review
  - Source Reference: `crates/nexus42/tests/command_surface_contract.rs:1086-1108`
  - Confidence: Medium

## Verification Evidence

| Command | Result |
| --- | --- |
| `cargo test -p nexus42` | Passed: 608 unit tests, integration/contract/regression suites passed |
| `cargo test -p nexus-daemon-runtime` | Passed; emitted pre-existing test-target warnings for unused import/variable/must-use |
| `cargo clippy -p nexus42 -p nexus-daemon-runtime -- -D warnings` | Passed |
| `cargo +nightly fmt --all -- --check` | Passed |
| `./target/debug/nexus42 creator run start --help` | Ran; help says `Default true` and documents `--chain-novel-writing=false` |
| `./target/debug/nexus42 creator run start --idea "qc parse smoke" --chain-novel-writing=false` | Failed at clap parsing; evidence for C-1 |

## Checklist Results

- Behavior change is documented: Partial; docs say default chaining, but implementation only prints a manual hint on the normal intake path.
- Opt-out preserved: **No**; documented `--chain-novel-writing=false` is rejected.
- Help text accuracy: **No**; opt-out syntax in help is not accepted, and "automatically chain" overstates the normal intake-path behavior.
- Stage advance hint uses creator run, not daemon schedule: Yes in the modified human output.
- DF-53 tracker accurately reflects partial delivery: Mostly yes; it correctly marks partial delivery/full auto-chain deferred, but should be revisited after resolving C-1/W-1 wording.
- Plan task tracking updated: Yes; T1/T2/T4/T5 checked, T3 deferred.
- No new top-level command group, no `nexus42d` references: Yes.
- Pre-release compatibility: Behavior default changes are permitted pre-1.0, but the opt-out must work as documented.
- No conditional routing, no FL-D: Yes.
- Test coverage: **No**; default/help is covered, opt-out and post-intake hint behavior are not.

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 1 |
| 🟡 Warning | 1 |
| 🟢 Suggestion | 1 |

**Verdict**: Request Changes
