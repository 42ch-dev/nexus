---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: "2026-06-22-v1.54-game-bible-scaffold"
verdict: "Request Changes"
generated_at: "2026-06-20"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-3
- Runtime Agent ID: qc-specialist-3
- Runtime Model: kimi-for-coding/k2p7
- Review Perspective: Performance and reliability risk
- Report Timestamp: 2026-06-20T00:00:00Z
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus`
- Working branch (verified): `iteration/v1.54`
- Re-review HEAD: `4abfd43b161b02245e2574fca57c4c4098bef20e`
- Original review HEAD: `eacc6b49bb41388ba6450224b7faa9ea8c3a0489`
- Merge base: `4e26305b876170a51841ca8d36b027dbc20f03f0`

## Scope
- plan_id: `2026-06-22-v1.54-game-bible-scaffold`
- Review range / Diff basis: `merge-base: origin/main` + `tip: iteration/v1.54 HEAD`
- Working branch (verified): `iteration/v1.54`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus`
- Files reviewed: 13
- Commit range: `4e26305b876170a51841ca8d36b027dbc20f03f0..eacc6b49bb41388ba6450224b7faa9ea8c3a0489`
- Tools run: `git rev-parse --show-toplevel`, `git branch --show-current`, `git rev-parse HEAD`, `git merge-base origin/main HEAD`, `git diff --name-only`, targeted file reads/greps, `cargo clippy --all -- -D warnings`, `cargo test --all`, `cargo +nightly fmt --all --check`, `pnpm run codegen`

**Scope (review)**:
- In: template render performance, capability registration impact, migration safety, error observability in profile gate paths, atomicity of scaffold creation
- Out: P0; P-last

## Findings

### 🔴 Critical
(none — the architecture seat already raised the profile-spelling Critical in qc1 C-001; this seat corroborates it in the reliability discussion below and concurs with `Request Changes`.)

### 🟡 Warning
- **W-001 — `game_bible.project_scaffold` is not atomic: filesystem writes and the DB `works` PATCH are separate steps with no rollback guard.**
  `GameBibleProjectScaffold::run` creates directories, writes `README.md` and 12 `Design/*.md` files, and only then runs `UPDATE works SET work_profile = 'game_bible' ...`. If the DB update fails, the Work directory is left partially created and a retry may conflict or leave orphan artifacts. The module doc comment acknowledges this as deferred W-005, but no residual is registered in `status.json` and the code differs from `novel.project_scaffold`, which uses a `ScaffoldTransaction` with `Drop`-based rollback.
  - Fix: adopt the same `ScaffoldTransaction` rollback pattern used by the novel scaffold, or wrap the capability in a transactionally compensating helper; register the residual explicitly if deferring.
  - Source: `crates/nexus-orchestration/src/capability/builtins/game_bible_scaffold.rs:188-264`; `crates/nexus-orchestration/src/capability/builtins/novel_scaffold.rs:763-830` (contrast).

- **W-002 — `cargo +nightly fmt --all --check` fails; P1 files are among the unformatted files.**
  The CI gate (`cargo +nightly fmt --all --check`) reports formatting diffs on files touched by this plan:
  - `crates/nexus-orchestration/src/capability/builtins/game_bible_scaffold.rs`
  - `crates/nexus-kb/src/validation.rs`
  - `crates/nexus42/src/commands/creator/bootstrap.rs`
  - `crates/nexus-local-db/src/work_chapters.rs`
  Two additional files in the same diff range (`crates/nexus-daemon-runtime/src/api/handlers/host_tool_executor.rs`, `crates/nexus-daemon-runtime/src/capability_registry.rs`) are also unformatted but belong to the already-closed P0 scope; they still block CI and need a hygiene pass.
  - Fix: run `cargo +nightly fmt --all` and commit only the formatting changes.
  - Source: local run `cargo +nightly fmt --all --check` on `iteration/v1.54` @ `eacc6b49`.

- **W-003 — Public bootstrap spelling mismatch makes the advertised `--profile game-bible` path unreliable (corroboration of qc1 C-001).**
  The CLI help and specs advertise `--profile game-bible`, but the gate/migration/store value is `game_bible`. If a user passes the hyphenated form, the raw string is persisted and will fail the `work_profile` CHECK constraint and the `game-bible-init` preset gate. This makes the bootstrap path fail non-idempotently and is a reliability issue.
  - Fix: normalize the CLI spelling to the stored `game_bible` value before persisting, or restrict accepted values and update docs consistently.
  - Source: `crates/nexus42/src/commands/creator/bootstrap.rs:31-36,185-198`; `crates/nexus-local-db/migrations/202606220001_work_profile_game_bible.sql:26-27`; `crates/nexus-orchestration/embedded-presets/game-bible-init/preset.yaml:31-34`.

