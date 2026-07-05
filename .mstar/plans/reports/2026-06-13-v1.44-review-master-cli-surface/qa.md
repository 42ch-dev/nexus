# QA Report (Report-only)

## Scope tested

- **Agent**: qa-engineer
- **Plan ID**: `2026-06-13-v1.44-review-master-cli-surface`
- **Working branch (verified)**: `iteration/v1.44`
- **Review cwd (verified)**: `/Users/bibi/workspace/organizations/42ch/nexus`
- **Review range / Diff basis**: `9d471bdc..a9262c33`
- **Current HEAD during QA**: `b023a357` (`a9262c33` verified as ancestor of `HEAD`)
- **QA mode**: Report-only; no implementation code modified.
- **Verdict**: Approve

This QA verified V1.44 P1 after fix-wave merge `a9262c33` and post-fix QC re-review Approve by `qc-specialist` and `qc-specialist-3`. Scope was limited to R-V143P0-002 (`creator run review-master <work_id>` CLI surface + quickstart/spec convergence) and excluded P0/P2 behavior except where the integrated branch log contains interleaved commits.

### Acceptance Criteria mapping

| AC | Result | Evidence |
| --- | --- | --- |
| AC1: `nexus42 creator run review-master --help` documents flags from spec §3.4 | Pass | Help output includes `<WORK_ID>`, `--finding-id`, `--auto-schedule`, `--json`, `novel-review-master`, `reflection-loop` distinction, and quickstart §5 reference. |
| AC2: Quickstart §5 updated to primary command path, with `stage advance --stage review` demoted | Pass | `docs/novel-writing-quickstart.md:171-181` presents `review-master` as primary path and demotes `stage advance --stage review` to an FL-E review-stage note for generating findings. |
| AC3: Hermetic CLI test lists master findings + enqueues preset with `--finding-id` | Pass for implemented hermetic scope | `cargo test -p nexus42 --test review_master_cli` passed 5/5. Tests assert help surface, quickstart discoverability, `novel-review-master` distinction, and no `auto_chain` help reference. Code inspection confirms `--finding-id` enqueues `preset_id: "novel-review-master"` and rejects non-`master` findings. |
| AC4: R-V143P0-002 `lifecycle: resolved` in `status.json` | Pass | `.mstar/status.json:1438-1452` has `id: R-V143P0-002`, `lifecycle: resolved`, and resolution plan/commit metadata. |
| AC5: Command does not fork/cancel FL-E auto-chain driver | Pass | Help test asserts no `auto-chain`/`auto_chain` in `review-master` help; code path uses `/v1/local/orchestration/schedules` with `preset_id: "novel-review-master"` and does not call auto-chain driver helpers or mutate auto-chain state. |

## Findings

No blocking QA findings.

### Non-blocking observations

- `cargo test -p nexus42 --test integration review_master` ran successfully but selected **0 tests** because `crates/nexus42/tests/integration.rs` has no `review_master`-named subset. Full `cargo test -p nexus42 --test integration` was run and passed 50/50.
- `git log --oneline 9d471bdc..a9262c33` includes interleaved P0/P2/QC commits because this is the integrated `iteration/v1.44` branch range. QA constrained behavioral assessment to the P1 files and ACs; a path-scoped log confirms the expected P1 original/fix commits plus merge and surrounding integration commits.

## Reproduction steps

From repo root `/Users/bibi/workspace/organizations/42ch/nexus` on branch `iteration/v1.44`:

```bash
git rev-parse --show-toplevel
git branch --show-current
git merge-base --is-ancestor a9262c33 HEAD
git log --oneline 9d471bdc..a9262c33
PATH="$PWD/target/debug:$PATH" nexus42 creator run review-master --help
cargo test -p nexus42 --test review_master_cli
cargo test -p nexus42 --test integration
cargo test -p nexus42 --test integration review_master
cargo clippy -p nexus42 -- -D warnings
cargo +nightly fmt --all --check
```

Manual checks:

```bash
# Quickstart §5 primary path and FL-E demotion
# Read docs/novel-writing-quickstart.md:143-181

# R-V143P0-002 lifecycle closure
# Read .mstar/status.json:1438-1452

# Code path review for AC5
# Read crates/nexus42/src/commands/creator/run.rs:154-175 and 786-1065
```

## Evidence

### Checkout alignment

```text
/Users/bibi/workspace/organizations/42ch/nexus
iteration/v1.44
b023a357
```

`git merge-base --is-ancestor a9262c33 HEAD` succeeded.

### `git log --oneline 9d471bdc..a9262c33`

