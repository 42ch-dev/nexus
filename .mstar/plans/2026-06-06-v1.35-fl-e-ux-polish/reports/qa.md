---
report_kind: qa
reviewer: qa-engineer
plan_id: 2026-06-06-v1.35-fl-e-ux-polish
verdict: Request Changes
generated_at: 2026-06-06T18:34:25Z
working_branch: feature/v1.35-fl-e-ux-polish
review_cwd: /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.35-p4
review_range: "merge-base: ef085b9; tip: 1b5fa75; equivalent: git diff ef085b9..1b5fa75"
---

# QA Report — V1.35 P4 FL-E UX Polish

## Scope tested

- Reviewer: `qa-engineer`
- Plan ID: `2026-06-06-v1.35-fl-e-ux-polish`
- Working branch: `feature/v1.35-fl-e-ux-polish`
- Review cwd: `/Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.35-p4`
- Review range / Diff basis: `merge-base: ef085b9` + `tip: 1b5fa75`; equivalent `git diff ef085b9..1b5fa75`
- Verified HEAD: `1b5fa75 harness(v1.35-p4): qc-consolidated Approve (PM override, qc-specialist re-review dispatch failed)`

## Findings

### Request Changes

1. **Required runtime smoke commands fail in this QA environment.**
   - Command: `./target/debug/nexus42 creator run start --idea "smoke" --chain-novel-writing=false`
   - Result: `Error: Network error: builder error`
   - Command: `./target/debug/nexus42 creator run start --idea "smoke" --chain-novel-writing=true`
   - Result: `Error: Network error: builder error`
   - Evidence: both variants fail identically before chain-specific behavior can be observed. Contract parsing tests pass, so this appears to be daemon/API reachability or local runtime configuration rather than a clap parsing failure; however the assignment explicitly requires these smoke commands to pass.

## Reproduction steps

From `/Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.35-p4`:

```bash
git rev-parse --show-toplevel
git branch --show-current
git log -1 --oneline
git diff ef085b9..HEAD --stat

cargo build -p nexus42
cargo test -p nexus42
cargo test -p nexus-daemon-runtime
cargo clippy -p nexus42 -p nexus-daemon-runtime -- -D warnings
cargo +nightly fmt --all -- --check

./target/debug/nexus42 creator run start --help
./target/debug/nexus42 creator run start --idea "smoke" --chain-novel-writing=false
./target/debug/nexus42 creator run start --idea "smoke" --chain-novel-writing=true

./target/debug/nexus42 creator run start --help | grep -A 6 "chain-novel-writing"
cargo test -p nexus42 --test command_surface_contract v135_chain 2>&1 | tail -8
grep -A 1 "DF-53" .mstar/knowledge/deferred-features-cross-version-tracker.md | head -10
grep "chain" .mstar/knowledge/specs/creator-workflow.md | head -5
./target/debug/nexus42 --help | head -20
./target/debug/nexus42 creator kb --help | head -10
./target/debug/nexus42 creator run start --help | head -25
```

## Evidence

### Pre-review alignment

- Repository root: `/Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.35-p4`
- Branch: `feature/v1.35-fl-e-ux-polish`
- HEAD: `1b5fa75 harness(v1.35-p4): qc-consolidated Approve (PM override, qc-specialist re-review dispatch failed)`
- Diff stat covered 7 files, including `crates/nexus42/src/commands/creator/run.rs`, `crates/nexus42/tests/command_surface_contract.rs`, plan/report artifacts, and knowledge/spec tracker updates.

### Build, test, lint, format gates

- `cargo build -p nexus42`: passed.
- `cargo test -p nexus42`: passed — 742 passed, 0 failed, 1 ignored.
- `cargo test -p nexus-daemon-runtime`: passed — 259 passed, 0 failed, 1 ignored. Rust warnings were emitted in daemon test targets, but the command completed successfully.
- `cargo clippy -p nexus42 -p nexus-daemon-runtime -- -D warnings`: passed.
- `cargo +nightly fmt --all -- --check`: passed.
- Targeted opt-out contract command: `cargo test -p nexus42 --test command_surface_contract v135_chain` passed — 2 passed, 0 failed.

### Help text accuracy

`./target/debug/nexus42 creator run start --help | grep -A 6 "chain-novel-writing"` shows:

- `Default true`
- opt-out syntax: ``--chain-novel-writing=false``
- accurate wording: `print the next-stage command for the user to run manually`
- future enhancement wording for full daemon `on_complete` auto-chain
- no overstated phrase `automatically chain into the production stage`

### Spec/tracker alignment

- `DF-53` tracker row includes: `V1.35 P4 partial: --chain-novel-writing defaults true (intake → produce); full multi-stage auto-chain deferred`.
- `creator-workflow.md` includes: `After V1.35 P4, creator run start chains intake → produce by default (--chain-novel-writing, default true); users may opt out with --chain-novel-writing=false`.

### No P2/P3 regression checks

- Root help shows the V1.35 command grouping with `creator`, `daemon`, `acp`, and `platform` visible in the captured first 20 lines; prior contract test `v135_root_help_shows_five_groups_with_sync_hidden` also passed.
- `creator kb --help` states TWO scopes: `--scope work` and `--scope world`.
- `creator run start --help` shows `--chain-novel-writing` in the first 25 help lines.

## Not tested

- A successful live `creator run start` end-to-end work creation path, because both assigned smoke invocations failed with `Network error: builder error` in this checkout/environment.
- Daemon-backed scheduling behavior after successful work creation.

## Acceptance criteria

| Criterion | Result | Notes |
| --- | --- | --- |
| Demo path requires fewer manual commands OR documents intentional explicit advance with improved hints | Fail | Help text documents explicit advance accurately, but assigned start smoke commands fail, so the demo path could not be verified end-to-end. |
| DF-53 tracker row updated | Pass | Tracker and creator workflow spec mention V1.35 P4 partial/default behavior. |
| Tests pass | Pass | Required build/test/clippy/fmt gates passed; targeted `v135_chain` tests passed. |

## Recommended owners

- `fullstack-dev` / PM: decide whether the runtime smoke failure is an expected environment precondition (daemon/API config) that should be documented in QA instructions, or a product/runtime issue that must be fixed before P4 can be accepted.

## Verdict

**Request Changes** — 2/3 acceptance criteria passed. All static/contract/help/spec checks passed, but the assignment required both `creator run start --idea "smoke"` smoke variants to pass, and both fail reproducibly with `Network error: builder error`.
