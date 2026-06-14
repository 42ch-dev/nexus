---
report_kind: qc-review
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: 2026-06-13-v1.45-creator-run-generic-runner
secondary_plan_ids:
  - 2026-06-13-v1.45-delete-bespoke-run-subcommands
  - 2026-06-13-v1.45-creator-bootstrap-and-works-migration
verdict: Approve
generated_at: 2026-06-13T16:40:43Z
review_range: merge-base: 76a9eb79; tip: HEAD (ad7b5565); equivalent: git diff 76a9eb79...HEAD
working_branch: iteration/v1.45
review_cwd: /Users/bibi/workspace/organizations/42ch/nexus
---

# Code Review Report — QC #3 (Performance / Reliability)

## Reviewer Metadata
- Reviewer: @qc-specialist-3
- Runtime Agent ID: qc-specialist-3
- Runtime Model: kimi-for-coding/k2p7
- Review Perspective: performance, reliability, and resource-lifecycle risk of the V1.45 B1 atomic merge (P0 generic runner + P1 subcommand deletion + P2 bootstrap/works migration)
- Report Timestamp: 2026-06-13T16:40:43Z

## Scope
- plan_id: 2026-06-13-v1.45-creator-run-generic-runner
- secondary_plan_ids:
  - 2026-06-13-v1.45-delete-bespoke-run-subcommands
  - 2026-06-13-v1.45-creator-bootstrap-and-works-migration
- Review range / Diff basis: merge-base: 76a9eb79; tip: HEAD (ad7b5565); equivalent: `git diff 76a9eb79...HEAD`
- Working branch (verified): iteration/v1.45
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 38 files changed (key paths: `crates/nexus42/src/commands/creator/run.rs`, `bootstrap.rs`, `works/mod.rs`, `mod.rs`; `crates/nexus-orchestration/src/preset/mod.rs`, `user_preset_dir.rs`, `system_preset_dir.rs`, `stage_gates.rs`, `preset/validation.rs`; `crates/nexus-contracts/src/local/orchestration/preset.rs`; `crates/nexus42/tests/command_surface_contract.rs`)
- Commit range: 76a9eb79..ad7b5565
- Tools run:
  - `cargo +nightly fmt --all -- --check` — passed
  - `cargo clippy --all -- -D warnings` — passed
  - `cargo test -p nexus42 --lib` — 665 passed in 30.23s
  - `cargo test -p nexus42 --test command_surface_contract` — 37 passed in 1.42s
  - `cargo test -p nexus-orchestration --lib preset` — 207 passed

## Findings

### Critical
None.

### Warning

#### W-1 — Generic non-FL-E runner re-scans and reloads all user/system presets on every invocation
**Issue:** In `crates/nexus42/src/commands/creator/run.rs`, `handle_run` calls `nexus_orchestration::preset::resolve_preset` for every non-FL-E preset dispatch. `resolve_preset` (in `crates/nexus-orchestration/src/preset/mod.rs`) performs:
1. `user_preset_dir::scan_user_presets(...)` — reads the entire `~/.nexus42/presets/` directory and loads/validates every user preset.
2. `system_preset_dir::scan_system_presets(...)` — reads the entire `~/.nexus42/presets/_system/` directory and loads/validates every system preset.
3. Falls back to embedded presets.

The scan helpers only build an in-memory index for the lifetime of the `ScanResult`; they do **not** cache across `resolve_preset` calls. Additionally, `handle_run` builds a fresh `CapabilityRegistry::with_builtins()` on every invocation. The whole path executes synchronously inside an `async fn` without `spawn_blocking`.

**Impact:** Each `nexus42 creator run <preset_id>` for a non-FL-E preset is O(N) in the number of user/system presets, including filesystem directory enumeration, YAML file reads, parsing, and validation. For creators with many presets this adds avoidable latency and blocks the async runtime thread during I/O.

**Evidence:**
- `crates/nexus42/src/commands/creator/run.rs:130-140`
- `crates/nexus-orchestration/src/preset/mod.rs:139-174`
- `crates/nexus-orchestration/src/user_preset_dir.rs:76-162`
- `crates/nexus-orchestration/src/system_preset_dir.rs:72-136`