- **W-004 — Plan T10 e2e/integration tests for game-bible bootstrap are missing.**
  The verification plan lists `bootstrap_game_bible_creates_design_tree`, `bootstrap_game_bible_idempotent`, and `game_bible_work_status_json`, but no such tests exist under `crates/nexus42/tests/` or `crates/nexus-orchestration/tests/`. Only unit tests cover CLI parsing and `is_work_completed`. The actual capability execution path (preset → capability → filesystem → DB PATCH) is therefore unverified end-to-end.
  - Fix: add an orchestration or CLI integration test that runs `game_bible.project_scaffold` against a temporary workspace and asserts the 12 files + README + logs directories exist and that the `works` row is updated.
  - Source: `.mstar/plans/2026-06-22-v1.54-game-bible-scaffold.md:167-185`; `grep -R "bootstrap_game_bible\|game_bible_work_status" crates/` returned no matches.

### 🟢 Suggestion
- **S-001 — 12 Design templates are hardcoded inline rather than loaded via `include_str!`.**
  The plan's own risk table recommended `include_str!` from embedded templates to avoid duplication. The current implementation builds each stub with `format!` over a `const DESIGN_TEMPLATES` table. Runtime cost is negligible (12 tiny strings), but maintainability and parity with the stated mitigation would improve by moving templates to `embedded-presets/game-bible-init/templates/`.
  - Source: `crates/nexus-orchestration/src/capability/builtins/game_bible_scaffold.rs:52-140,242-252`.

- **S-002 — Profile gate paths lack `tracing::warn!` / audit observability.**
  `is_work_completed` silently returns `Ok(false)` for game-bible Works, and `reconcile_from_filesystem` returns a structured error but emits no log. Operators have no signal that novel-specific logic was intentionally skipped. Adding `tracing::info!` or `tracing::warn!` at these gate sites would aid debugging.
  - Source: `crates/nexus-local-db/src/work_chapters.rs:1187-1211` (`is_work_completed`); `crates/nexus-local-db/src/work_chapters.rs:606-639` (`reconcile_from_filesystem` gate).

- **S-003 — `work_profile` CHECK expansion is safe today but vulnerable to index drift on the next profile.**
  The migration correctly recreates all indexes present at V1.54, but the next profile that expands the CHECK will again need to remember every index added after this migration. A regression test that asserts the post-migration `works` table has the expected index set would catch drift early.
  - Source: `crates/nexus-local-db/migrations/202606220001_work_profile_game_bible.sql:56-82`.

## Source Trace
- **W-001**
  - Source Type: git-diff / static-analysis / manual-reasoning
  - Source Reference: `crates/nexus-orchestration/src/capability/builtins/game_bible_scaffold.rs:188-264`; `crates/nexus-orchestration/src/capability/builtins/novel_scaffold.rs:763-830`
  - Confidence: High
- **W-002**
  - Source Type: static-analysis (formatter)
  - Source Reference: `cargo +nightly fmt --all --check` output on `iteration/v1.54 @ eacc6b49`
  - Confidence: High
- **W-003**
  - Source Type: manual-reasoning / doc-rule / git-diff
  - Source Reference: `crates/nexus42/src/commands/creator/bootstrap.rs:31-36,185-198`; `crates/nexus-local-db/migrations/202606220001_work_profile_game_bible.sql:26-27`; `crates/nexus-orchestration/embedded-presets/game-bible-init/preset.yaml:31-34`
  - Confidence: High
- **W-004**
  - Source Type: doc-rule / static-analysis
  - Source Reference: `.mstar/plans/2026-06-22-v1.54-game-bible-scaffold.md:167-185`; `grep -R "bootstrap_game_bible\|game_bible_work_status" crates/`
  - Confidence: High
- **S-001**
  - Source Type: maintainability-review
  - Source Reference: `crates/nexus-orchestration/src/capability/builtins/game_bible_scaffold.rs:52-140,242-252`
  - Confidence: Medium
- **S-002**
  - Source Type: maintainability-review
  - Source Reference: `crates/nexus-local-db/src/work_chapters.rs:1187-1211`; `crates/nexus-local-db/src/work_chapters.rs:606-639`
  - Confidence: Medium
