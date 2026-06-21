---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: "2026-06-22-v1.55-script-scaffold"
verdict: "Approve"
generated_at: "2026-06-22"
---

# Code Review Report — V1.55 P3 (Script Scaffold) Performance / Reliability

## Reviewer Metadata
- Reviewer: @qc-specialist-3
- Runtime Agent ID: qc-specialist-3
- Runtime Model: k2p7
- Review Perspective: Performance and reliability risk (scaffold atomicity, memory/throughput bounds, codegen sync, tracing, resource lifecycle tests, graceful degradation)
- Report Timestamp: 2026-06-22T15:00:00Z

## Scope
- plan_id: `2026-06-22-v1.55-script-scaffold`
- Review range / Diff basis: `merge-base: origin/main` (`9f5298e4`) + `tip: iteration/v1.55 HEAD` (`c30cdd48`); P3 commits only
- Working branch (verified): `iteration/v1.55`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus`
- P3 commits reviewed:
  - `59ad649a` feat(v1.55-P3): add script BlockType taxonomy + ValidationMode::Script
  - `4eb88c20` feat(v1.55-P3): implement script scaffold + --profile script CLI
  - `08f2c37c` fix(v1.55-P3): apply ScaffoldTransaction to game_bible_scaffold (R-V154P1-W001)
  - `4a545ab1` docs(v1.55-P3): update specs + status.json for script scaffold closure
  - `c30cdd48` merge(v1.55): integrate P3 — script scaffold
- Files reviewed: 9 (core code + generated contracts)
  - `crates/nexus-orchestration/src/capability/builtins/script_scaffold.rs`
  - `crates/nexus-orchestration/src/capability/builtins/game_bible_scaffold.rs`
  - `crates/nexus-orchestration/src/capability/mod.rs`
  - `crates/nexus-orchestration/tests/capability_registry.rs`
  - `crates/nexus-orchestration/embedded-presets/script-init/preset.yaml`
  - `crates/nexus-kb/src/validation.rs`
  - `crates/nexus-kb/src/errors.rs`
  - `crates/nexus42/src/commands/creator/bootstrap.rs`
  - `schemas/common/common.schema.json` + generated Rust/TypeScript outputs
- Tools run:
  - `git rev-parse --show-toplevel` / `git branch --show-current` / `git rev-parse HEAD`
  - `git merge-base origin/main HEAD`
  - `git log --oneline --reverse <merge-base>..HEAD`
  - `git diff --name-status` / `git diff` for P3 range
  - `pnpm run codegen`
  - `cargo clippy -p nexus-orchestration -p nexus-kb -p nexus42 -p nexus-contracts -- -D warnings`
  - `cargo test -p nexus-orchestration -p nexus-kb -p nexus42 -p nexus-contracts`
  - `cargo +nightly fmt --all --check`
  - GitNexus `impact` on `ValidationMode`, `handle_bootstrap`
  - GitNexus `detect_changes` against P3 base (`9b3d70ce`)

## Findings

### 🔴 Critical
None.

### 🟡 Warning
None.

### 🟢 Suggestion

#### S1 — Synchronous `std::fs` I/O inside async capability `run` blocks the executor
- **Severity (machine):** `low`
- **Scope:** `crates/nexus-orchestration/src/capability/builtins/script_scaffold.rs:204-315` and `game_bible_scaffold.rs:195-293`
- **Evidence:** Both scaffolds call `std::fs::create_dir_all`, `std::fs::write`, and `std::fs::remove_file` directly inside `async fn run`. The capability is invoked from the async orchestration graph, so these calls block the async runtime thread for the duration of each FS syscall. Template files and directories are small, so a single Work creation is fast, but concurrent scaffold schedules contend for executor threads.
- **Impact:** Low today — typical Work creation is a one-shot, low-frequency operation and files are small. Risk rises if the daemon later runs multiple init presets concurrently or if the Works root is on a slow/network filesystem.
- **Recommended fix:** Offload FS work to `tokio::task::spawn_blocking` (or use `tokio::fs` if the crate already depends on tokio) and return the `ScaffoldTransaction` state across the boundary. This is a structural improvement, not a P3 blocker; can be deferred to a future scaffold-hardening pass.

#### S2 — `ScaffoldTransaction` comment overstates file idempotency for re-init
- **Severity (machine):** `low`
- **Scope:** `script_scaffold.rs:88-147`, `game_bible_scaffold.rs:296-347` (and inherited from `novel_scaffold.rs` pattern)
- **Evidence:** The doc comment says "Files/dirs that pre-existed (e.g. re-init over a partially-scaffolded tree) are left untouched." Directories are guarded by `if !dir.exists()` before pushing to `dirs_created`, but files (`README.md`, template files) are written unconditionally and pushed to `files_created`. If a file pre-existed and the scaffold later fails before `commit()`, the `Drop` rollback will delete the pre-existing file.
- **Impact:** Low in normal single-shot usage; higher if users re-run `script-init` over an existing Work with user-edited README/template files.
- **Recommended fix:** Either (a) check file existence before writing and only push newly-created files to `files_created`, or (b) update the comment to accurately describe the current guarantee (directories only) and accept the inherited novel-scaffold behavior. If (a) is chosen, apply consistently to novel/game-bible/script scaffolds.

#### S3 — No integration test for pool-backed DB PATCH path or DB-failure rollback
- **Severity (machine):** `low`
- **Scope:** `script_scaffold.rs:278-285`, `game_bible_scaffold.rs:252-262`
- **Evidence:** Both scaffolds have unit tests for `ScaffoldTransaction` rollback/commit and for directory-tree creation using `pool: None`. There is no test that exercises the `if let Some(ref pool) = self.pool { sqlx::query!("UPDATE works ...") }` path or verifies that a DB write failure triggers FS rollback via the transaction guard.
- **Impact:** Low — the `ScaffoldTransaction` guard covers the failure path structurally, and the SQL is a single bound UPDATE. A regression in error mapping or pool interaction would only be caught at integration/e2e time.
- **Recommended fix:** Add one hermetic test per scaffold that constructs the capability with an in-memory `SqlitePool`, runs the scaffold, and asserts the `works` row was updated. Optionally inject a failure (e.g., bad work_id) and assert rollback cleans up files.

#### S4 — `ValidationMode::Script` and `script_category` mapping lack dedicated unit tests
- **Severity (machine):** `low`
- **Scope:** `crates/nexus-kb/src/validation.rs` test module
- **Evidence:** The test module covers Generic, Novel, and GameBible modes extensively (positive, negative, and structured-error cases). `ValidationMode::Script` is implemented symmetrically but has no dedicated tests. The `validation_mode_display` test also omits `Script`. `is_valid_script_category` and `default_block_type_for_script_category` are exercised only indirectly.
- **Impact:** Low — the implementation mirrors the well-tested GameBible path, so structural correctness is high. Dedicated tests would guard against future drift when the script taxonomy expands.
- **Recommended fix:** Add script-mode tests mirroring the GameBible suite: positive acceptance of `dialogue`/`beat`/`act`, rejection of `novel_category` and `game_bible_category`, missing/invalid/non-string `script_category`, and structured-error kind assertions. Also add `ValidationMode::Script` to `validation_mode_display`.

## Source Trace

| Finding ID | Source Type | Source Reference | Confidence |
|------------|-------------|------------------|------------|
| S1 | manual-reasoning | `script_scaffold.rs:204-315` (`async fn run` with `std::fs` calls) | High |
| S2 | manual-reasoning | `script_scaffold.rs:245-275` (unconditional file writes + push to tx.files_created); `Drop` impl at L118-146 | High |
| S3 | manual-reasoning | `script_scaffold.rs:278-285` (pool-backed UPDATE); test module L318-452 (no pool tests) | High |
| S4 | manual-reasoning | `validation.rs` test module L505-1019 (no Script-mode tests); `validation_mode_display` at L928-932 | High |

## Performance / Reliability Analysis

### Scaffold atomicity
- P3 closes `R-V154P1-W001` by wrapping FS writes + DB PATCH in a `ScaffoldTransaction` with `Drop`-based rollback for both `game_bible.project_scaffold` and `script.project_scaffold`.
- Rollback removes files, then directories in reverse order. Directories created by this invocation are tracked correctly (`if !dir.exists()` guard).
- The DB UPDATE is still best-effort relative to FS: if the process crashes after the SQL succeeds but before `tx.commit()`, the FS was already written and the guard will not run on restart. This is the same limitation documented in the novel-scaffold pattern and is acceptable for local-first, single-user daemon usage.

### Memory / throughput bounds
- Template content is static (`&'static str`); per-invocation allocations are limited to a few small `String`s, fixed-size `Vec<PathBuf>` (bounded by template/directory count), and the serde output value.
- No unbounded loops, recursion, or dynamic growth based on user input.
- File write throughput is acceptable for typical Work creation: ~7 small directories and ~4 small files for script, ~12 small files for game-bible.

