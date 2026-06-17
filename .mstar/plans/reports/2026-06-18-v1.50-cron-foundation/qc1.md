---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: 2026-06-18-v1.50-cron-foundation
working_branch: feature/v1.50-cron-foundation
review_cwd: /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v150-cron-foundation
review_range: merge-base c38fbe1f264b9574b25355d872d20138c1c04e77..a7ea8349260fa7c8cc5be0f586fa9f84d13549ee
verdict: Request Changes
generated_at: 2026-06-17T05:59:00Z
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Runtime Model: zhipuai-coding-plan/glm-5.2
- Review Perspective: architecture coherence + maintainability (primary); shared baseline coverage for regression, security/correctness, performance, test adequacy
- Report Timestamp: 2026-06-17T05:59:00Z

## Scope
- plan_id: 2026-06-18-v1.50-cron-foundation
- Review range / Diff basis: merge-base c38fbe1f264b9574b25355d872d20138c1c04e77..a7ea8349260fa7c8cc5be0f586fa9f84d13549ee
- Working branch (verified): feature/v1.50-cron-foundation
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v150-cron-foundation
- Files reviewed: 9 source/test/config files in the diff (`migrations/202606180001_works_schedule_json.sql`, `nexus-local-db/src/works.rs`, `nexus-local-db/tests/works_schedule_migration.rs`, `nexus42/src/commands/creator/works/cron.rs`, `nexus42/src/commands/creator/works/mod.rs`, `nexus42/tests/cron_cli.rs`, `Cargo.toml`, `crates/nexus42/Cargo.toml`, `Cargo.lock`) + plan `.mstar/plans/2026-06-18-v1.50-cron-foundation.md` + spec overlay `.mstar/knowledge/specs/novel-writing/cron-staggering.md` + existing `nexus-local-db/src/works.rs::get_work` / `LocalDbError` for convention checks
- Commit range: 5 commits `cdceac31` (T1) → `a7ea8349` (T7 HEAD), matching the Assignment `Review range` exactly
- Tools run: `git rev-parse --show-toplevel`, `git branch --show-current`, `git log --oneline c38fbe1f..HEAD` (5 commits verified), `git diff c38fbe1f..HEAD --stat` (10 files, +1711/-8), `git diff` per-crate, `cargo build -p nexus-local-db -p nexus42`, `cargo clippy -p nexus-local-db -p nexus42 --lib -- -D warnings` (clean), `cargo clippy -p nexus-local-db -p nexus42 --all-targets -- -D warnings` (2 in-scope + many pre-existing nits — see S5), `cargo test -p nexus-local-db --test works_schedule_migration` (7/7 pass), `cargo test -p nexus42 --test cron_cli` (8/8 pass), `cargo +nightly fmt --all --check` (exit 0)

## Findings

### 🔴 Critical

None.

### 🟡 Warning

#### W1 — TZ silently clobbered on role-only patch operations (correctness / data consistency)

- **Location**: `crates/nexus42/src/commands/creator/works/cron.rs:494-532` (`handle_set`), interacting with `apply_set_args` at `cron.rs:208-260`.
- **Trigger**: `creator works cron set <work> --no-review` (or any patch that omits `--tz`) on a Work that previously had a custom TZ stored.
- **Bug**: `handle_set` unconditionally computes `resolved_tz = args.tz.or(NEXUS_TZ env).or(UTC)` and, when `args.tz.is_none()`, folds it into `effective_args.tz = Some(resolved_tz)` (lines 508-518). `apply_set_args` then sees `args.tz.is_some()` (line 227) and overwrites `schedule.tz` from `base.tz` (e.g. `Asia/Shanghai`) to `NEXUS_TZ`/`UTC` (line 243). A surgical role-only edit silently discards the previously-configured author TZ.
- **Evidence (trace)**:
  1. User runs `set my-work --tz Asia/Shanghai` → stored `schedule_json` has `tz:"Asia/Shanghai"`.
  2. User runs `set my-work --no-review` → `args.tz = None`, `args.no_review = true`.
  3. `handle_set`: `effective_args.tz = Some(env::var("NEXUS_TZ").unwrap_or("UTC"))`.
  4. `apply_set_args(base, effective_args)`: `args.tz.is_some()` → patch branch → `schedule.tz = "UTC"` (clobber).
