---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-06-06-v1.35-cli-ia-implement"
verdict: "Approve"
generated_at: "2026-06-06T17:37:58Z"
revalidation: "targeted — F-001 (5-group root help) + F-002 (IA order)"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Runtime Model: openai/gpt-5.5
- Review Perspective: Architecture coherence and maintainability risk
- Report Timestamp: 2026-06-07

## Scope
- plan_id: 2026-06-06-v1.35-cli-ia-implement
- Review range / Diff basis: merge-base: 31b7e4e (iteration/v1.35 HEAD after P0) + tip: 441b0da (current HEAD). Equivalent: `git diff 31b7e4e..441b0da`.
- Working branch (verified): feature/v1.35-cli-ia-implement
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.35-p2
- Files reviewed: 5 implementation/test files plus plan/spec context
- Commit range: 31b7e4e..441b0da
- Tools run:
  - `git rev-parse --show-toplevel`
  - `git branch --show-current`
  - `git log -1 --oneline`
  - `git diff 31b7e4e..HEAD --stat`
  - `git diff 31b7e4e..HEAD -- crates/nexus42/src/cli.rs crates/nexus42/src/commands/platform/mod.rs crates/nexus42/src/commands/platform/sync.rs crates/nexus42/src/main.rs crates/nexus42/tests/command_surface_contract.rs`
  - `cargo test -p nexus42 --test command_surface_contract` — passed, 33/33
  - `cargo clippy -p nexus42 -- -D warnings` — passed
  - `cargo +nightly fmt --all -- --check` — passed
  - `./target/debug/nexus42 --help` — completed
  - `./target/debug/nexus42 platform sync --help` — completed

## Findings

### 🔴 Critical
- **F-001 — Root help still exposes six top-level command groups, contradicting the V1.35 five-group IA target.**  
  Evidence: `./target/debug/nexus42 --help` prints `Commands:` with `daemon`, `sync`, `creator`, `acp`, `system`, and `platform`. This conflicts with `cli-command-ia.md` §2/§8, which states the V1.35 target has five groups and standalone `sync` is removed from top-level, and with the assignment checklist item requiring the five groups `creator`, `daemon`, `acp`, `platform`, `system`. The new test `v135_root_help_shows_five_groups_with_sync_deprecated` also locks the opposite behavior: its comment and assertion expect all six groups including `sync`, despite the function name and IA target saying five.  
  **Fix:** Make top-level `sync` a compatibility alias that remains callable but is not shown as a normal root command group, or otherwise align the IA/spec/assignment before merge. Update the V1.35 test to assert the five canonical groups and separately verify alias compatibility/deprecation behavior without preserving `sync` as a sixth visible group.

### 🟡 Warning
- **F-002 — Root command ordering was not aligned with the assigned IA order.**  
  Evidence: assignment checklist specifies root help ordering as `creator`, `daemon`, `acp`, `platform`, `system`; current `./target/debug/nexus42 --help` shows `daemon`, `sync`, `creator`, `acp`, `system`, `platform`. Even after resolving F-001, leaving the enum/help order unchanged will keep the creator-first IA only in `long_about`, not in the visible command list.  
  **Fix:** Reorder the visible top-level command declarations/help output to match the V1.35 IA order, while preserving P3's non-goal of not reordering `creator` subcommands.

### 🟢 Suggestion
- None.

## Source Trace
- Finding ID: F-001
  - Source Type: doc-rule + manual-reasoning + command-output
  - Source Reference: `.mstar/knowledge/specs/cli-command-ia.md` §2/§8; `crates/nexus42/tests/command_surface_contract.rs:898-926`; `./target/debug/nexus42 --help`
  - Confidence: High
- Finding ID: F-002
  - Source Type: assignment-checklist + command-output
  - Source Reference: assignment Architectural Review Checklist; `./target/debug/nexus42 --help`; `crates/nexus42/src/cli.rs`
  - Confidence: High