- **S-003**
  - Source Type: maintainability-review
  - Source Reference: `crates/nexus-local-db/migrations/202606220001_work_profile_game_bible.sql:56-82`
  - Confidence: Medium

## Validation Evidence
- Branch/cwd:
  - `git rev-parse --show-toplevel` → `/Users/bibi/workspace/organizations/42ch/nexus`
  - `git branch --show-current` → `iteration/v1.54`
  - `git rev-parse HEAD` → `eacc6b49bb41388ba6450224b7faa9ea8c3a0489`
  - `git merge-base origin/main HEAD` → `4e26305b876170a51841ca8d36b027dbc20f03f0`
- `cargo clippy --all -- -D warnings` → pass (`Finished dev profile` with no warnings/errors).
- `cargo test --all` → pass (all workspace suites green; final reported counts include 760+ unit tests plus integration/doc tests with 0 failures).
- `cargo +nightly fmt --all --check` → **fail**; P1 files among those needing reformat (see W-002).
- `pnpm run codegen` → pass; no generated-file diff after running.
- Pre-existing unstaged `.mstar/status.json` modification was present before this review; this report did not edit it.

## Revalidation (targeted re-review, fix-wave on iteration/v1.54)

Re-reviewed at `iteration/v1.54` HEAD `4abfd43b161b02245e2574fca57c4c4098bef20e` (merge commit `4abfd43b`).
Fix-wave commits examined: `07d39486` (C-001/C-002/W-001), `d5c4eb42` (W-002), `9bbf1e25` (W-004), plus follow-ups `7427e8ff` and `ca948acf`.

### Revalidation evidence

- Branch/cwd:
  - `git rev-parse --show-toplevel` → `/Users/bibi/workspace/organizations/42ch/nexus`
  - `git branch --show-current` → `iteration/v1.54`
  - `git rev-parse HEAD` → `4abfd43b161b02245e2574fca57c4c4098bef20e`
  - `git merge-base origin/main HEAD` → `4e26305b876170a51841ca8d36b027dbc20f03f0`
- `cargo clippy --all -- -D warnings` → pass (`Finished dev profile` with no warnings/errors).
- `cargo +nightly fmt --all --check` → fail. Remaining unformatted files: `crates/nexus-daemon-runtime/src/api/handlers/host_tool_executor.rs`, `crates/nexus-daemon-runtime/src/capability_registry.rs`.
- `cargo test --all` → fail. Three `nexus-creator-memory` tests fail under concurrent workspace execution due to hardcoded `/tmp/test_agg_exp_acp` path (pre-existing, see below).
- `cargo test -p nexus-creator-memory --lib` → pass (150/150; confirms failures are workspace-concurrency isolation, not deterministic crate failures).
- `cargo test -p nexus-orchestration --test game_bible_scaffold_e2e` → pass (4/4 e2e tests green).
- `pnpm run codegen` → not re-run; no schema changes were part of the fix wave.

### Per-finding disposition

- **W-001 — scaffold atomicity**: **Still open / deferred without residual SSOT.**
  The fix commit `07d39486` addresses a related but distinct concern (the bootstrap production-scheduling gate for non-novel profiles) and does not modify `GameBibleProjectScaffold`. The filesystem-DB non-atomicity originally reported remains. The Assignment states this is deferred to V1.55+, but `.mstar/status.json` root `residual_findings["2026-06-22-v1.54-game-bible-scaffold"]` is empty; no residual entry documents the deferral. Per `mstar-plan-artifacts`, open residuals must be registered in `status.json`. **Action required:** PM/QA must add a residual entry before the plan can be considered closed with residuals.

- **W-002 — nightly fmt**: **Partially resolved; CI gate still red.**
  Commit `d5c4eb42` correctly reformatted the P1 files originally listed in W-002 (`crates/nexus-orchestration/src/capability/builtins/game_bible_scaffold.rs`, `crates/nexus-kb/src/validation.rs`, `crates/nexus-local-db/src/work_chapters.rs`). However, `cargo +nightly fmt --all --check` still reports diffs in the P0 files also called out in the original finding:
  - `crates/nexus-daemon-runtime/src/api/handlers/host_tool_executor.rs`
  - `crates/nexus-daemon-runtime/src/capability_registry.rs`
  The CI formatter gate therefore still fails. **Action required:** run `cargo +nightly fmt --all` and commit the resulting P0-file formatting.