**Fix:**
- Short-term: resolve a specific preset ID via direct bundle path lookup (`~/.nexus42/presets/<id>/preset.yaml`) before falling back to a full scan, so the common case is O(1).
- Longer-term: introduce a process-lifetime (or daemon-side) preset cache keyed by `(source, id, mtime)` and reuse a single `CapabilityRegistry` instance, or move the resolution into `spawn_blocking`.

---

#### W-2 — `stage_advance` rollback ignores the result of the restoring PATCH
**Issue:** In `crates/nexus42/src/commands/creator/run.rs`, when schedule creation after a stage advance fails, the code attempts to roll the Work back to its previous stage/status but discards the result:

```rust
let _ = client
    .patch::<serde_json::Value, _>(&format!("/v1/local/works/{work_id}"), &rollback)
    .await;
```

**Impact:** If the stage PATCH succeeds (Work is now `active` at the target stage) and the subsequent schedule POST fails, the Work is left without a driver schedule. If the rollback PATCH also fails, the caller receives only the schedule-failure error and the Work remains in the inconsistent `active` state. This is a reliability regression compared to the atomic semantics the rollback comment implies.

**Evidence:**
- `crates/nexus42/src/commands/creator/run.rs:2064-2076`

**Fix:** Either propagate the rollback failure as a secondary error (e.g., append it to the returned error message) or return a dedicated error that includes both the schedule failure and the rollback failure, so operators can detect and remediate the orphaned active stage.

### Suggestion

#### S-1 — Announce hard deletion of legacy `creator run` subcommands in release notes
V1.44 callers/scripts using `creator run start`, `continue`, `stage`, `resume`, `reconcile-chapters`, `audit-chapter`, or `review-master` will break. Compass §0.1 #9 mandates hard delete, but a short migration note in release notes (`creator bootstrap`, `creator works <subcommand>`, `creator run <preset_id>`) will reduce user friction.

#### S-2 — `parse_preset_cli_args` can pre-size its collections
The two `HashMap`s created in `parse_preset_cli_args` use default capacity. Since `cli_args` is already known, use `HashMap::with_capacity(cli_args.len())` for the lookup and parsed maps to avoid small rehashes.

**Evidence:**
- `crates/nexus42/src/commands/creator/run.rs:219-223`

#### S-3 — `sanitize_for_terminal` recompiles the ANSI regex on every call
`works status` calls `sanitize_for_terminal` for each finding title, hint, and work_id. The function compiles `r"\x1B\[[0-9;]*[a-zA-Z]"` on every invocation. Cache the regex with `once_cell::sync::Lazy` or `std::sync::OnceLock`.

**Evidence:**
- `crates/nexus42/src/commands/creator/works/mod.rs:1337-1343`

#### S-4 — World KB block assembly runs on every world-bound stage advance
`stage_advance` opens the local state DB and queries the KB store to assemble `world_kb_block` for every `produce`/`research`/`review`/`persist` advance on a world-bound Work. For large worlds this can become expensive and the call uses `max_tokens: None`. Consider bounding the query or caching the block when the underlying KB rows have not changed.

**Evidence:**
- `crates/nexus42/src/commands/creator/run.rs:1978-1995`

## Source Trace

| Finding ID | Source Type | Source Reference | Confidence |
|------------|-------------|------------------|------------|
| W-1 | static-analysis / manual-reasoning | `crates/nexus42/src/commands/creator/run.rs:130-140`, `crates/nexus-orchestration/src/preset/mod.rs:139-174` | High |
| W-2 | static-analysis / manual-reasoning | `crates/nexus42/src/commands/creator/run.rs:2064-2076` | High |
| S-1 | doc-rule / manual-reasoning | Compass §0.1 #9, deleted `RunCommand` variants in `run.rs` | High |
| S-2 | static-analysis | `crates/nexus42/src/commands/creator/run.rs:219-223` | High |
| S-3 | static-analysis | `crates/nexus42/src/commands/creator/works/mod.rs:1337-1343` | High |
| S-4 | static-analysis | `crates/nexus42/src/commands/creator/run.rs:1978-1995` | Medium |

## Summary

| Severity | Count |
|----------|-------|
| Critical | 0 |
| Warning | 2 |
| Suggestion | 4 |

**Verdict:** Approve

All required CI gates pass. No Critical findings. The two Warnings are performance/reliability residuals that should be tracked for a follow-up slice (direct preset lookup / process cache for W-1; rollback error propagation for W-2). The generic runner and atomic `works`/`bootstrap` commands meet the V1.45 B1 merge bar with no correctness blockers.