- **Contradicts**:
  - The clap `--tz` doc itself (cron.rs:403): *"IANA timezone (default: read from env `NEXUS_TZ`, fallback `UTC`)"* — implies the default applies when *setting* TZ, not when *patching other fields*.
  - Spec §3.1 example `creator works cron set my-work --no-review` which is clearly a surgical disable, not a TZ reset.
  - General patch-command ergonomics: unmentioned fields should be preserved.
- **Testability gap (amplifies the bug)**: `NEXUS_TZ` resolution lives at `cron.rs:511` inside `handle_set`, which is **not** covered by any test. The pure functions (`apply_set_args`, `resolve_schedule`) are well-tested but never exercise the env-resolution or the `effective_args.tz` fold, so the suite gives false confidence that patch operations preserve TZ.
- **Severity rationale**: Warning (not Critical). No crash, no corruption beyond the single `tz` field, schedule remains structurally valid, foundation-only (no firing yet → no production impact), and recoverable by re-setting TZ. But it is **silent state loss on a common patch path**, contradicting the flag's documented semantics and the spec's patch-command intent — blocking before T-A P1 builds on this contract.
- **Suggested fix**: Only fold env/default TZ when the user explicitly resets (the no-flags → defaults branch already at cron.rs:254-257) OR passes `--tz`. For patch operations without `--tz`, leave `base.tz` unchanged. Recommend extracting TZ resolution into a pure helper (e.g. `resolve_tz(args.tz, env) -> Option<String>`) so the merge semantics become unit-testable; add a regression test that runs `set --no-review` on a Work with a stored custom TZ and asserts TZ is preserved.

#### W2 — Missing "at least one role enabled" validation and missing `--all-off` flag (spec compliance / correctness)

- **Location**: `crates/nexus42/src/commands/creator/works/cron.rs` — `CronCommand::Set` variant (lines 391-418, no `--all-off` flag) and `apply_set_args` (lines 208-260, no all-disabled check).
- **Bug**: Spec §3.1 (normative V1.50) mandates, verbatim:
  > At least one role must remain `enabled: true` unless `--all-off` is passed (CLI rejects empty schedules).
- The implementation has **both** halves missing:
  1. No `--all-off` flag in `CronCommand::Set`.
  2. No post-mutation check in `apply_set_args` rejecting the all-disabled state.
- `creator works cron set my-work --no-brainstorm --no-write --no-review` succeeds and persists an all-disabled schedule (`{"roles":{"brainstorm":{"enabled":false},...}}`).
- **Why this matters for the foundation**: spec §4.1 (daemon firing, T-A P1) iterates "for each role enabled" — an all-disabled Work is technically a no-op, but the spec's contract is that the **CLI rejects** this at config time (the author must explicitly opt in via `--all-off` to express "pause everything"). Shipping P0 without the validation means T-A P1 will either have to retro-fit it (re-balancing the contract) or inherit a permissive surface that contradicts the locked spec.
- **Severity rationale**: Warning (not Critical). Explicit normative spec rule missing; no security/crash impact; foundation-only so no firing consequence yet. But the contract is locked and the gap is cheap to close now.
- **Suggested fix**: (a) Add `#[arg(long, default_value_t = false)] all_off: bool` to `CronCommand::Set`. (b) In `apply_set_args`, after computing the patched schedule, reject with a stable error code (e.g. `E_CRON_ALL_DISABLED`) when `!args.all_off && !brainstorm.enabled && !write.enabled && !review.enabled`. (c) Add a test asserting the rejection and a test asserting `--all-off` permits it.

