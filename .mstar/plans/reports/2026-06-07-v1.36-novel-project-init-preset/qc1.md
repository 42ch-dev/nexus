---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-06-07-v1.36-novel-project-init-preset"
verdict: "Request Changes"
generated_at: "2026-06-07T10:53:56Z"
revalidated_at: "2026-06-07T10:53:56Z"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Runtime Model: openai/gpt-5.5
- Review Perspective: Architecture coherence and maintainability risk
- Report Timestamp: 2026-06-07T09:59:43Z

## Scope
- plan_id: `2026-06-07-v1.36-novel-project-init-preset`
- Review range / Diff basis: `merge-base: iteration/v1.36` (commit `1856258`) + `tip: feature/v1.36-novel-project-init-preset` (commit `2a97858`)
- Working branch (verified): `feature/v1.36-novel-project-init-preset`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.36-p1-init`
- Files reviewed: 30 changed files in diff; full reads of new preset YAML, new scaffold capability, new `work_chapters` DB module, new migrations, new hermetic tests, and CLI spec/run-command deltas.
- Commit range: `iteration/v1.36..feature/v1.36-novel-project-init-preset` (`1856258..2a97858`)
- Tools run:
  - `git rev-parse --show-toplevel && git branch --show-current && git log --oneline iteration/v1.36..HEAD`
  - `git diff iteration/v1.36..feature/v1.36-novel-project-init-preset --stat`
  - `git diff iteration/v1.36..feature/v1.36-novel-project-init-preset`
  - `cargo +nightly clippy -p nexus-orchestration -p nexus42 -p nexus-local-db -- -D warnings`

## Findings

### 🔴 Critical

- **C-001 — `novel-project-init` never invokes the scaffold capability it is supposed to deliver.** The preset declares only `creator.inject_prompt` and `acp.prompt` as required capabilities and its three states run only ACP prompt inner graphs; there is no `kind: capability` node for `novel.project_scaffold`, and `novel.project_scaffold` is not listed under `requires_capabilities`. As a result, a successful init conversation can reach `done` without creating `Works/<work_ref>/`, seeding `work_chapters`, or PATCHing `works`, violating the plan acceptance criteria and novel-workflow-profile §5.4. Fix by wiring an explicit post-confirmation capability step (with validated/context-bound inputs) before terminal completion, and include `novel.project_scaffold` in the preset capability requirements.
  - Evidence: `crates/nexus-orchestration/embedded-presets/novel-project-init/preset.yaml:22-36`, `:38-68`, `:69-112`; `crates/nexus-orchestration/src/capability/builtins/novel_scaffold.rs:121-288`.

- **C-002 — Scaffold protocol is not atomic despite claiming/specifying atomicity.** `NovelProjectScaffold::run` creates directories and writes files first, then opens independent DB operations (`seed_chapters` transaction, then `patch_work`). If `seed_chapters` or `patch_work` fails after filesystem writes succeed, the workspace is left with a partial scaffold but missing/incorrect DB state. This directly contradicts novel-workflow-profile §5.4.3 and the plan T2/T3/T4 requirement that mkdir/copy + chapter inserts + work PATCH succeed or fail together with rollback. This also creates a P2 hazard: filesystem gates may appear satisfied while `work_chapters` or `works` are inconsistent. Fix by introducing a workspace transaction/rollback guard (or equivalent compensating delete list) that coordinates filesystem writes with a single DB transaction for chapter seed + work patch.
  - Evidence: `crates/nexus-orchestration/src/capability/builtins/novel_scaffold.rs:143-228` performs filesystem writes before DB mutation; `:229-278` performs separate DB seed/patch; `crates/nexus-local-db/src/work_chapters.rs:56-86` scopes only chapter inserts to its own transaction.

### 🟡 Warning

- **W-001 — CLI `--init-preset` scheduling seam does not pass usable Work context to the init preset.** After creating the Work, the CLI posts an `AddScheduleRequest` with `creator_id: String::new()` and only `seed: Some(idea.clone())`. The preset comments require `work_id` and `initial_idea`, while the scaffold capability requires `creator_id`, `work_id`, `work_ref`, `title`, and `total_planned_chapters`. Even after C-001 is fixed, this scheduling path does not supply the context needed to bind the init run to the just-created Work. Fix by using the created Work's real creator/work identity and passing/binding the required preset input/context through the orchestration schedule path, or by deferring schedule creation until the engine has a typed Work-bound init invocation surface.
  - Evidence: `crates/nexus42/src/commands/creator/run.rs:207-247`; `crates/nexus-orchestration/embedded-presets/novel-project-init/preset.yaml:10-12`; `crates/nexus-orchestration/src/capability/builtins/novel_scaffold.rs:20-35`.

### 🟢 Suggestion

- **S-001 — The `work_chapters` seam is useful for P2 but should stay aligned with DB conventions.** The module is clearly scoped and documents the V1.37 PK extension path, but it uses runtime `sqlx::query()` for static SQL because `.sqlx` has not been prepared in this cycle. This is acceptable as a temporary implementation note with existing `// SAFETY:` comments, but after the schema stabilizes P2 should convert these static queries to compile-time checked macros or refresh `.sqlx` metadata to match `crates/nexus-local-db/AGENTS.md`.
  - Evidence: `crates/nexus-local-db/AGENTS.md:9-12`; `crates/nexus-local-db/src/work_chapters.rs:64-82`, `:98-107`, `:132-139`.

## Source Trace

- Finding ID: C-001
  - Source Type: git-diff + manual-reasoning
  - Source Reference: `preset.yaml` contains only prompt inner graphs; `novel_scaffold.rs` capability is registered but not invoked by the preset.
  - Confidence: High
