---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-06-06-v1.35-cli-ia-implement"
verdict: "Request Changes"
generated_at: "2026-06-07"
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

**Verdict**: Request Changes