- **W-003 — profile spelling (corroboration of qc1 C-001)**: **Resolved.**
  Commit `07d39486` adds normalization in `crates/nexus42/src/commands/creator/bootstrap.rs:138-145`: CLI argument `--profile game-bible` is mapped to the canonical stored value `game_bible` before persistence and preset derivation. Two regression unit tests were added (`bootstrap_profile_game_bible_hyphen_parses`, `bootstrap_game_bible_skip_intake_no_production_schedule`) and pass.

- **W-004 — T10 e2e/integration tests**: **Resolved.**
  Commit `9bbf1e25` adds `crates/nexus-orchestration/tests/game_bible_scaffold_e2e.rs` with 4 hermetic e2e tests:
  1. `bootstrap_game_bible_creates_design_tree`
  2. `bootstrap_game_bible_idempotent`
  3. `game_bible_work_status_json`
  4. `game_bible_scaffold_with_world_id`
  All four pass when run with `cargo test -p nexus-orchestration --test game_bible_scaffold_e2e`.

- **S-001 — templates via `include_str!`**: **Accepted as deferred.** Not implemented in the fix wave; no functional impact. Note: not registered in `status.json` residuals for this plan.

- **S-002 — `tracing::warn!` observability**: **Accepted as deferred.** Not implemented; gate paths remain silent. Note: not registered in `status.json` residuals for this plan.

- **S-003 — index drift regression test**: **Accepted as deferred.** Not implemented. Note: not registered in `status.json` residuals for this plan.

### CI gate re-check

| Gate | Command | Result | Notes |
|------|---------|--------|-------|
| Clippy | `cargo clippy --all -- -D warnings` | ✅ pass | No warnings or errors. |
| Formatter | `cargo +nightly fmt --all --check` | ❌ fail | Only P0 files remain unformatted (see W-002). |
| Tests (all) | `cargo test --all` | ❌ fail | 3 failures in `nexus-creator-memory` (unrelated to P1 scope; see below). |
| Tests (P1 e2e) | `cargo test -p nexus-orchestration --test game_bible_scaffold_e2e` | ✅ pass | 4/4 e2e tests green. |

#### `cargo test --all` failure details

Three tests in `nexus-creator-memory` fail when the entire workspace test suite runs concurrently:

- `experience_aggregation::tests::aggregate_experience_with_synthesizer_success`
- `memory_io::tests::save_creates_parent_dirs`
- `memory_io::tests::save_and_load_roundtrip`

Failure mode is hardcoded temp-path collision: `experience_aggregation.rs:479` uses `std::path::PathBuf::from("/tmp/test_agg_exp_acp")`, which can conflict across concurrent or repeated runs. `git blame` shows this code dates to `d7a973fdb` (2026-05-21), well before the V1.54 P1 scope. When the same three tests are run individually or the whole crate is run in isolation (`cargo test -p nexus-creator-memory --lib`), they pass. This is therefore a **pre-existing workspace-level test-isolation flake**, not caused by the game-bible scaffold fix wave. It does, however, prevent the required `cargo test --all` command from passing and should be tracked as a separate hygiene residual or fixed before merge if the iteration requires a green `--all` gate.

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 2 |
| 🟢 Suggestion | 3 (deferred) |

Unresolved warnings:
- **W-001**: scaffold atomicity still open; deferred status not registered in `status.json`.
- **W-002**: `cargo +nightly fmt --all --check` still fails on P0 files (`host_tool_executor.rs`, `capability_registry.rs`).

Additional CI blocker (pre-existing, out-of-scope):
- `cargo test --all` fails due to `nexus-creator-memory` hardcoded temp-path test isolation (see Revalidation section).

## Verdict

**Verdict**: Request Changes

The fix wave resolves the profile-spelling reliability issue (W-003, corroborating qc1 C-001) and delivers the planned T10 e2e coverage (W-004). However, two originally reported warnings remain unaddressed:

1. **W-001** — the scaffold atomicity concern is still present in `GameBibleProjectScaffold`, and its deferral to V1.55+ is not captured in the canonical `status.json` residual tracker.
2. **W-002** — the workspace formatter gate still fails because the P0-scope files (`host_tool_executor.rs`, `capability_registry.rs`) were left out of the formatting pass.

In addition, the required `cargo test --all` command fails on pre-existing workspace test isolation in `nexus-creator-memory`; while unrelated to P1 changes, it blocks the iteration-level CI gate and must be dispositioned before merge.

Approval is blocked until W-002 is resolved (a one-command `cargo +nightly fmt --all` pass) and either W-001 is registered as a residual or the atomicity issue is fixed. The pre-existing `cargo test --all` failure should be verified against `origin/main` and tracked separately if confirmed pre-existing.