### 🟢 Suggestion

#### S1 — Default schedule table: naming coherence with spec §2.2
- Spec §2.2 names the symbol `cron.rs::DEFAULT_SCHEDULE` (a table constant); the implementation uses four separate consts (`DEFAULT_BRAINSTORM_CRON`, `DEFAULT_WRITE_CRON`, `DEFAULT_REVIEW_CRON`, `DEFAULT_TZ`) plus a `WorkSchedule::defaults()` typed constructor. The spec's "(or equivalent)" hedge covers this and the typed constructor is arguably cleaner, but a future reader grepping for `DEFAULT_SCHEDULE` per the spec will not find it. Consider either a `DEFAULT_SCHEDULE: &[(role, cron)]` table for display/`list` use, or a one-line comment at the consts block pointing spec readers here.

#### S2 — `render_show` omits `Local time` and `Next fire (UTC)` columns (spec §3.2)
- Spec §3.2 normative table has four columns (`Role / Cron / Local time / Next fire (UTC)`); impl has three (`Role / Cron / Enabled`). The `Enabled` column is a sensible addition (spec §3.2 does not forbid it). `"Next fire (UTC)"` reasonably defers to T-A P1 (requires cron iteration via `cron::Schedule::iter_after`). `"Local time"` is pure rendering and could be added now. Plan AC #3 ("show with TZ display") is met by the TZ header + offset line, so this is below the AC bar — but spec fidelity is partial and worth a tracking note for P1.

#### S3 — `list` JSON output omits `work_id`
- `handle_list` JSON branch (`cron.rs:573-581`) emits `{"work_ref": ..., "schedule": ...}` but not `work_id`. A Work with `work_ref = null` produces `"work_ref": null` with no identifier for machine consumers. The human `render_list` falls back to `work_id` (`cron.rs:323`); the JSON path should too. One-line addition: add `"work_id": r.work_id` to the `json!` object.

#### S4 — `#[allow(clippy::unwrap_used)]` without adjacent justification
- `cron.rs:611` declares `#[allow(clippy::unwrap_used)]` on the test module with no justification comment. Repo root `AGENTS.md` (Clippy section): *"Do not suppress with `#[allow(...)]` without a brief justification comment."* Add a one-line comment, e.g. `// test-only: unwrap keeps assertion intent readable`.

#### S5 — `doc_markdown` clippy nits in the new test file (under `--all-targets`)
- `crates/nexus-local-db/tests/works_schedule_migration.rs:9` (`SQLite`) and `:14` (`sqlx::migrate`) trigger `clippy::doc_markdown` (missing backticks around mixed-case words) under `--all-targets`.
- **Context**: the AC #7 / CI command shape (`cargo clippy -p nexus-local-db -p nexus42 -- -D warnings`, and repo CI `cargo clippy --all -- -D warnings`) does **not** compile test targets, so CI is green and the Completion Report's "Finished, no warnings" claim is accurate for the command as written. This finding is hygiene-only under the stricter `--all-targets` scope. Fix: wrap `SQLite` → `` `SQLite` `` and `sqlx::migrate` → `` `sqlx::migrate` ``.
- **Note for PM**: the same `--all-targets` run surfaces ~53 **pre-existing** errors in untouched files (`test_tracing.rs`, `v148_serial_hardening`, `v142_migration_fixes`, lib-test ×N). Verified none of those source files were modified in this plan's range (`git diff --name-only c38fbe1f..HEAD` ∩ those files = ∅ for everything except `works.rs`, which is clean). Those are out-of-scope for this QC row but may warrant a separate hygiene plan.