### Codegen / contract sync
- `pnpm run codegen` produced no diff in `crates/nexus-contracts/src/generated/` or `packages/nexus-contracts/src/generated/`.
- `enum_conversions.rs` was updated manually to include the new `BlockType` variants (`Dialogue`, `Beat`, `Act`).
- Schema change is additive only; no existing variant was renamed or removed.

### Tracing / observability
- Scaffold start and completion are logged at `info!` with `work_id` and `work_ref`.
- Rollback failures are logged at `warn!` with path and error.
- Validation advisory mismatches are logged at `warn!`.
- Error paths are observable; no silent swallowing was found.

### Graceful degradation
- Invalid scaffold input returns `CapabilityError::InputInvalid`.
- FS write failures return `CapabilityError::Internal` with path context.
- DB PATCH failures return `CapabilityError::Internal`; the transaction guard cleans up FS on drop.
- CLI bootstrap preserves existing error handling for force-gates, reason length, and schedule creation failures (logged as warnings, non-fatal).

## CI Gates

| Gate | Command | Result |
|------|---------|--------|
| Codegen | `pnpm run codegen` | **No diff** on generated directories |
| Clippy (touched crates) | `cargo clippy -p nexus-orchestration -p nexus-kb -p nexus42 -p nexus-contracts -- -D warnings` | **Clean** |
| Tests (nexus-kb) | `cargo test -p nexus-kb` | **37 passed**, 0 failed |
| Tests (nexus-orchestration) | `cargo test -p nexus-orchestration` | All binaries passed, 0 failed |
| Tests (nexus42) | `cargo test -p nexus42` | All binaries passed, 0 failed |
| Tests (nexus-contracts) | `cargo test -p nexus-contracts` | All binaries passed, 0 failed |
| Format | `cargo +nightly fmt --all --check` | **Clean** |

