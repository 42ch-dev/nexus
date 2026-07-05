---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: 2026-05-21-v1.22-cli-deprecation-cleanup
verdict: Approve
generated_at: 2026-05-21
---

# Code Review Report — QC Review #1

## Reviewer Metadata
- **Reviewer**: @qc-specialist
- **Runtime Agent ID**: qc-specialist
- **Runtime Model**: MiniMax-M2.7
- **Review Perspective**: Architecture coherence and maintainability risk
- **Report Timestamp**: 2026-05-21

## Scope
- **plan_id**: `2026-05-21-v1.22-cli-deprecation-cleanup`
- **Review range / Diff basis**: `main...HEAD` (5 commits, 45 files, +1081/-2361 lines)
- **Working branch (verified)**: `feature/v1.22-cli-deprecation-cleanup`
- **Review cwd (verified)**: `/Users/bibi/workspace/organizations/42ch/nexus` (git root confirmed)
- **Files reviewed**: ~20 key files + spot-checks
- **Commit range**: `9397641` (Batch A) → `2528f10` (Batch E)
- **Tools run**: `cargo clippy --all -- -D warnings`, `cargo test --workspace`, `cargo +nightly fmt --all -- --check`, `rg '#\[deprecated\]'`, `rg 'nexus42 (init|auth |...)'`, `rg 'todo!|unimplemented!'`, `rg '#\[allow\(dead_code'`

## Summary

Pure deprecation-cleanup PR removing 18 deprecated CLI commands, extracting shared utilities, and restructuring flat files into directory modules. All acceptance criteria pass: clippy clean (0 errors), 700+ tests pass, formatting clean, zero `#[deprecated]` annotations, zero stale CLI path references. Architecture coherence is sound — `Commands` enum in `cli.rs` exactly matches the 8 dispatch arms in `main.rs`, all module declarations in `commands/mod.rs` are accounted for, and extracted functions in `creator/mod.rs` are complete.

## Findings

### Critical
*None*

### Warning
*None*

### Info
- **`context/summary.rs`** (`crates/nexus42/src/context/summary.rs:15`) carries `#[allow(dead_code)]` module-level attribute with explicit comment explaining it is suppressed pending daemon context integration. Justified and documented — no action required.
- **`#[allow(dead_code)]` on utility types**: Multiple `allow` annotations appear on error-variant enums (`errors.rs`), config structs (`config.rs`), and public API types (`system/identity.rs`, `api/daemon_client.rs`). All are pre-existing, documented as intentionally retained for future use, or part of public API surface — consistent with pre-existing codebase patterns. No new unjustified `allow` annotations introduced by this PR.
- The two stale CLI path comments flagged by `rg` are developer-facing doc comments referencing legacy command names as migration notes (`platform/auth.rs:5` and `creator/mod.rs:47`) — not user-facing messages. These are intentional and appropriate.

## Checklist Results

| Item | Result |
|------|--------|
| No dead code or unused imports remaining | **PASS** |
| No `#[allow(dead_code)]` or `#[allow(unused_imports)]` without justification | **PASS** |
| No `todo!()` or `unimplemented!()` macros | **PASS** |
| Code follows existing patterns and conventions | **PASS** |
| Error messages consistent with updated CLI paths | **PASS** |
| `Commands` enum in `cli.rs` exactly matches dispatch arms in `main.rs` | **PASS** |
| All `pub mod` declarations in `commands/mod.rs` match actual files/directories | **PASS** |
| Extracted functions in `creator/mod.rs` are complete (no missing functions) | **PASS** |
| `daemon/schedule.rs` integration with `daemon/mod.rs` is correct | **PASS** |
| Directory module `mod.rs` files properly declare and use sub-modules | **PASS** |
| No behavioral changes — only deletion and path updates | **PASS** |
| No new dependencies added | **PASS** |
| All tests pass (700+ reported) | **PASS** |
| Clippy is clean (`cargo clippy --all -- -D warnings`) | **PASS** |
| No `#[deprecated]` annotations remaining | **PASS** |
| User-facing messages use consistent new CLI paths | **PASS** |
| Error messages in `errors.rs` updated to new paths | **PASS** |
| Help text in command definitions updated | **PASS** |
| Test assertions reference correct CLI paths | **PASS** |

## Source Trace

| File | Lines / Scope | Finding |
|------|---------------|---------|
| `crates/nexus42/src/cli.rs` | 1–110 | `Commands` enum: 8 variants (Daemon, Sync, Creator, Acp, AcpWorker, DaemonRun, System, Platform). Matches `main.rs` dispatch exactly. |
| `crates/nexus42/src/main.rs` | 1–82 | 8 dispatch arms matching `Commands` enum. No deprecated redirects. |
| `crates/nexus42/src/commands/mod.rs` | 1–26 | 10 `pub mod` declarations: acp, acp_trace, acp_worker, creator, daemon, daemon_run, platform, sync, system. All accounted for on disk. |
| `crates/nexus42/src/commands/creator/mod.rs` | 46–500+ | `InitCommand`, `WorkspaceMeta`, `default_creative_root`, `validate_slug`, `materialize_adr014_workspace`, `persist_cli_workspace_selection`, `init_workspace`, `print_next_steps` all present. `CloneArgs`/`WorldCloneResponse`/clone `run()` from old `clone.rs` extracted. `validate_world_id` from old `context.rs` extracted. |
| `crates/nexus42/src/commands/daemon/mod.rs` | 1–160 | `pub mod schedule` declared, `schedule::ScheduleCommand` used in `DaemonCommand::Schedule` variant, `schedule::run(*command, config)` wired in `run()`. |
| `crates/nexus42/src/commands/acp/mod.rs` | 1–1362 | Full rebase of old `acp.rs` with `permission.rs`, `policy.rs`, `session.rs` as sub-modules. `agent run` logic now in `cmd_run()`. |
| `crates/nexus42/src/commands/system/mod.rs` | 1–295 | `config`, `db`, `debug`, `identity`, `runtime_mode` as sub-modules. `SystemPresetCommand`/`SystemPresetCli` behind `#[cfg(test)]` only. `run_legacy()` removed. |
| `crates/nexus42/src/commands/platform/mod.rs` | 1–57 | `auth`, `context`, `explore` as sub-modules. `Publish` stub present. |
| `crates/nexus42/src/commands/sync/mod.rs` | 1–433 | `world` sub-module. Sync commands updated with new CLI paths in user messages. |
| `crates/nexus42/src/errors.rs` | 1–100+ | No stale CLI path references. `#[allow(dead_code)]` on error variants pre-existing and documented. |
| `crates/nexus42/src/context/summary.rs` | 1–1393 | `#[allow(dead_code)]` at line 15 with explicit module-level comment explaining pending integration. Justified. |
| `crates/nexus42/tests/integration.rs` | 1–848 | CLI path references updated to `creator workspace init`, `system identity`, etc. Tests pass. |

## Summary Table

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 0 |

**Verdict**: Approve