#### S6 — `resolve_work_id_by_ref_or_id` uses `LIMIT 1` without `ORDER BY`
- `nexus-local-db/src/works.rs:1394-1410` query `WHERE creator_id=? AND workspace_slug=? AND (work_ref=? OR work_id=?) LIMIT 1` has no `ORDER BY`, so the chosen row is non-deterministic in the theoretical case where a `work_ref` slug collides with another row's `work_id`. In practice collision is implausible (slugs vs `wrk_...` IDs live in different namespaces), so this is a clarity/determinism nit, not a live bug. Suggest either `ORDER BY work_id` for determinism, or split into two sequential lookups (ref first, then id) which also makes the "ref wins over id" precedence explicit.

## Source Trace

| Finding ID | Source Type | Source Reference | Confidence |
|------------|-------------|------------------|------------|
| W1 | manual-reasoning + git-diff | `cron.rs:508-520` (`handle_set` TZ fold) + `cron.rs:223-244` (`apply_set_args` patch branch); trace reproduced in finding body | High |
| W2 | doc-rule + git-diff | spec `cron-staggering.md` §3.1 last bullet vs `cron.rs:391-418` (no `--all-off`) + `cron.rs:208-260` (no all-disabled check) | High |
| S1 | doc-rule | spec `cron-staggering.md` §2.2 vs `cron.rs:31-34, 67-88` | High |
| S2 | doc-rule + git-diff | spec `cron-staggering.md` §3.2 table vs `cron.rs:267-299` (`render_show`) | High |
| S3 | git-diff | `cron.rs:573-581` vs `cron.rs:323` | High |
| S4 | linter + doc-rule | `cron.rs:611` vs root `AGENTS.md` Clippy section | High |
| S5 | linter | `cargo clippy --all-targets` output on `works_schedule_migration.rs:9,14` | High |
| S6 | manual-reasoning | `nexus-local-db/src/works.rs:1394-1410` | Medium |

## Architecture & Maintainability Observations (no action required — recorded for traceability)

- **Layer separation is clean.** DAO (`nexus-local-db::works`: `set/get/list_schedule_json`, `resolve_work_id_by_ref_or_id`, `WorkScheduleRow`) / CLI glue (`handle_set/show/list`) / pure validation+render (`apply_set_args`, `validate_cron_expr`, `validate_tz`, `render_*`) are properly split. The pure functions are unit-testable; the `handle_*` glue is exercised via integration tests. The one seam that escapes unit coverage is the env-resolution inside `handle_set` (see W1 testability gap).
- **No leaky abstractions; no duplicate DTOs.** `WorkSchedule` / `RoleSchedule` / `RolesSchedule` live in the CLI crate as the spec-mandated rendering model. They are intentionally **not** in `nexus-contracts` (foundation slice has no wire surface; spec §4.1 daemon firing is T-A P1). This matches the plan's non-goal of "no new `schemas/` JSON Schema files" and avoids premature DTO codification. When T-A P1 needs to transport the schedule over the daemon API, the model should be promoted to `nexus-contracts` at that point — worth flagging in the P1 plan.
- **`MissingVersionKey` reuse is correct.** The new `set_schedule_json` returns `LocalDbError::MissingVersionKey` for Work-not-found (cron.rs DAO `works.rs:1351`). The variant name is misleading (it is really a generic row-not-found), but it is the **established codebase convention**: 7+ existing call sites in `works.rs` (lines 710, 840, 1107, 1172, 1265, 1301), plus `runtime_lock.rs`, `novel_pool_entries.rs`, `inspiration_items.rs`, `lib.rs`. The implementer correctly followed local convention rather than introducing a one-off `WorkNotFound` variant. Not a finding.
- **`resolve_work_id_by_ref_or_id` is not a duplicate.** Existing `get_work` (`works.rs:455`) resolves by `work_id` only; no prior ref-or-id resolver existed. The new helper is justified for the CLI surface that accepts both forms.
- **Migration naming follows convention.** `202606180001_works_schedule_json.sql` matches the established `works_*` pattern (`works_novel_profile`, `works_auto_chain_checkpoint`, `works_auto_review_master`) and the `YYYYMMDDNNNN_<table>_<change>.sql` format. The migration header comment is thorough (cites spec §2.1 shape, §2.3 back-compat, and the empty-vs-NULL convention).
- **`cron` + `chrono-tz` are new workspace deps**, despite spec §3.1's "already a dep; verify V1.50 T-A P0 T2" hedge and the Assignment's "existing cron crate dep" hint. The spec's "verify" wording explicitly allowed this; both deps are used (in `validate_cron_expr` / `validate_tz`) and the additions are minimal and workspace-scoped. Not a finding.
- **CLI opens local `state.db` directly** (no daemon HTTP) — explicitly justified in plan §Decision #1 with precedent (`soul::open_global_db`, `db::Schema::init`). Foundation-appropriate; the daemon's P1 firing layer reads the same column via the same DAO.
- **Changes are surgical.** 10 files, all within the plan's "Code touch" list (plan §18-24). No opportunistic refactor of unrelated code; `mod.rs` change is a clean 3-hunk wiring (module decl + enum variant + dispatch arm). The `Cargo.lock` +63 lines is consistent with adding 2 new deps + transitive closure.

