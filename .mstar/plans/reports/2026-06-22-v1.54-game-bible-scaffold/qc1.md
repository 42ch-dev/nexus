---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-06-22-v1.54-game-bible-scaffold"
verdict: "Request Changes"
generated_at: "2026-06-20"
revalidated_at: "2026-06-20"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Runtime Model: openai/gpt-5.5
- Review Perspective: Architecture coherence and maintainability risk
- Report Timestamp: 2026-06-20T00:00:00Z
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus`
- Working branch (verified): `iteration/v1.54`
- P1 implementation HEAD reviewed: `eacc6b49bb41388ba6450224b7faa9ea8c3a0489`
- Pre-report branch HEAD (after parallel qc2 report landed): `18afbd715b3dd2fc5f38f8ea054df33ba5adad33`
- Merge base: `4e26305b876170a51841ca8d36b027dbc20f03f0`

## Scope
- plan_id: `2026-06-22-v1.54-game-bible-scaffold`
- Review range / Diff basis: `merge-base: origin/main` + `tip: iteration/v1.54 HEAD` (P1 game-bible scaffold merged into integration; P0 already reviewed and approved separately — out of scope)
- Working branch (verified): `iteration/v1.54`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus`
- Files reviewed: 28
- Commit range: P1 commits `7a2a0317`, `2e207f29`, `3fab1b33`, `e8e8a425`, `f558e094`, `0e993c38`, `a9eb4e01`, `1dae4983`, `0c2fa9bf` within `4e26305b876170a51841ca8d36b027dbc20f03f0..HEAD`
- Tools run: `git rev-parse --show-toplevel`, `git branch --show-current`, `git rev-parse HEAD`, `git status --short`, `git log --oneline --decorate -20`, `git merge-base origin/main HEAD`, `git log --oneline --ancestry-path $(git merge-base origin/main HEAD)..HEAD`, `git show --stat --oneline --name-only <P1 commits>`, `cargo clippy --all -- -D warnings`, `cargo test --all`, targeted file reads/greps

