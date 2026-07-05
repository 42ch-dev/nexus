---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: 2026-06-18-v1.50-cron-foundation
working_branch: feature/v1.50-cron-foundation
review_cwd: /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v150-cron-foundation
review_range: merge-base c38fbe1f264b9574b25355d872d20138c1c04e77..a7ea8349260fa7c8cc5be0f586fa9f84d13549ee
verdict: Approve
generated_at: 2026-06-17T21:45:00Z
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: xai/grok-build-0.1
- Review Perspective: security + correctness (primary); shared baseline coverage for regression, maintainability
- Report Timestamp: 2026-06-17T21:45:00Z

## Scope
- plan_id: 2026-06-18-v1.50-cron-foundation
- Review range / Diff basis: merge-base c38fbe1f264b9574b25355d872d20138c1c04e77..a7ea8349260fa7c8cc5be0f586fa9f84d13549ee
- Working branch (verified): feature/v1.50-cron-foundation
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v150-cron-foundation
- Files reviewed: 9 (plan + 8 source/test files touched by the 5 commits)
- Commit range: a7ea8349 (T7 done) .. 5 commits back to cdceac31 (T1 migration/DAO)
- Tools run: `git rev-parse`, `git log --oneline <range>`, `git diff --name-only`, targeted `read` of plan + DAO + migration + cron.rs + mod.rs + tests + AGENTS.md files, `cargo test -p nexus-local-db --test works_schedule_migration`, `cargo test -p nexus42 --test cron_cli`, `cargo +nightly fmt --all -- --check`, `cargo clippy -p nexus-local-db -p nexus42 -- -D warnings`

## Findings

### 🔴 Critical
- None.

### 🟡 Warning
- **W-1 (DAO surface not creator-scoped)**: `set_schedule_json(pool, work_id, json, now)` and `get_schedule_json(pool, work_id)` (and the new `list_works_schedule`) operate only on `work_id` in the UPDATE/SELECT WHERE. Sibling functions in the same file (`patch_work`, `get_work`, `list_works`, etc.) consistently take `creator_id` and include it in the predicate. Current CLI path closes the vector because `handle_set` / `handle_show` / `handle_list` first call the scoped `resolve_work_id_by_ref_or_id(creator_id, workspace_slug, ref_or_id)` before touching schedule_json. However, the DAO primitive itself exposes a weaker contract than the rest of the module. Any future internal caller that bypasses the resolve layer (or a daemon T-A P1 path that receives a work_id from a different trust boundary) could affect another creator's schedule. Matches the "不变量和状态转换逻辑连贯" and input-validation checklist items.
  - Source: `crates/nexus-local-db/src/works.rs:1334-1356` (set), `1367-1381` (get), `1425-1450` (list); contrast with `patch_work` at 625+ and `get_work` at 455+ which carry creator_id.
  - Fix: add `creator_id` parameter to the three new functions (or a single `set_schedule_json_for_creator` variant) and include `AND creator_id = ?` (and the equivalent filter on list). Keep the public CLI entry point unchanged.
  - Severity justification: latent correctness / defense-in-depth gap, not an active exploit in this foundation slice.

- **W-2 (migration not re-runnable)**: The migration `202606180001_works_schedule_json.sql` is a bare `ALTER TABLE works ADD COLUMN schedule_json TEXT;`. SQLite will error "duplicate column name" on re-execution against an already-migrated database. Project convention (see prior QC findings on V1.42 multi-volume and general migration hygiene in archived knowledge) and `nexus-local-db/AGENTS.md` expectations favor migrations that are safe to re-apply (IF NOT EXISTS for CREATE, guards or conditional DDL for ALTER). The hermetic test `works_schedule_migration.rs` uses a fresh temp DB for every case (forward, rollback simulation via manual DROP, DAO round-trips) and therefore never exercises re-apply. AC #2 only asserts "adds the column"; it does not explicitly claim re-run safety, but the pattern is a latent reliability / correctness risk under tooling retry, dev DB reset scripts, or partial migration apply.
  - Source: `crates/nexus-local-db/migrations/202606180001_works_schedule_json.sql:29`; test file uses `fresh_pool()` + `run_migrations` per test.
  - Fix: wrap with a conditional (common pattern: `PRAGMA table_info` check or a small guard table / version bump) or document the one-shot nature explicitly if the project decides ADD COLUMN is intentionally non-idempotent.
  - Severity justification: matches "资源生命周期处理正确" and prior migration idempotency reviews; not data-loss, but fails the "safe to re-execute" expectation.

