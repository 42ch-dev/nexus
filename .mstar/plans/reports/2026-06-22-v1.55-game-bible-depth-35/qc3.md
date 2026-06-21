---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: "2026-06-22-v1.55-game-bible-depth-35"
verdict: "Request Changes"
generated_at: "2026-06-21"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-3
- Runtime Agent ID: qc-specialist-3
- Runtime Model: OpenCode/k2p7
- Review Perspective: Performance and reliability risk (P2 focused)
- Report Timestamp: 2026-06-21T12:00:00+08:00

## Scope
- plan_id: `2026-06-22-v1.55-game-bible-depth-35`
- Review range / Diff basis: `merge-base: origin/main` (`9f5298e4`) + `tip: iteration/v1.55 HEAD` (`0718a6fe`); P2 commit `fb298429` only
- Working branch (verified): `iteration/v1.55`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus`
- Files reviewed: 9 (P2-only diff)
- Commit range: `9f5298e4..0718a6fe`, narrowed to P2 implementation commit `fb2984292a36e12db1898bef8d7a718f224cb24b`
- Tools run: `cargo test -p nexus-local-db -p nexus-orchestration -p nexus-daemon-runtime`, `cargo clippy -p nexus-local-db -p nexus-orchestration -p nexus-daemon-runtime -- -D warnings`, `cargo test -p nexus-orchestration --test game_bible_scaffold_e2e`, GitNexus context/impact queries

## Findings

### 🔴 Critical
- None.

### 🟡 Warning

#### W-1: `tracing::info!` on `get_work` hot path for every game-bible section evaluation
- **Issue**: `is_game_bible_design_complete` (called synchronously from the `get_work` HTTP handler) emits `tracing::info!` for each critical section evaluated and for the final verdict. This violates the acceptance criterion "no info-level on hot path" and creates unbounded log volume proportional to `get_work` call frequency.
- **Fix**: Downgrade the per-section and verdict events inside `is_game_bible_design_complete` to `tracing::debug!`. Keep `tracing::info!` only for the actual state-change event (work auto-promoted to `completed`), which already exists in `works.rs`.
- **Machine severity**: `high`
- **Source**: `crates/nexus-local-db/src/work_chapters.rs:1327-1399`, `crates/nexus-daemon-runtime/src/api/handlers/works.rs:727-773`

#### W-2: Synchronous blocking filesystem I/O inside async `get_work` handler
- **Issue**: `is_game_bible_design_complete` calls `std::fs::read_to_string` for up to three `Design/*.md` files on every `get_work` invocation for an incomplete game-bible Work. Because this runs in the daemon's async request handler, it blocks the async runtime thread for the duration of disk reads. On local SSD the latency is small, but it is unbounded for large design documents and degrades under I/O pressure.
- **Fix**: Use `tokio::fs::read_to_string` (or `spawn_blocking`) for the file reads, or cache the completion verdict in the `works` row and invalidate it via preset stage transitions rather than re-evaluating on every read.
- **Machine severity**: `medium`
- **Source**: `crates/nexus-local-db/src/work_chapters.rs:1355-1370`

#### W-3: Key P2 deliverables are defined and tested but not wired into production code paths
- **Issue**: Three P2-scope artifacts exist only as tested library code or as an embedded preset file; none are reachable from production flows:
  1. `quality_loop::design_five_q_check` — never called by the `design-writing` preset or any other code; the preset relies solely on the LLM judge.
  2. `quality_loop::block_type_to_game_bible_category` — never called; the V1.51 KB extraction path (`extract_kb_candidates_for_review` / `extract_candidates_from_text`) remains novel-only and does not use this mapping.
  3. `DESIGN_WRITING_PRESET_ID` / the new `design-writing` embedded preset — validated by `all_embedded_presets_pass_strict_validation_gate`, but not registered in `STAGE_PRESET_ALLOWLIST` and not referenced by any supervisor/admission code, so it cannot advance through the FL-E stage chain.
- **Fix**: Either (a) wire these into the production paths (preferred), or (b) update the plan acceptance criteria and residual findings to track the unwired surface as deferred scope. If deferred, remove/comment the dead code or mark it `#[doc(hidden)]` with a residual pointer to avoid shipping untested integration paths.
- **Machine severity**: `medium`
- **Source**: `crates/nexus-orchestration/src/quality_loop.rs:789-820`, `1422-1544`, `crates/nexus-orchestration/src/preset_ids.rs:78-88`, `crates/nexus-orchestration/src/preset/validation.rs:1784-1797`

### 🟢 Suggestion

#### S-1: Add handler-level integration test for game-bible auto-promotion
- **Issue**: The only tests for `is_game_bible_design_complete` are unit tests in `nexus-local-db`. There is no test that exercises the `get_work` handler's game-bible auto-promotion branch, including the "already completed" short-circuit and the workspace-path-empty guard.
- **Fix**: Add an integration test in `nexus-daemon-runtime` (or an orchestration E2E test) that calls the `get_work` handler with a game-bible Work whose critical Design sections are accepted and verifies the status flips to `completed`.
- **Machine severity**: `low`

#### S-2: Consider memoizing design-completion status to avoid repeated file reads
- **Issue**: `get_work` re-reads and re-parses the same three files on every call until promotion. For a CLI-driven status UX this is acceptable today, but it does not scale to frequent polling.
- **Fix**: Store a denormalized `design_complete_evaluated_at`/`design_complete_verdict` column or rely on preset stage transitions to set `works.status = 'completed'` only when the section is accepted, removing the need for read-time evaluation.
- **Machine severity**: `low`

#### S-3: Document why `design-writing` is absent from `STAGE_PRESET_ALLOWLIST`
- **Issue**: The preset docstring states it is "Registered as a `work_continue` preset" and consumed by `STAGE_PRESET_ALLOWLIST`, but the allowlist still only maps `produce` to `["novel-writing"]`.
- **Fix**: Add `design-writing` to the `produce` stage allowlist (e.g., `["design-writing", "novel-writing"]` for game-bible vs novel) or add a code comment explaining the intended stage routing.
- **Machine severity**: `low`

## Source Trace

| Finding ID | Source Type | Source Reference | Confidence |
|------------|-------------|------------------|------------|
| W-1 | manual-reasoning + acceptance-criteria | `work_chapters.rs` tracing calls inside `is_game_bible_design_complete`; caller `works.rs:get_work` | High |
| W-2 | manual-reasoning + static-analysis | `std::fs::read_to_string` inside `async fn is_game_bible_design_complete` | High |
| W-3 | static-analysis + grep | No non-test call sites for `design_five_q_check`, `block_type_to_game_bible_category`, or `DESIGN_WRITING_PRESET_ID`; `STAGE_PRESET_ALLOWLIST` unchanged | High |
| S-1 | manual-reasoning | No test files call the daemon `get_work` handler with `work_profile = game_bible` | High |
| S-2 | manual-reasoning | `get_work` calls `is_game_bible_design_complete` unconditionally for incomplete game-bible works | High |
| S-3 | static-analysis | `preset/validation.rs:1784-1797` | High |

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 3 |
| 🟢 Suggestion | 3 |

**Verdict**: Request Changes

Rationale: W-1 violates an explicit acceptance criterion (info-level tracing on the `get_work` hot path) and W-3 leaves three P2 deliverables unwired, meaning the design 五问 rubric and game-bible KB extraction path are not actually reused or exercised in production despite being tested in isolation. W-2 is a reliability concern for the same hot path. These must be resolved or explicitly deferred with residual tracking before P2 can be approved from a performance/reliability perspective.

## Verification Evidence

```text
$ git rev-parse --show-toplevel && git branch --show-current && git rev-parse HEAD
/Users/bibi/workspace/organizations/42ch/nexus
iteration/v1.55
0718a6fe4f65898e67a5fa6145f90f6a9f476d2a

$ git merge-base origin/main HEAD
9f5298e4ec4c9376a22d99ebb7af38e92186b5f5

$ cargo clippy -p nexus-local-db -p nexus-orchestration -p nexus-daemon-runtime -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] in 22.05s

$ cargo test -p nexus-local-db -p nexus-orchestration --lib -- game_bible design_five_q block_type_to_game_bible_category design_writing_preset_id
running 18 tests
... ok

$ cargo test -p nexus-orchestration --test game_bible_scaffold_e2e
running 4 tests
... ok

$ cargo test -p nexus-daemon-runtime --lib
running 247 tests
... ok
```
