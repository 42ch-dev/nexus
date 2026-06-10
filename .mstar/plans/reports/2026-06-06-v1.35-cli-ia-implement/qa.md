---
report_kind: qa
reviewer: qa-engineer
plan_id: "2026-06-06-v1.35-cli-ia-implement"
verdict: "Approve"
generated_at: "2026-06-06T17:41:51Z"
working_branch: "feature/v1.35-cli-ia-implement"
review_cwd: "/Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.35-p2"
review_range: "merge-base: 31b7e4e (iteration/v1.35 HEAD after P0) + tip: d5d3098 (current HEAD). Equivalent: git diff 31b7e4e..d5d3098"
---

# QA Report — V1.35 P2 CLI IA Implement

## Scope tested

- Reviewer: `qa-engineer`
- Plan ID: `2026-06-06-v1.35-cli-ia-implement`
- Plan path: `.mstar/plans/2026-06-06-v1.35-cli-ia-implement.md`
- Working branch (verified): `feature/v1.35-cli-ia-implement`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.35-p2`
- Review range / Diff basis: `merge-base: 31b7e4e (iteration/v1.35 HEAD after P0) + tip: d5d3098 (current HEAD). Equivalent: git diff 31b7e4e..d5d3098`
- QC consolidated verdict: Approve (`qc1` + `qc2` + `qc3` all Approve after fix wave)

## Pre-review alignment

```text
$ git rev-parse --show-toplevel
/Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.35-p2

$ git branch --show-current
feature/v1.35-cli-ia-implement

$ git log -1 --oneline
d5d3098 harness(v1.35-p2): qc-consolidated Approve — proceed to QA

$ git diff 31b7e4e..HEAD --stat
9 files changed, 566 insertions(+), 25 deletions(-)
```

## Acceptance evidence

| # | Acceptance criterion | Result | Evidence |
|---|---|---|---|
| 1 | `nexus42 platform sync --help` works and shows `pull|push|status` plus delegated subcommands | Pass | Help lists `push`, `pull`, `status`, `resolve`, `world`, `retry`. |
| 2 | `nexus42 sync pull` still works and emits stderr deprecation warning | Pass | `./target/debug/nexus42 sync pull 2>&1 | head -5` starts with `Warning: \`nexus42 sync\` is deprecated...`; handler remains callable and then reports local-only platform-connectivity limitation in this environment. |
| 3 | `command_surface_contract` tests pass | Pass | `cargo test -p nexus42 --test command_surface_contract`: 33 passed, 0 failed. |
| 4 | Root `nexus42 --help` shows 5-group IA in canonical order | Pass | Commands section lists exactly `creator`, `daemon`, `acp`, `platform`, `system` in order; `sync` is not visible. |
| 5 | Root `long_about` mentions `creator run start` and `workspace init` | Pass | Root help Quick start includes `nexus42 creator workspace init` and `nexus42 creator run start`. |
| 6 | CI gates pass: cargo test, clippy, fmt | Pass | `cargo test -p nexus42 --test command_surface_contract`, `cargo clippy -p nexus42 -- -D warnings`, and `cargo +nightly fmt --all -- --check` all exited successfully. |

## Reproduction steps and command results

### Build

```text
$ cargo build -p nexus42
Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.23s
```

### Tests

```text
$ cargo test -p nexus42 --test command_surface_contract
running 33 tests
...
test result: ok. 33 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.62s
```

### Clippy

```text
$ cargo clippy -p nexus42 -- -D warnings
Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.22s
```

### Formatting

```text
$ cargo +nightly fmt --all -- --check
# no output; exit 0
```

### Root help IA

```text
$ ./target/debug/nexus42 --help
Commands:
  creator   Manage Creator entities (register, pair, credentials, workspace, soul, memory, kb)
  daemon    Manage the daemon runtime
  acp       ACP capability plane (agents, registry, skills, connectivity)
  platform  Platform interaction (auth, explore, context, publish, **sync**)
  system    System management (presets, diagnostics, config, identity, etc.)
  help      Print this message or the help of the given subcommand(s)
```

Root long_about evidence:

```text
Quick start:
nexus42 creator workspace init    Set up a new workspace
nexus42 creator run start         Launch a creative run
```

### Platform sync help

```text
$ ./target/debug/nexus42 platform sync --help
Commands:
  push     Push local changes to platform
  pull     Pull platform bundles into the local outbox (requires platform URL/token on daemon)
  status   Show sync status
  resolve  Resolve a sync conflict with a specific strategy
  world    World fork and snapshot (migrated from `nexus42 world`)
  retry    Retry a failed sync operation (coming soon)
```

### Deprecated top-level sync alias

```text
$ ./target/debug/nexus42 sync --help 2>&1 | head -5
Hidden: deprecated top-level sync alias — use `platform sync` instead. Kept callable for ≥1 iteration (V1.35) per cli-command-ia.md §5

Usage: nexus42 sync [OPTIONS] <COMMAND>

Commands:
```