## Checklist Notes
- Naming clarity: `PlatformCommand::Sync` and `commands::platform::sync::run` are clear and match the existing module pattern.
- Single responsibility: `commands/platform/sync.rs` is a thin delegate with no duplicated sync business logic.
- Reuse vs duplication: sync handlers are reused via `crate::commands::sync::run`.
- Deprecation strategy: top-level `sync` remains functional and emits a stderr warning on dispatch, but its visible root placement conflicts with the five-group IA target.
- Root help ordering: not satisfied; see F-001 and F-002.
- Long_about update: satisfies creator-first wording and demotes `daemon schedule` to Advanced.
- Test coverage: the new tests pass, but one test encodes the wrong six-group root-help contract.
- No breaking change to existing `creator` order: satisfied; no creator subcommand order changes observed.
- No new top-level command group: no new group was introduced, but the legacy `sync` group remains visible, leaving six root groups instead of the five-group target.
- `nexus42d` references: none found in the reviewed Rust scope.

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 1 |
| 🟡 Warning | 1 |
| 🟢 Suggestion | 0 |

**Initial Verdict**: Request Changes (superseded by the targeted revalidation below)

## Revalidation

### What was re-reviewed
- Targeted re-review of the fix wave for the two original QC #1 findings:
  - F-001 (Critical): root help exposed six groups including visible `sync`.
  - F-002 (Warning): root command order did not match the V1.35 IA order.
- Review range / Diff basis: merge-base `31b7e4e` (iteration/v1.35 HEAD after P0) through tip `34270c1` (current HEAD after fix wave), equivalent to `git diff 31b7e4e..34270c1`.
- Working branch verified: `feature/v1.35-cli-ia-implement`.
- Review cwd verified: `/Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.35-p2`.

### F-001 status: RESOLVED
- `crates/nexus42/src/cli.rs` now declares visible `Commands` variants as `Creator`, `Daemon`, `Acp`, `Platform`, `System`, followed by hidden aliases/internal entries.
- `Commands::Sync` has `#[command(hide = true)]` and remains after the five visible groups.
- `./target/debug/nexus42 --help | head -25` shows exactly the five canonical command entries in the `Commands:` section: `creator`, `daemon`, `acp`, `platform`, `system`; no visible top-level `sync` entry appears.
- `./target/debug/nexus42 sync --help | head -10` still succeeds and shows the deprecated hidden alias help with sync subcommands.
- `./target/debug/nexus42 sync status` emits the stderr deprecation warning and reaches the sync status handler (which then reports the local state database migration error in this checkout, confirming handler execution rather than an unrecognized command).
- `cargo test -p nexus42 --test command_surface_contract v135` passed: 4/4 V1.35 tests, including `v135_root_help_shows_five_groups_with_sync_hidden` and `v135_sync_deprecation_warning`.

### F-002 status: RESOLVED
- `Commands` enum order is now `Creator`, `Daemon`, `Acp`, `Platform`, `System`, then hidden `Sync`, `AcpWorker`, and `DaemonRun`.
- `./target/debug/nexus42 --help | grep -A 20 "^Commands:"` shows visible root command order as `creator`, `daemon`, `acp`, `platform`, `system`.
- The three hidden variants (`Sync`, `AcpWorker`, `DaemonRun`) appear at the end of the enum and are hidden from root help.

### Verification commands
- `git rev-parse --show-toplevel` → `/Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.35-p2`.
- `git branch --show-current` → `feature/v1.35-cli-ia-implement`.
- `git log -1 --oneline` → `34270c1 fix(cli): v1.35-p2 F-001/F-002 hide deprecated top-level sync + IA order`.
- `git log --oneline 31b7e4e..HEAD` reviewed the implementation and prior QC report commits in the assigned range.
- `git diff 31b7e4e..HEAD --stat` reviewed the assigned diff scope.
- `./target/debug/nexus42 --help | head -25` completed with five visible command groups.
- `./target/debug/nexus42 sync --help | head -10` completed for the hidden alias.
- `./target/debug/nexus42 sync status` completed far enough to emit the deprecation warning and invoke sync status handling.
- `cargo test -p nexus42 --test command_surface_contract v135` passed (4/4).
- `cargo test -p nexus42 --test command_surface_contract` passed (33/33).
- `cargo clippy -p nexus42 -- -D warnings` passed.
- `cargo +nightly fmt --all -- --check` passed.

### New verdict
**Verdict**: Approve. Both original QC #1 findings are resolved, required command-surface tests and scoped clippy/fmt checks passed, and no new architecture/maintainability findings were introduced by the fix.