```text
a9262c33 merge(v1.44 P1): fix wave — review-master correctness + spec convergence for QC Request Changes
44a12a6e merge(v1.44 P0): fix wave — split preset + harden CLI for QC Request Changes
a5a9bd7e fix(v1.44): R-V144P1-003/004/005/006 review-master correctness and hygiene
fc9f2f6d fix(v1.44): R-V144P0-006,009 plan verification fix + CLI integration tests
3297d925 fix(v1.44): R-V144P0-002..005,007,008 CLI handler hardening
d6b9400e fix(v1.44): R-V144P0-001 split preset into review + extract, fix CLI dispatch
9e953abd fix(v1.44): R-V144P1-001/002 update cli-command-ia.md and novel-writing/author-experience.md for review-master
1ecd8358 qa(v1.44 P2): qa-engineer acceptance verification
226731d9 qc(v1.44 Wave 1): consolidate tri-review reports (P0 RC, P1 RC, P2 Approve)
9d59b895 qc(v1.44 P1): qc-specialist architecture review
b0ec45cc qc(v1.44 P2): qc-specialist architecture review
9f5f609a qc(v1.44 P2): qc-specialist-3 performance review
3f963183 qc(v1.44 P0): qc-specialist-3 performance review
16e8e703 qc(v1.44 P0): qc-specialist architecture review
888dc423 qc(v1.44 P1): qc-specialist-3 performance review
c6dd3058 qc(v1.44 P1): qc-specialist-2 security review
35e38b0b qc(v1.44 P2): qc-specialist-2 security review
a0858b17 qc(v1.44 P0): qc-specialist-2 security review
4e3399b4 harness(v1.44): mark Wave 1 (P0/P1/P2) InReview post-merge
9c53d8f6 merge(v1.44 P2): multi-volume completion + supervisor volume propagation
c54b1aa6 merge(v1.44 P1): review-master CLI surface + spec convergence
73850ddb test(P1-T6): hermetic review_master_cli tests + close R-V143P0-002
b7d27aa7 style(P2): nightly fmt fixes after T4 regression tests
031da6ae docs(P1-T4+T5): update quickstart §5 + spec amendments for review-master
233bc3f2 test(P2-T4): add multi-volume completion + volume propagation regression tests
a7b70ebf feat(P1-T1): add creator run review-master CLI subcommand
22324ddc fix(P2-T1/T2/T3): harden multi-volume completion + thread volume through supervisor chain
```

### `nexus42 creator run review-master --help`

Command used:

```bash
PATH="$PWD/target/debug:$PATH" nexus42 creator run review-master --help
```

Output:

```text
Run the master-decision review on open findings (V1.44 P1).

Lists open findings with `target_executor=master` and optionally enqueues the `novel-review-master` preset for a specific finding. Distinct from `stage advance --stage review` which runs the `reflection-loop` FL-E review stage.

See docs/novel-writing-quickstart.md §5 for usage patterns.

Usage: nexus42 creator run review-master [OPTIONS] <WORK_ID>

Arguments:
  <WORK_ID>
          Work ID (wrk_...) to review

Options:
      --finding-id <FINDING_ID>
          Run review-master preset scoped to a specific finding

      --auto-schedule
          Opt-in: enqueue novel-review-master when this Work has stale (96h+) findings. Scoped to the supplied `work_id` — only stale findings belonging to this Work trigger the schedule

      --json
          Emit machine-readable JSON instead of human text

  -v, --verbose
          Enable verbose logging

  -o, --output <OUTPUT_FORMAT>
          Output format (text or json)
          
          [default: text]

  -h, --help
          Print help (see a summary with '-h')

  -V, --version
          Print version
```

### `cargo test -p nexus42 --test review_master_cli`

```text
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.32s
     Running tests/review_master_cli.rs (target/debug/deps/review_master_cli-dbec9ad25d71b794)

running 5 tests
test review_master_help_shows_expected_flags ... ok
test review_master_help_distinguishes_from_stage_advance ... ok
test creator_run_help_lists_review_master ... ok
test review_master_help_references_quickstart_section_5 ... ok
test review_master_help_does_not_mention_auto_chain ... ok

test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.75s
```

### `cargo test -p nexus42 --test integration`

```text
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.20s
     Running tests/integration.rs (target/debug/deps/integration-bc16b1ce55fd9e41)

running 50 tests
...
test result: ok. 50 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.24s
```

### `cargo test -p nexus42 --test integration review_master`

```text
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.25s
     Running tests/integration.rs (target/debug/deps/integration-bc16b1ce55fd9e41)

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 50 filtered out; finished in 0.00s
```

### `cargo clippy -p nexus42 -- -D warnings`

```text
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.20s
```

### `cargo +nightly fmt --all --check`

```text
(no output; command exited successfully)
```

### Quickstart §5 evidence

`docs/novel-writing-quickstart.md:171-181`:

```text
# Primary path — run the master-decision review on open findings
nexus42 creator run review-master <work_id>

# List master findings (default), then enqueue the review for a specific finding:
nexus42 creator run review-master <work_id> --finding-id <finding_id>

# Opt-in: auto-schedule review when stale findings exist:
nexus42 creator run review-master <work_id> --auto-schedule
...
`review-master` enqueues the `novel-review-master` preset ... distinct from `creator run stage advance --stage review`, which runs the `reflection-loop` FL-E review stage ...
```

### Status residual closure evidence

`.mstar/status.json:1438-1452`:

```text
"id": "R-V143P0-002",
...
"lifecycle": "resolved",
"resolution": {
  "commit": "a7b70ebf",
  "plan_id": "2026-06-13-v1.44-review-master-cli-surface",
  "note": "Shipped creator run review-master CLI subcommand (T1), quickstart §5 updated (T4), spec amendments (T5), hermetic tests green (T6). Archive in P-last."
}
```

### Code-path evidence for AC5

`crates/nexus42/src/commands/creator/run.rs:154-175` documents the `ReviewMaster` subcommand and flags. `run.rs:868-970` enqueues `preset_id: "novel-review-master"` for `--finding-id` after re-validating `target_executor == "master"`; `run.rs:973-1061` scopes stale findings to `work_id` and enqueues the same preset for `--auto-schedule`. The reviewed path does not call auto-chain driver APIs or mutate auto-chain state.

## Not tested

- No live daemon end-to-end schedule execution was performed in this report-only QA pass.
- No P0/P2 feature behavior was tested beyond avoiding regressions in scoped `nexus42` commands.
- No browser/UI validation applies.

## Recommended owners

- **No required fixes** for V1.44 P1.
- Optional future owner: `@fullstack-dev` for post-V1.44 refinement if the team wants a daemon-side `target_executor=master` findings filter or a named `review_master` integration subset.