## GitNexus Impact

- `ValidationMode` upstream impact: **LOW** — 0 direct callers outside the KB crate.
- `handle_bootstrap` upstream impact: **LOW** — 1 direct caller (`creator/mod.rs::run`), affecting the CLI `creator` command surface.
- `detect_changes` (P3 range `9b3d70ce..c30cdd48`) reports **high** summary count because the merge touches 25 files / 40 symbols, but affected processes are cross-community registration flows (`CapabilityRegistry.with_builtins`, `ValidationError` consumers) and the bootstrap `run` process. No high-risk execution flows were identified for the perf/reliability focus.

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 4 |

**Verdict**: Approve

All performance/reliability acceptance criteria are satisfied: scaffold rollback is implemented and tested, memory growth is bounded, file write throughput is acceptable, codegen is in sync, tracing is present, and resource-lifecycle tests exist (with the coverage gaps noted above as low-severity suggestions). The open suggestions are improvements and test-coverage expansions, not blockers for this P3 merge.

---

## Completion Report v2

**Agent**: qc-specialist-3  
**Task**: QC tri-review (performance/reliability) for V1.55 P3 — script scaffold  
**Status**: Done  
**Scope Delivered**: Full review of P3 diff range on `iteration/v1.55` (`c30cdd48`), focused on scaffold atomicity, memory/throughput, codegen sync, tracing, and resource-lifecycle test coverage.  
**Artifacts**: `.mstar/plans/reports/2026-06-22-v1.55-script-scaffold/qc3.md` (this file)  
**Validation**:
- cwd/branch/range verified via `git rev-parse`, `git branch --show-current`, `git merge-base origin/main HEAD`
- `pnpm run codegen` produced no diff on generated dirs
- `cargo clippy -p nexus-orchestration -p nexus-kb -p nexus42 -p nexus-contracts -- -D warnings` clean
- `cargo test -p nexus-orchestration -p nexus-kb -p nexus42 -p nexus-contracts` all passed
- `cargo +nightly fmt --all --check` clean
- GitNexus impact attempted on `ValidationMode` and `handle_bootstrap` (LOW risk)
- GitNexus `detect_changes` against `9b3d70ce` reviewed

**Issues/Risks**: Four low-severity suggestions (S1–S4) around async FS I/O, file-idempotency doc accuracy, pool-backed DB PATCH tests, and script-mode validation tests. No Critical/Warning findings in the perf/reliability focus.  
**Plan Update**: N/A (reviewer; no plan edits).  
**Handoff**: PM to consolidate qc1/qc2/qc3. Note that qc2 identified a Critical path-validation finding; that is outside the perf/reliability focus but will likely drive a fix-wave before merge.  
**Git**: Report committed to `iteration/v1.55`; exact hash in assistant Completion Report v2.

**Reviewer alignment note**: Review performed in strict leaf-executor mode per `mstar-dispatch-gates` + `qc-specialist-shared`. No subagent dispatch, no code changes, no `status.json` writes. Report only under `{PLAN_DIR}/reports/.../qc3.md`.