```text
$ ./target/debug/nexus42 sync status 2>&1 | head -3
Warning: `nexus42 sync` is deprecated. Use `nexus42 platform sync` instead. The top-level `sync` alias will be removed in a future version.
Sync Status:
  Error: sync error: outbox database error: migration failed: database migration failed: migration 20260418 was previously applied but has been modified
```

```text
$ ./target/debug/nexus42 sync pull 2>&1 | head -5
Warning: `nexus42 sync` is deprecated. Use `nexus42 platform sync` instead. The top-level `sync` alias will be removed in a future version.
Error: Operation 'sync pull' is not available in local_only mode.

  Suggestion: This operation requires platform connectivity. Switch to `local_first` or `cloud_enhanced` mode with `nexus42 system config set runtime_mode <mode>`.
```

### Root command visibility check

```text
$ ./target/debug/nexus42 --help > /tmp/help.txt && grep -E "^  (creator|daemon|acp|platform|system|sync)\\b" /tmp/help.txt
  creator   Manage Creator entities (register, pair, credentials, workspace, soul, memory, kb)
  daemon    Manage the daemon runtime
  acp       ACP capability plane (agents, registry, skills, connectivity)
  platform  Platform interaction (auth, explore, context, publish, **sync**)
  system    System management (presets, diagnostics, config, identity, etc.)
```

### V1.34 command smoke checks

```text
$ ./target/debug/nexus42 daemon --help | head -10
Manage the daemon runtime

Usage: nexus42 daemon [OPTIONS] <COMMAND>

Commands:
  start     Start the daemon runtime
  stop      Stop the running daemon
  restart   Restart the daemon (stop then start)
  status    Check daemon status / health
  logs      View daemon logs

$ ./target/debug/nexus42 creator --help | head -10
Manage Creator entities (register, pair, credentials, workspace, soul, memory, kb)

Usage: nexus42 creator [OPTIONS] <COMMAND>

Commands:
  register     Register a new Creator entity
  status       Show current Creator status
  use          Switch the active Creator
  list         List all registered Creators
  pair         Initiate pairing flow with a Creator

$ ./target/debug/nexus42 acp --help | head -10
ACP capability plane (agents, registry, skills, connectivity)

Usage: nexus42 acp [OPTIONS] <COMMAND>

Commands:
  status      Show daemon and ACP agent status
  doctor      Run ACP connectivity diagnostics
  probe       Verify ACP connectivity (registry or agent handshake)
  registry    ACP registry management
  agent       Agent selection and discovery

$ ./target/debug/nexus42 system --help | head -10
System management (presets, diagnostics, config, identity, etc.)

Usage: nexus42 system [OPTIONS] <COMMAND>

Commands:
  preset        Show registered system presets
  version       Print CLI version info
  doctor        Diagnostic health checks
  completion    Generate shell completion script
  config        Configuration file management
```

### Shell completion help

```text
$ ./target/debug/nexus42 system completion --help 2>&1 | head -15
Generate shell completion script

Usage: nexus42 system completion [OPTIONS] <SHELL>

Arguments:
  <SHELL>  Shell type (bash, zsh, fish, elvish, powershell)

Options:
  -v, --verbose                 Enable verbose logging
  -o, --output <OUTPUT_FORMAT>  Output format (text or json) [default: text]
  -h, --help                    Print help
  -V, --version                 Print version
```

## GitNexus impact evidence

QA did not modify source code. For the reviewed implementation surface, pre-report impact checks showed low blast radius for indexed CLI entry symbols:

- `PlatformCommand` (`crates/nexus42/src/commands/platform/mod.rs`): risk LOW, 0 direct callers/processes affected in index.
- `main` (`crates/nexus42/src/main.rs`): risk LOW, 0 direct callers/processes affected in index.
- `Cli` struct (`crates/nexus42/src/cli.rs`): risk LOW, 0 direct callers/processes affected in index.

## Findings

No blocking findings.

### Notes

- `sync status` reaches the legacy handler after printing the deprecation warning. In this local QA environment, the handler reports an existing local database migration mismatch. This does not block the P2 acceptance criterion under test because the required deprecation warning is visible before handler output and the alias remains callable.
- `sync pull` reaches the handler after printing the deprecation warning. It reports `local_only` platform-connectivity limitation in this environment, which is consistent with a callable command path rather than a clap routing failure.

## Not tested

- Full cloud/platform connectivity for `sync pull`/`push` was not exercised; the assignment scope required CLI IA, alias routing, help surface, and local CI gates.
- Full workspace-wide `cargo test --all` / `cargo clippy --all` were not run; the assignment explicitly required scoped `nexus42` checks.

## Recommended owners

- None. No required follow-up actions from QA.

## Summary

| Severity | Count |
|----------|-------|
| Critical | 0 |
| Warning | 0 |
| Suggestion | 0 |

**Verdict**: Approve