**Scope (review)**:
- In: game-bible-profile.md Draft spec; 7 new BlockType variants; ValidationMode::GameBible; game-bible-init preset + GameBibleProjectScaffold capability; 12 Design templates; profile gates (is_novel_profile / is_game_bible_profile); bootstrap --profile game-bible; migration for work_profile CHECK constraint; entity-scope-model.md §5.1.1 amendments; cli-spec.md §12.1 docs; non-novel-profiles-roadmap.md update
- Out: P0 DF-46 write tools (already QC'd); P-last spec hygiene

## Findings

### 🔴 Critical
- **C-001 — Documented `--profile game-bible` path does not map to the persisted `game_bible` profile.** The assignment and game-bible spec acceptance require `nexus42 creator bootstrap --profile game-bible`, but `handle_bootstrap` matches only `"game_bible"` for preset derivation and passes the raw CLI string into `/v1/local/works` as `work_profile`. The migration CHECK accepts `('novel', 'essay', 'game_bible')`, not `game-bible`, so the documented command either creates the wrong derived preset path or fails the DB constraint. Fix by adding a single normalization layer (`game-bible` CLI spelling → `game_bible` stored profile) and test both spellings or explicitly update all specs/assignment-facing docs to `game_bible`.
- **C-002 — `game-bible-init` uses strict `{{preset.input.creator_id}}`, but bootstrap never seeds `creator_id` into `preset.input`.** The preset calls `game_bible.project_scaffold` with `creator_id: "{{preset.input.creator_id}}"`; the scheduler's capability arg rendering fails closed on missing placeholders, while `handle_bootstrap` seeds only `work_id`, `work_ref`, `title`, `total_planned_chapters`, and `world_id`. That means the generated init schedule can fail before reaching the scaffold capability. Fix by including `creator_id` (and preferably `initial_idea`, matching the preset header) in `init_input`, or change the preset/capability to use trusted `_creator_id` injection and remove the untrusted `creator_id` arg.

### 🟡 Warning
- **W-001 — Non-novel bootstrap still leaks the novel production auto-chain branch when `--skip-intake` is used.** `chain_novel_writing` defaults true and the `if chain_novel_writing && skip_intake` path schedules `primary_preset_id` with `novel_input` containing `chapter: 1`. For `game_bible`, the default `primary_preset_id` is `game-bible`, which has no production preset in V1.54 and violates the spec's “no auto-chain / no run-loop” rule. Gate this branch with `is_novel_profile`/normalized profile equality and add a regression test for `--profile game_bible --skip-intake` that proves no production schedule is attempted.

### 🟢 Suggestion
- **S-001 — Reuse the profile helpers across consumer crates instead of reintroducing literal profile checks.** The local-db helpers are exported, but daemon enrichment still checks `dto.work_profile.as_deref() != Some("novel")` directly. This is currently functionally correct, but it weakens the maintainability goal of centralizing profile string handling. Prefer `nexus_local_db::is_novel_profile(dto.work_profile.as_deref())` for consistency.

## Source Trace
- C-001
  - Source Type: manual-reasoning / doc-rule / git-diff
  - Source Reference: `.mstar/knowledge/specs/game-bible-profile.md:339,377`; `.mstar/knowledge/specs/non-novel-profiles-roadmap.md:54`; `crates/nexus42/src/commands/creator/bootstrap.rs:185-188,278-281,621-634`; `crates/nexus-local-db/migrations/202606220001_work_profile_game_bible.sql:26-27`
  - Confidence: High
- C-002
  - Source Type: manual-reasoning / static-analysis
  - Source Reference: `crates/nexus-orchestration/embedded-presets/game-bible-init/preset.yaml:8-14,45-50`; `crates/nexus42/src/commands/creator/bootstrap.rs:290-296`; `crates/nexus-orchestration/src/tasks/mod.rs:916-921`
  - Confidence: High
- W-001
  - Source Type: manual-reasoning / git-diff
  - Source Reference: `.mstar/knowledge/specs/game-bible-profile.md:339-341`; `crates/nexus42/src/commands/creator/bootstrap.rs:360-412`
  - Confidence: High
- S-001
  - Source Type: maintainability-review
  - Source Reference: `crates/nexus-local-db/src/works.rs:23-40`; `crates/nexus-local-db/src/lib.rs:129-130`; `crates/nexus-daemon-runtime/src/api/handlers/works.rs:721-733`
  - Confidence: Medium

## Validation Evidence
- Branch/cwd: `git rev-parse --show-toplevel` → `/Users/bibi/workspace/organizations/42ch/nexus`; `git branch --show-current` → `iteration/v1.54`; initial `git rev-parse HEAD` → `eacc6b49bb41388ba6450224b7faa9ea8c3a0489`; pre-report-commit `git rev-parse HEAD` → `18afbd715b3dd2fc5f38f8ea054df33ba5adad33` (parallel qc2 report commit only; P1 implementation commits unchanged).
- P1 commits present in review range: `7a2a0317`, `2e207f29`, `3fab1b33`, `e8e8a425`, `f558e094`, `0e993c38`, `a9eb4e01`, `1dae4983`, `0c2fa9bf`.
- `cargo clippy --all -- -D warnings` → pass.
- `cargo test --all` → pass (`760` nexus42 unit tests plus workspace integration/doc tests; final output reported no failures).
- Workspace note: pre-existing unstaged `.mstar/status.json` modification was present before this review; this report did not edit it.

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 2 |
| 🟡 Warning | 1 |
| 🟢 Suggestion | 1 |

## Verdict

**Verdict**: Request Changes

The scaffold structure, BlockType taxonomy, `ValidationMode::GameBible`, and local-db completion/reconcile gates are broadly coherent, but the public bootstrap path and init scheduling path are not reliable enough to approve. The two Critical items directly affect the advertised `creator bootstrap --profile game-bible` acceptance path.

## Revalidation

### Revalidation Scope
- Targeted re-review on `iteration/v1.54` HEAD `4abfd43b161b02245e2574fca57c4c4098bef20e`.
- Review range / Diff basis: `merge-base: origin/main` + `tip: iteration/v1.54 HEAD` (post fix-wave).
- Fix-wave commits checked: `07d39486` (C-001, C-002, W-001), `d5c4eb42`, `9bbf1e25`, `7427e8ff`, `ca948acf`.
- Worktree/cwd verified: `/Users/bibi/workspace/organizations/42ch/nexus`; branch verified: `iteration/v1.54`.
- Pre-existing workspace note: `.mstar/status.json` was already modified before this re-review; this report did not edit it.

### Finding-by-Finding Status
- **C-001 — Resolved.** `07d39486` normalizes the CLI spelling `game-bible` to canonical stored `game_bible` in `handle_bootstrap` before deriving presets and before POSTing `/v1/local/works`. Current code uses the normalized value for `work_profile`, `primary_preset_id`, and effective `game-bible-init` selection. Regression test verified: `commands::creator::bootstrap::tests::bootstrap_profile_game_bible_hyphen_parses`.
- **C-002 — Resolved.** `07d39486` adds both `creator_id` and `initial_idea` to `init_input`, so the `game-bible-init` preset's strict `{{preset.input.creator_id}}` / `{{preset.input.initial_idea}}` placeholders have seeded values. Targeted game-bible scaffold e2e tests also pass (`game_bible_scaffold_e2e.rs`).
- **W-001 — Resolved for the reviewed gate.** `07d39486` gates the direct production schedule branch with `profile == "novel"`, so normalized `game_bible` bootstrap with `--skip-intake` no longer enters the novel production auto-chain path. Regression test verified: `commands::creator::bootstrap::tests::bootstrap_game_bible_skip_intake_no_production_schedule`.
- **S-001 — Deferred / accepted.** The daemon enrichment helper still uses the local literal check (`dto.work_profile.as_deref() != Some("novel")`), but this remains a non-blocking maintainability suggestion and is accepted as deferred per PM assignment.

### Validation Evidence
- `git rev-parse --show-toplevel` → `/Users/bibi/workspace/organizations/42ch/nexus`; `git branch --show-current` → `iteration/v1.54`; `git rev-parse HEAD` → `4abfd43b161b02245e2574fca57c4c4098bef20e`.
- `git log --oneline -10` confirms fix-wave sequence through merge `4abfd43b` with `07d39486`, `d5c4eb42`, `9bbf1e25`, `7427e8ff`, and `ca948acf` present.
- `cargo clippy --all -- -D warnings` → pass.
- Targeted tests → pass:
  - `cargo test -p nexus42 bootstrap_profile_game_bible_hyphen_parses`
  - `cargo test -p nexus42 bootstrap_game_bible_skip_intake_no_production_schedule`
  - `cargo test -p nexus-orchestration --test game_bible_scaffold_e2e` (`bootstrap_game_bible_creates_design_tree`, `bootstrap_game_bible_idempotent`, `game_bible_work_status_json`, `game_bible_scaffold_with_world_id`).
- Mandatory workspace gates did **not** fully pass in this checkout:
  - `cargo +nightly fmt --all --check` → fail; rustfmt reports diffs in `crates/nexus-daemon-runtime/src/api/handlers/host_tool_executor.rs` and `crates/nexus-daemon-runtime/src/capability_registry.rs`.
  - `cargo test --all` → fail in `nexus-creator-memory` with 11 unrelated memory I/O / personality sync / review tests (examples: `memory_io::tests::update_memory_overwrites`, `personality_sync::tests::push_creates_valid_memory_with_frontmatter`, `review::tests::promote_truncates_oversized_raw_digest_at_utf8_boundary`).

### Revalidation Verdict

The four qc1 findings are resolved or accepted-deferred as listed above. However, because this assignment explicitly required clean `cargo +nightly fmt --all --check` and green `cargo test --all`, and both mandatory workspace gates failed on `iteration/v1.54` HEAD, this targeted re-review remains **Request Changes** rather than Approve.