- Finding ID: C-002
  - Source Type: manual-reasoning + spec-check
  - Source Reference: spec §5.4.3 atomicity vs `novel_scaffold.rs` filesystem-before-DB flow and `work_chapters::seed_chapters` isolated transaction.
  - Confidence: High
- Finding ID: W-001
  - Source Type: git-diff + manual-reasoning
  - Source Reference: `creator/run.rs` init schedule request fields vs preset/capability input contracts.
  - Confidence: High
- Finding ID: S-001
  - Source Type: doc-rule + manual-reasoning
  - Source Reference: local-db AGENTS compile-time query rule and current runtime query comments.
  - Confidence: Medium

## Checklist Notes
- Spec alignment: blocked by C-001 and C-002 for §5.4 scaffold execution/atomicity; gates field itself matches §5.3.1 shape.
- Module boundaries: new capability and DB module are readable and locally scoped, but the preset-to-capability seam is missing.
- Reuse vs duplication: registration follows built-in registry patterns; the hand-rolled template replacement is acceptable for limited placeholders but should not expand beyond this capability without reusing the engine's template system.
- Extension surface: `work_chapters` and `works` columns provide the P2 database seam, but partial-scaffold states would make P2 reconciliation harder unless C-002 is fixed.
- Pre-existing clippy concerns: required scoped clippy passed; no evidence that earlier `tasks/mod.rs` / `worker/registry.rs` concerns are P1 regressions.
- Future-proofing: preset `gates:` syntax aligns with orchestration-engine §7.9 and novel-workflow-profile §5.3.1; actual gate evaluator coverage remains a documented future seam in tests.
- `cli-spec.md`: flag documentation style is broadly consistent, but implementation semantics are blocked by W-001/C-001.

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 2 |
| 🟡 Warning | 1 |
| 🟢 Suggestion | 1 |

**Verdict**: Request Changes

## Revalidation

Targeted re-review for QC Reviewer #1 findings only, against post-fix tip `a8060f4` on `feature/v1.36-novel-project-init-preset` with diff basis `merge-base: iteration/v1.36` (commit `1856258`) + `tip: feature/v1.36-novel-project-init-preset` (commit `a8060f4`).

### Rechecked Evidence

- Checkout alignment:
  - `git rev-parse --show-toplevel` → `/Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.36-p1-init`
  - `git branch --show-current` → `feature/v1.36-novel-project-init-preset`
  - `git log --oneline iteration/v1.36..HEAD | head -15` confirmed post-fix tip `a8060f4` and fix commits F6 `7dea65a`, F2 `ec4032b`, F7 `6d95c9a`.
- F6 (`7dea65a`) reviewed with `git show 7dea65a --stat` and `git show 7dea65a -- crates/nexus-orchestration/embedded-presets/novel-project-init/preset.yaml`.
- F2 (`ec4032b`) reviewed with `git show ec4032b --stat` and source/test diff reads for `novel_scaffold.rs` and `novel_project_init.rs`.
- F7 (`6d95c9a`) reviewed with `git show 6d95c9a --stat` and source/test/status diffs for `run.rs`, daemon schedule handler, CLI command-surface test, and `.mstar/status.json`.
- Residual check: `rg 'R-V136P1-01' .mstar/status.json` found the registered residual.
- Validation:
  - `cargo +nightly clippy -p nexus-orchestration -p nexus42 -p nexus-local-db -- -D warnings` → passed.
  - `cargo test -p nexus-orchestration --test novel_project_init` → passed, 19 tests.

### Finding Disposition

- **C-001 — Closed.** F6 adds `novel.project_scaffold` to `preset.requires_capabilities` and inserts a `committing` state before `done` with `enter: kind: capability`, `name: novel.project_scaffold`. The args include `creator_id`, `work_id`, `work_ref`, `title`, `world_id`, `total_planned_chapters`, and `fields_changed`, satisfying the architectural requirement that the preset actually invokes the scaffold capability before terminal completion.

- **C-002 — Partial / Not closed.** F2 implements an FS-side `ScaffoldTransaction` rollback guard and adds `t7g_db_failure_rolls_back_filesystem_scaffold`, which is a useful improvement and does verify partial filesystem cleanup on a `seed_chapters` FK failure. However, the original critical required coordinating filesystem writes with **a single DB transaction for chapter seed + work patch** (or equivalent full atomic protocol). The F2 implementation explicitly documents that T3 and T4 still run their own internal transactions and that rows produced by T3 may remain if T4 fails. The new test does **not** assert sentinel preservation plus no orphan `work_chapters` row; it only forces T3 to fail before any rows can be seeded and asserts filesystem rollback. Therefore the cross-store / DB atomicity portion of C-002 remains unresolved.

- **W-001 — Acceptable partial closure with residual.** F7 fixes the empty `creator_id` bug by resolving `config.active_creator_id` into `resolved_creator_id` and using it for init, intake, and chained schedule requests. It adds a daemon `tracing::warn!` for `*-init` schedules without populated input context, includes a `run.rs` comment block cross-linking `R-V136P1-01`, and adds `v136_creator_run_start_has_init_preset_flag`. The remaining full `preset.input.*` wire plumbing is correctly registered as open residual `R-V136P1-01` in `.mstar/status.json`, so W-001 is acceptable under the PM's Option C rule.

### Revalidation Summary

| Finding | Status | Blocking? |
|---------|--------|-----------|
| C-001 | Closed | No |
| C-002 | Partial / Not closed | Yes |
| W-001 | Acceptable partial closure with residual | No |

**Revalidation Verdict**: Request Changes — C-002 remains partially unresolved because the fix wave does not implement a single DB transaction for T3 + T4 (nor an equivalent full atomic DB protocol), and the targeted atomicity test does not prove no orphan `work_chapters` rows after a post-seed failure.