## Verification Evidence

| Check | Command | Result |
|-------|---------|--------|
| Worktree/branch | `git rev-parse --show-toplevel` + `git branch --show-current` | `/Users/bibi/.../.worktrees/v150-cron-foundation`, `feature/v1.50-cron-foundation` ✓ |
| Commit range | `git log --oneline c38fbe1f..HEAD` | 5 commits: `cdceac31`, `a1a67217`, `867c4c88`, `11295a34`, `a7ea8349` ✓ (matches Assignment) |
| Build | `cargo build -p nexus-local-db -p nexus42` | Finished clean |
| Clippy (lib+bin, CI shape) | `cargo clippy -p nexus-local-db -p nexus42 --lib -- -D warnings` | Finished, no warnings (matches AC #7 / CI gate) |
| Clippy (all-targets, stricter) | `cargo clippy -p nexus-local-db -p nexus42 --all-targets -- -D warnings` | 2 in-scope `doc_markdown` nits (S5) + ~53 pre-existing in untouched files (out of scope) |
| Migration test | `cargo test -p nexus-local-db --test works_schedule_migration` | 7 passed; 0 failed (AC #2 ✓) |
| CLI integration test | `cargo test -p nexus42 --test cron_cli` | 8 passed; 0 failed (AC #3 ✓) |
| Nightly fmt | `cargo +nightly fmt --all --check` | exit 0 (AC #6 ✓) |
| Clean tree | `git status --porcelain` | empty (no stray edits before report write) |

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 2 |
| 🟢 Suggestion | 6 |

**Verdict**: **Request Changes**

Two unresolved Warning findings block approval:

1. **W1 (TZ-clobber correctness bug)** — `handle_set` silently discards a previously-configured author TZ on any role-only patch operation (`--no-review`, `--brainstorm`, etc.). The bug lives in untested glue (`NEXUS_TZ` resolution at `cron.rs:511`), so the pure-function test suite gives false confidence. Must fix + add a TZ-preservation regression test before T-A P1 builds on this contract.
2. **W2 (missing spec-mandated validation)** — spec §3.1 explicitly requires the CLI to reject all-disabled schedules unless `--all-off` is passed; neither the validation nor the `--all-off` escape hatch is implemented. Cheap to close now; expensive to retro-fit once T-A P1 fires.

Six Suggestions (S1–S6) are non-blocking improvements (naming coherence, render fidelity, JSON completeness, lint-policy adherence, lint hygiene, query determinism). None block merge after W1/W2 are addressed.

The foundation's architecture is otherwise sound: clean DAO/CLI/validation layering, correct use of established codebase conventions (`MissingVersionKey`, migration naming, direct-DB CLI access with precedent), surgical change set, no duplicate DTOs, and solid hermetic test coverage at the pure-function and migration layers. The gaps are concentrated in the `handle_set` merge semantics (W1) and one missing normative validation rule (W2) — both fixable without re-architecting the slice.

---

## Revalidation

```yaml
report_kind: qc-revalidation
reviewer: qc-specialist
reviewer_index: 1
plan_id: 2026-06-18-v1.50-cron-foundation
working_branch: feature/v1.50-cron-foundation
review_cwd: /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v150-cron-foundation
review_range: a7ea8349..f5f58edd
fix_wave_commits:
  - 87ea2ef6 (R-V150P0-W4 list_works_schedule bounded)
  - 000d63fd (R-V150P0-W1 TZ preservation)
  - e478079c (R-V150P0-W2 all-off flag + check)
  - a364f31a (R-V150P0-W3 show columns)
  - 5dc8eaa3 (R-V150P0-W5 TODO marker)
  - f5f58edd (plan completion report)
verdict: Approve
generated_at: 2026-06-17T07:09:21Z
```

### Re-review scope

- **Reviewer**: @qc-specialist (Reviewer #1 — architecture coherence + maintainability)
- **Re-review kind**: targeted (qc1 only; qc2 was Approve in the initial wave; qc3 is reviewing the fix-wave in parallel under a separate Assignment)
- **Revalidation focus**: the two blocking Warnings raised by qc1 in the initial wave — **W1** (TZ silently clobbered on role-only patch) and **W2** (all-off rule not enforced). W3/W4/W5 were Suggestion-tier and are not blocking; they are in the fix-wave diff and were spot-checked for regressions only.
- **Diff basis (fix-wave)**: `a7ea8349..f5f58edd` (6 commits, verified via `git log --oneline a7ea8349..f5f58edd`)
- **Working branch (verified)**: `feature/v1.50-cron-foundation`
- **Review cwd (verified)**: `/Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v150-cron-foundation` (`git rev-parse --show-toplevel`)

### Per-finding disposition

#### W1 — TZ silently clobbered on role-only patch → **Resolved**

Commit `000d63fd` (`fix(nexus42): R-V150P0-W1 TZ preservation on role-only patch + regression test`).

**Required fix elements — all present:**

1. **`resolve_tz(args_tz, env) -> Option<String>` pure helper** — extracted at `cron.rs` (diff hunks `@@ -254,11 +254,36` and `@@ -510,17 +535,29`). Pure merge semantic: `args_tz` wins; else `env`; else `None`. Doc comment explicitly states "Pure merge semantic, unit-tested independently of `handle_set`". This directly closes the **testability gap** flagged in the initial W1 ("`NEXUS_TZ` resolution lives at `cron.rs:511` inside `handle_set`, which is not covered by any test") — the env-resolution is now a pure, unit-tested function.
2. **`handle_set` only folds env/default TZ on reset/pass** — `is_reset` is computed from all patch flags being absent; `env_for_resolve` is `NEXUS_TZ` only on the reset path, else `None`; `effective_args.tz` is `Some(tz)` when there is intent, `Some(DEFAULT_TZ)` on reset-with-no-tz, and **`None` on patch-without-`--tz`** (preserving `base.tz`).
3. **`apply_set_args` honors the merge** — patch branch (unchanged) only overwrites `schedule.tz` when `args.tz.is_some()`; reset branch now explicitly folds `effective_args.tz` when present (`if let Some(tz) = &args.tz { schedule.tz.clone_from(tz); }`) so `NEXUS_TZ`/`UTC` still land on a reset. The original W1 trace (4-step reproduction) is broken at step 3: `effective_args.tz` is now `None`, not `Some("UTC")`.
4. **Regression test `set_no_review_preserves_custom_tz`** — pre-stores `base.tz = "Asia/Shanghai"`, patches with `--no-review` and no `--tz`, asserts `out.tz == "Asia/Shanghai"` and `!out.roles.review.enabled`. **Passes** (see Verification Evidence below).

**Test evidence (run this session):**
- `cargo test -p nexus42 --lib commands::creator::works::cron::tests::set_no_review_preserves_custom_tz` → **1 passed; 0 failed**.
- `cargo test -p nexus42 --lib commands::creator::works::cron::tests::resolve_tz` → **3 passed; 0 failed** (`resolve_tz_explicit_arg_wins`, `resolve_tz_env_used_when_no_arg`, `resolve_tz_none_when_no_intent`).

**Architecture/maintainability assessment:** the fix is the cleanest available shape — pure helper extracted, documented, and unit-tested; the `handle_set` `is_reset`/`effective_args.tz` match is readable and comments cite R-V150P0-W1 and the spec §3.1 reset-vs-patch intent. Layer separation (pure merge fn vs env-glue) is preserved. No regression.

#### W2 — Missing "at least one role enabled" validation + missing `--all-off` flag → **Resolved**

Commit `e478079c` (`fix(nexus42): R-V150P0-W2 all-off flag + all-disabled check + tests`).

**Required fix elements — all present:**

1. **`--all-off` flag in `CronCommand::Set`** — added with `#[arg(long, default_value_t = false)]` and doc comment "Permit disabling all three roles at once (spec §3.1 \"all-off\" rule)" (`@@ -437,6 +461,9` hunk). Wired through `handle_cron` → `CronSetArgs.all_off` → `handle_set` (`@@ -491,6 +518` and `@@ -506,6 +534` hunks).
2. **`all_off: bool` on `CronSetArgs`** with `#[allow(clippy::struct_excessive_bools)]` **and an adjacent justification comment** ("The bool fields are a 1:1 mirror of clap's `--no-<role>` / `--all-off` flags; restructuring into enums would diverge from the CLI surface"). This satisfies the repo `AGENTS.md` Clippy rule that was the subject of initial-wave S4 — the allow is justified here.
3. **Post-mutation enabled-role count + stable code** — check placed **after** all role mutations at the tail of `apply_set_args` (`@@ -263,6 +274,19`): `any_enabled = brainstorm || write || review`; if `!any_enabled && !args.all_off` → `Err(CliError::Config("[E_CRON_ALL_ROLES_DISABLED] ..."))`. Stable code `E_CRON_ALL_ROLES_DISABLED` is a named `const ERR_ALL_DISABLED`.
4. **Both regression tests**:
   - `apply_set_args_all_off_without_flag_rejects` — `no_brainstorm/no_write/no_review` + `all_off: false`, asserts `unwrap_err()` and message contains `E_CRON_ALL_ROLES_DISABLED`. **Passes.**
   - `apply_set_args_all_off_with_flag_succeeds` — same flags + `all_off: true`, asserts all three roles disabled and `apply_set_args` returns `Ok`. **Passes.**
5. **Help coverage** — `crates/nexus42/tests/cron_cli.rs::cron_set_help_documents_flags` extended to assert `--all-off` appears in `set --help` (`@@ -239,6 +239` hunk). **Passes** (part of the 9/9 cron_cli suite).

**Test evidence (run this session):**
- `cargo test -p nexus42 --lib commands::creator::works::cron::tests::apply_set_args_all_off` → **2 passed; 0 failed** (both `_with_flag_succeeds` and `_without_flag_rejects`).

**Architecture/maintainability assessment:** the validation sits in the correct layer (`apply_set_args`, the pure validation+mutation fn) and fires after all mutations complete, so it sees the final schedule state regardless of patch vs reset path. The stable error code is consistent with the existing `E_CRON_INVALID_EXPR` / `E_CRON_INVALID_TZ` convention. No regression.

### New findings introduced by the fix-wave

**None (Critical = 0, Warning = 0).**

One **non-blocking observation** recorded for traceability (not a finding, does not affect verdict):

- **O-1 (maintainability, defer/accept)**: the "is this a reset?" predicate now exists in two places — `handle_set` computes `is_reset` to decide the env-fold (cron.rs ~line 540), and `apply_set_args` re-derives the same condition via its `else` branch (cron.rs ~line 254-257) to decide the default-reset. They are **consistent today** and serve different layers (env-glue vs pure mutation), so the duplication is acceptable and arguably correct given the layer separation. A shared `CronSetArgs::is_reset()` helper would be a future Suggestion-tier cleanup if a third caller appears; not worth refactoring in this fix-wave.

### Verification evidence (re-review)

| Check | Command | Result |
|-------|---------|--------|
| Worktree/branch | `git rev-parse --show-toplevel` + `git branch --show-current` | `.worktrees/v150-cron-foundation`, `feature/v1.50-cron-foundation` ✓ |
| Fix-wave commits | `git log --oneline a7ea8349..f5f58edd` | 6 commits (`87ea2ef6`, `000d63fd`, `e478079c`, `a364f31a`, `5dc8eaa3`, `f5f58edd`) ✓ (matches Assignment) |
| Diff stat | `git diff a7ea8349..f5f58edd --stat` | 8 files, +929/-47 (3 QC reports + plan + 4 source/test/config) ✓ |
| Build | `cargo build -p nexus-local-db -p nexus42` | Finished clean ✓ |
| Clippy (CI shape) | `cargo clippy -p nexus-local-db -p nexus42 -- -D warnings` | Finished, no warnings ✓ |
| Nightly fmt | `cargo +nightly fmt --all --check` | exit 0 ✓ |
| W1 regression | `cargo test -p nexus42 --lib ...::set_no_review_preserves_custom_tz` | 1 passed; 0 failed ✓ |
| W1 pure helper | `cargo test -p nexus42 --lib ...::resolve_tz` | 3 passed; 0 failed ✓ |
| W2 regression | `cargo test -p nexus42 --lib ...::apply_set_args_all_off` | 2 passed; 0 failed ✓ |
| Full CLI suite (regression) | `cargo test -p nexus42 --test cron_cli` | 9 passed; 0 failed (was 8 in initial wave; +1 for `cron_list_help_documents_limit_flag` from W4) ✓ |
| Migration suite (regression) | `cargo test -p nexus-local-db --test works_schedule_migration` | 8 passed; 0 failed (was 7 in initial wave; +1 for `list_works_schedule_applies_limit` from W4) ✓ |

### Revalidation verdict

**Approve.**

Both blocking Warnings from the initial wave are **Resolved** with the exact fix shapes requested in the Assignment:

- **W1 Resolved** — `resolve_tz(args_tz, env) -> Option<String>` pure helper extracted and unit-tested; `handle_set` only folds env/default TZ on reset/pass, preserving `base.tz` on role-only patches; `apply_set_args` reset branch honors the folded TZ. Regression test `set_no_review_preserves_custom_tz` + 3 `resolve_tz_*` unit tests pass. The original 4-step clobber trace is broken. Commit `000d63fd`.
- **W2 Resolved** — `--all-off` flag added to `CronCommand::Set` and wired through `handle_cron` → `CronSetArgs.all_off`; post-mutation all-disabled check in `apply_set_args` rejects with stable code `E_CRON_ALL_ROLES_DISABLED` unless `--all-off` is passed; both `apply_set_args_all_off_without_flag_rejects` and `..._with_flag_succeeds` pass; `cron set --help` documents `--all-off`. Commit `e478079c`.

No new Critical or Warning findings introduced by the fix-wave (one non-blocking maintainability observation O-1 noted for traceability). Full build, CI-shape clippy (`-D warnings`), nightly fmt, and the complete cron/migration test suites are green — no regressions from W3/W4/W5 or the W1/W2 fixes themselves. The initial-wave Suggestions S1–S6 remain non-blocking and outside this targeted re-review's scope.

**Final verdict (qc1, after fix-wave `a7ea8349..f5f58edd`): Approve.**