### 🟢 Suggestion
- **S-1 (AC wording vs implementation error type)**: Plan Acceptance Criteria §5 states "Invalid cron expression returns `ValidationError` with stable error code". The implementation returns `CliError::Config(format!("[{ERR_...}] ..."))` (with the stable `E_CRON_INVALID_EXPR` / `E_CRON_INVALID_TZ` tokens embedded). The unit tests (`cron_cli.rs:653-666`) correctly assert that the message contains the bracketed code. This is functionally correct and satisfies the "stable error codes per the AGENTS.md rule for CLI failures". However, the literal AC text mentions a `ValidationError` variant that is not used on this path (a `DomainError::ValidationError` exists elsewhere but is not surfaced here). Minor spec/impl drift.
  - Source: plan §4 AC5; `cron.rs:123,153` (CliError::Config with codes); `errors.rs` (CliError definition); tests assert on message content.
  - Recommendation: align the AC text to "returns a stable error code inside a Config error" (or introduce a typed ValidationError if policy changes). Not blocking.

- **S-2 (future blob size / structure guard)**: `schedule_json` is written as an arbitrary TEXT blob. Current `apply_set_args` + `WorkSchedule` serde round-trip only ever produces a small, well-formed object (three validated cron strings + IANA TZ + three booleans). `resolve_schedule` safely falls back to defaults on any malformation. When the daemon T-A P1 layer begins consuming this column under potentially adversarial creator-controlled input, an explicit documented max size or structural guard at the write boundary would be defense-in-depth. Not a current injection vector (no shell, no path, validated fields only).
  - Source: `cron.rs:520-523` (set path), `172-177` (resolve fallback).
  - Recommendation: add a small constant + length check in `to_json_string` / handle_set, or a schema validation step, with a clear comment tying it to the future daemon consumer.

## Source Trace
- Finding ID: W-1
- Source Type: manual code review + pattern comparison in same module
- Source Reference: git diff on `crates/nexus-local-db/src/works.rs` (new DAO functions vs existing creator-scoped paths); `cron.rs:502-523` (resolve-then-set call site)
- Confidence: High

- Finding ID: W-2
- Source Type: migration file + test structure review
- Source Reference: `crates/nexus-local-db/migrations/202606180001_works_schedule_json.sql`; `works_schedule_migration.rs` (fresh_pool per test)
- Confidence: High

- Finding ID: S-1
- Source Type: AC text vs implementation + test assertions
- Source Reference: plan §4 AC5; `cron.rs:105-106,118-127,149-157`; `cron_cli.rs:653-666`
- Confidence: High

- Finding ID: S-2
- Source Type: data-flow review for future untrusted consumer
- Source Reference: `cron.rs:520` (blob write), `172` (resolve), plan T-A P1 note in module doc
- Confidence: Medium

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 2 |
| 🟢 Suggestion | 2 |

**Verdict**: Approve

All acceptance criteria explicitly called out in the plan (§4 AC #2–#5) are exercised by the hermetic test suites, which pass. Validation uses the `cron` + `chrono-tz` crates with stable bracketed error codes on the CLI surface (satisfies AGENTS.md CLI failure rule). No SQL injection, no cross-creator authorization bypass on the current path, no unvalidated input driving privileged behavior, and no shell/path metachar vectors. The two Warnings are latent defense-in-depth / project-norm gaps rather than active exploitable conditions in this foundation slice (T-A P0 only; daemon firing is P1). Tests, fmt, and clippy are clean on the touched crates. Recommend fixing W-1 and W-2 in a follow-up hygiene task before the daemon consumption layer lands.

---

## Completion Report v2

**Agent**: qc-specialist-2
**Task**: QC Reviewer #2 (security + correctness) — V1.50 T-A P0 cron-foundation initial wave tri-review
**Status**: Done
**Scope Delivered**: Full direct review of the 5 commits / 9 files in the assigned range; verification of cwd/branch/merge-base alignment; execution of all plan verification commands (migration tests, cron_cli tests, nightly fmt --check, clippy -D warnings); security/correctness-focused analysis of cron expression/TZ validation, DAO SQL binding, creator authz scoping, migration re-run safety, stable error codes, and AC coverage; production of this report with verbatim frontmatter and Scope per assignment + mstar-review-qc template.
**Artifacts**:
- `.mstar/plans/reports/2026-06-18-v1.50-cron-foundation/qc2.md` (this file)
**Validation**:
- `cargo test -p nexus-local-db --test works_schedule_migration` → 7/7 passed
- `cargo test -p nexus42 --test cron_cli` → 8/8 passed
- `cargo +nightly fmt --all -- --check` → clean
- `cargo clippy -p nexus-local-db -p nexus42 -- -D warnings` → clean (no output)
- All 5 commits and diff scope manually inspected; AC #3–#5 paths exercised in tests with evidence of stable codes and round-trips.
**Issues/Risks**: Two Warnings logged (DAO creator scoping gap; migration re-apply safety). No Criticals. No blocking security or correctness defects for the foundation scope.
**Plan Update**: N/A (QC role does not mutate plan or status.json; PM owns residual registration from this report).
**Handoff**: Report committed to the canonical path. Ready for PM consolidation with qc1/qc3.
**Git**: 8e3f0c2 qc(qc-specialist-2): V1.50 T-A P0 cron-foundation security+correctness review (qc2)
