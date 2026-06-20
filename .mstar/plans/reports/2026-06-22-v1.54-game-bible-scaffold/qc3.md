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
- Implementation HEAD reviewed: `eacc6b49bb41388ba6450224b7faa9ea8c3a0489`
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

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 4 |
| 🟢 Suggestion | 3 |

## Verdict

**Verdict**: Request Changes

From the performance/reliability seat the implementation is structurally sound: the 23-entry `CapabilityRegistry` lookup remains O(1), template rendering is low overhead, the `work_profile` CHECK expansion preserves data and indexes, and the profile gates correctly short-circuit novel logic for game-bible Works. However, the scaffold capability is not all-or-nothing (W-001), the CI formatter gate fails on P1 files (W-002), the public bootstrap spelling path is unreliable (W-003, corroborating qc1 C-001), and the end-to-end bootstrap path is missing the integration tests called out in the plan (W-004). These unresolved warnings block approval.
