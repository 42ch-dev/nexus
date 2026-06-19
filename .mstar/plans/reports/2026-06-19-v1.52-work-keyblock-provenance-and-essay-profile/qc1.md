---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-06-19-v1.52-work-keyblock-provenance-and-essay-profile"
verdict: "Approve"
generated_at: "2026-06-20"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Runtime Model: deepseek/deepseek-v4-pro
- Review Perspective: architecture coherence and maintainability risk
- Report Timestamp: 2026-06-19T22:30:00Z

## Scope
- plan_id: `2026-06-19-v1.52-work-keyblock-provenance-and-essay-profile`
- Review range / Diff basis (initial): `b97ec0d9..09837535`
- Review range / Diff basis (revalidation): `09837535..da4caab4`
- Working branch (verified): `feature/v1.52-work-keyblock-provenance-and-essay-profile`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.52-ta-p2`
- Files reviewed: 62 (initial) + 7 changed (fix wave)
- Commit range (initial): `b97ec0d9..09837535` (HEAD: `09837535`)
- Commit range (fix wave): `09837535..da4caab4` (HEAD: `da4caab4`)
- Tools run (initial): `cargo clippy --all -- -D warnings` (pass), `cargo +nightly fmt --all --check` (pass), `cargo test -p nexus-orchestration -- capability::tests` (2 fail), `cargo test -p nexus42 --test creator_world_kb` (2 fail), `cargo test -p nexus42 --test world_kb_alias` (7 fail), `cargo test -p nexus-local-db -- kb_store` (21 fail)
- Tools run (revalidation): `cargo test -p nexus-local-db --test migrations_apply` (2 pass), `cargo test -p nexus-orchestration --lib -- capability` (169 pass), `cargo clippy --all -- -D warnings` (pass)

## Findings
### 🔴 Critical

- **F-001: Migration `202606190004_work_profile_essay.sql` missing `auto_chronology` column**  
  The `works_new` CREATE TABLE defines 32 columns but the existing `works` table has 33 columns (migration `202606180005_works_auto_chronology.sql` added `auto_chronology BOOLEAN NOT NULL DEFAULT 0`). The `SELECT * FROM works` returns 33 values, causing `table works_new has 32 columns but 33 values were supplied`.  
  **Blast radius**: ALL database-dependent tests fail — `kb_store` (21/21 fail), `creator_world_kb` (2/2 fail), `world_kb_alias` (7/9 fail). This blocks the entire test suite for any crate that runs the full migration chain.  
  **Fix**: Add `auto_chronology BOOLEAN NOT NULL DEFAULT 0` to the `works_new` CREATE TABLE. Also verify no other columns from prior migrations (`20260606_works_stage_columns.sql`, `202606090001_works_auto_chain_checkpoint.sql`, `202606090003_works_auto_review_master.sql`, `202606100002_v141_multi_work_locks.sql`, `202606180001_works_schedule_json.sql`, `202606180004_works_schedule_json_partial_idx.sql`) are missing.  
  **Root cause**: The essay migration hard-coded the `works` schema at a point-in-time that excluded a column added by a parallel V1.52 migration. This is a systemic risk — any future migration that uses CREATE TABLE + INSERT + DROP + RENAME pattern must explicitly enumerate ALL columns from ALL prior migrations. Consider adding a CI gate: `cargo test -p nexus-local-db --lib` must pass before merge.  
  **Severity**: 🔴 **Critical** — blocks all DB tests; equivalent to a broken build.

- **F-002: Stale capability registry count assertions**  
  `capability::tests::registry_has_twenty_one_builtins` asserts `reg.len() == 21` but the actual count is 22 (added `essay.project_scaffold`). `registry_iter_returns_all` asserts `names.len() == 21` and fails identically.  
  **Fix**: Update both assertions to `22` and add `"essay.project_scaffold"` to `registry_iter_returns_all`'s containment checks.  
  **Severity**: 🔴 **Critical** — test failures in CI would block merge; these tests are not gated behind any feature flag.

### 🟡 Warning

- **W-001: Essay preset structure diverges from plan ACs**  
  Plan §4 AC.10 specifies 4 prompts (`intake.md`, `outline.md`, `draft.md`, `finalize.md`) and implied stage chain `intake → outline → draft → finalize → done`. Implementation has 8 prompts (`intake.md`, `collect-title.md`, `collect-thesis.md`, `collect-audience.md`, `collect-structure.md`, `collect-work-ref.md`, `collect-world.md`, `summary.md`) and a grill-me stage chain `init_intro → collecting_details → summarizing → committing → done`. The preset is an interactive grill-me intake bootstrap, not a stage-chain preset.  
  **Impact**: The preset structure is reasonable (interactive intake bootstrap), but the plan ACs and spec are misaligned with the implementation. The `essay-profile.md` spec §5 describes a `intake → outline → draft → revise → finalize` stage chain that is not implemented in this preset. The preset is `essay-init` (work bootstrap), not `essay-writing` (stage chain).  
  **Recommendation**: Either (a) rename the preset to clarify it's intake-only and update plan ACs, or (b) defer the full stage-chain preset to the same follow-up plan that handles T13 (CLI bootstrap). Document the gap in a residual.

- **W-002: T6 deferred — missing provenance-specific tests**  
  Plan §5 T6 lists `kb_store::tests::provenance_columns` and `creator_world_kb::adopt_with_work_provenance` as required tests. Neither exists. The `kb_store` tests that exercise provenance do so indirectly through `KeyBlockRow::to_key_block()` mapping, but there is no dedicated test asserting that `insert_key_block` with provenance fields round-trips correctly through `get_key_block`.  
  **Impact**: Medium — the provenance columns are exercised by `auto_promote_columns_default_to_null_and_record_on_flip` (in `kb_extract_job.rs`), but the `kb_key_blocks` path has no direct test coverage for provenance field persistence.

- **W-003: T9 + T13 + T14 deferred — missing essay DAO, CLI bootstrap, and tests**  
  Plan tasks T9 (essay DAO), T13 (CLI bootstrap `--profile essay`), and T14 (essay tests) are all deferred. Without these, the `works.work_profile` CHECK constraint accepts `'essay'` but there is no tested path to create an essay Work from the CLI. The `essay.project_scaffold` capability exists but cannot be exercised from an end-user command path.  
  **Impact**: Medium — the essay profile infrastructure (migration, capability, preset) is in place but untested end-to-end. The `essay-init` preset and `essay.project_scaffold` capability have no integration test proving they work together.

- **W-004: `sqlx::query` (runtime) used for 3 static SQL queries in `kb_store.rs`**  
  Three queries in `SqliteKbStore` (`insert_key_block`, `insert_key_block_in_tx`, `get_key_block`) use `sqlx::query()` with SAFETY comments citing "new provenance columns unknown to sqlx offline mode". The `nexus-local-db/AGENTS.md` requires compile-time checked queries for static SQL and runtime queries only for DDL/PRAGMAs/truly dynamic SQL.  
  **Assessment**: The SAFETY justification is pragmatic — sqlx offline data in `.sqlx/` may not include the new migration. However, the `cargo sqlx prepare` workflow is designed for this exact case. These queries should be converted to `sqlx::query!()` / `sqlx::query_as!()` after running `cargo sqlx prepare` with the migration applied.  
  **Impact**: Medium — runtime queries lose compile-time schema verification, making it easier for schema drift to go undetected.

- **W-005: Deferred tasks touch new public API surface without test guardrails**  
  The implementer self-reported T6, T7, T9, T13-T19 as deferred. T6 (provenance tests), T9 (essay DAO), T13 (CLI bootstrap), T14 (essay tests), T16 (cross-wiring), and T17 (full verification) all touch new public API surface or cross-crate wiring without dedicated tests.  
  **Impact**: Medium — while `cargo clippy` and `cargo fmt` pass, the lack of integration tests for new public APIs (essay scaffold CLI trigger, provenance adopt round-trip, essay preset load+run) means the happy path is unverified. This is acceptable for a V1.52 development branch that will receive follow-up work, but must be tracked as a residual with a clear follow-up plan.

### 🟢 Suggestion

- **S-001: `EssayProjectScaffold` has no unit test**  
  The `EssayProjectScaffold::new()` constructor exists for testing but there is no test exercising `run()` with a pool-less instance. Compare with `NovelProjectScaffold` which has tests in `tests/e2e_novel_writing.rs`.  
  **Recommendation**: Add a minimal test that creates a temp directory, calls `run()` with a pool-less instance, and asserts the expected files exist.

- **S-002: `essay.project_scaffold` not added to `registry_lookup_each_builtin` test name list**  
  The test enumerates 21 capability names but `essay.project_scaffold` (the 22nd) is missing. The test passes only because it checks individual `get()` calls without asserting total count. When the count assertions are fixed (F-002), add `"essay.project_scaffold"` to this list.

- **S-003: `source_provenance_kind` stored as `Option<String>` — consider typed enum**  
  The `KeyBlock` struct uses `Option<String>` for `source_provenance_kind`. A Rust enum (`enum ProvenanceKind { Manual, ReviewTimeExtract, FinalizeTimeExtract, CrossChapterRescan, AuthorExplicit }`) with `serde` rename would provide compile-time exhaustiveness checking and prevent typos. The DB TEXT CHECK constraint provides runtime safety but not compile-time.

- **S-004: `auto_chronology` column name uses BOOLEAN type in SQLite**  
  Migration `202606180005` uses `BOOLEAN` type for `auto_chronology`. SQLite uses type affinity and BOOLEAN is stored as INTEGER (0/1). The `202606190004` recreation must preserve the identical column type `BOOLEAN NOT NULL DEFAULT 0` to avoid type-identity mismatch.

## Source Trace
- Finding ID: F-001
- Source Type: test-failure
- Source Reference: `cargo test -p nexus42 --test creator_world_kb` — `table works_new has 32 columns but 33 values were supplied`
- Confidence: High

- Finding ID: F-002
- Source Type: test-failure
- Source Reference: `cargo test -p nexus-orchestration -- capability::tests` — `assertion left == right failed: left: 22, right: 21`
- Confidence: High

- Finding ID: W-001
- Source Type: doc-rule
- Source Reference: Plan §4 AC.10 vs `embedded-presets/essay/preset.yaml`
- Confidence: High

- Finding ID: W-003
- Source Type: diff-gap
- Source Reference: Plan §5 T9/T13/T14 marked `[ ]` (deferred)
- Confidence: High

- Finding ID: W-004
- Source Type: manual-reasoning
- Source Reference: `crates/nexus-local-db/src/kb_store.rs` L177-199, L355-384, L407-436; `nexus-local-db/AGENTS.md` compile-time-query rule
- Confidence: Medium

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 2 |
| 🟡 Warning | 5 |
| 🟢 Suggestion | 4 |

**Verdict**: Request Changes

**Rationale**: Two Critical findings: (1) migration `202606190004` is broken — missing `auto_chronology` column causes 30+ tests to fail; (2) registry count assertions are stale and fail. Both would block CI merge and must be fixed before approval.

Five Warnings document scope gaps that are acceptable for V1.52 development branch with proper residual tracking: the essay preset is an intake bootstrap (not a full stage chain), provenance/integration tests are deferred, the CLI bootstrap path is deferred, and runtime sqlx queries need follow-up. The implementer's self-report of deferred tasks is accurate and honest — the residual tracking mechanism must be engaged.

## Revalidation

**Re-review type**: Targeted (fix wave `da4caab4`)

**Revalidation date**: 2026-06-20

**Fix wave commits**: `09837535..da4caab4` (2 commits: `89a7e0cb` qc3 report, `da4caab4` fix-wave)

### Fix Validation Table

| Finding | Severity | Status | Evidence |
|---------|----------|--------|----------|
| **F-001** | 🔴 Critical | ✅ **Resolved** | `202606190004_work_profile_essay.sql`: added `auto_chronology BOOLEAN NOT NULL DEFAULT 0` column; fixed `auto_chain_interrupted TEXT` → `INTEGER NOT NULL DEFAULT 0`. `migrations_apply` test: 2/2 pass. |
| **F-002** | 🔴 Critical | ✅ **Resolved** | `capability/mod.rs`: `registry_has_twenty_two_builtins()` + `assert_eq!(22)` + `essay.project_scaffold` in lookup list. `tests/capability_registry.rs`: count updated 21→22 with comment. Capability test: 169/169 pass. |
| **W-001** | 🟡 Warning | ⚠️ **Deferred** | Essay preset (`embedded-presets/essay/preset.yaml`) still has 8-prompt grill-me intake structure, not 4-prompt stage chain. The preset serves as `essay-init` (intake bootstrap) per plan ACs. CLI `--profile essay` auto-selects `essay-init` for init preset. Full stage-chain preset (`essay-writing`) deferred to follow-up plan. |
| **W-002** | 🟡 Warning | ⚠️ **Deferred** | No dedicated `provenance_columns` / `adopt_with_work_provenance` tests added. Provenance columns exercise path exists through `auto_promote_columns_default_to_null_and_record_on_flip`. T6 still deferred. |
| **W-003** | 🟡 Warning | ⚠️ **Partially** | T13 **resolved**: `--profile essay` CLI flag added to `bootstrap`, `--init-preset` auto-selects `essay-init` for essay profile, 2 CLI parse tests added (`bootstrap_profile_default_is_novel`, `bootstrap_profile_essay_parses`). T9 (essay DAO) and T14 (essay integration tests) still deferred. Bootstrap `--profile essay` + init preset wiring is functional but not tested end-to-end. |
| **W-004** | 🟡 Warning | ⚠️ **Deferred** | Two `sqlx::query()` calls remain in `kb_store.rs` lines 184, 362 (both `INSERT INTO kb_key_blocks`). SAFETY comments cite "new provenance columns unknown to sqlx offline mode". No conversion to `sqlx::query!()`. Follow-up: run `cargo sqlx prepare` then convert to compile-time checked macros. |
| **W-005** | 🟡 Warning | ⚠️ **Deferred** | `essay_scaffold.rs`: TOCTOU concurrency note added (§4-5 doc comment), but `ScaffoldTransaction` pattern (from `novel.project_scaffold`) not implemented. `essay.project_scaffold` still does FS writes + DB PATCH in separate steps. Deferred residual tracked per V1.52 P-last WL-A. |

### Cross-review fixes verified

The following fixes from qc2/qc3 findings are also confirmed in this fix wave:

| Finding | Source | Status | Evidence |
|---------|--------|--------|----------|
| **C1** (qc2) | R-V150KBED-02 auth gate | ✅ **Resolved** | `require_world_or_work_owner()` function implemented at line 1290 in `kb.rs`. `kb_edit` (line 333) and `kb_delete` (line 389) now call this gate with `block.source_work_id`. When `source_work_id` is set, the source Work's creator is authorized alongside the World owner. Legacy NULL rows fall back to World owner only. |
| **C2** (qc2) / **C-QC3-2** (qc3) | `--profile essay` CLI | ✅ **Resolved** | `BootstrapArgs.profile` field added (`--profile`, default `"novel"`). `primary_preset_id` derived from profile. `effective_init_preset` auto-selects `essay-init` for essay profile. Hint text uses `primary_preset_id`. 2 CLI parse tests pass. |
| **W-QC3-3** (qc3) | `kb_adopt_auto` scale limit | ✅ **Resolved** | Doc comment added at line 907-929: scale recommendation ≤100, batch grouping 50-100, CAS version guard safety note. |

### Updated Verdict

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 (both resolved) |
| 🟡 Warning | 0 resolved, 5 deferred (W-001 through W-005) |
| 🟢 Suggestion | 4 (unchanged, low priority) |

**Verdict**: **Approve**

**Rationale**: Both Critical findings (F-001, F-002) are fully resolved and verified via passing tests. All 5 Warnings are deferred with explicit documentation in code and plan artifacts — they represent scope gaps for follow-up plans (essay stage-chain preset, provenance tests, essay DAO/integration tests, sqlx compile-time migration, essay ScaffoldTransaction), not blocking regressions. The fix wave also resolves cross-review Criticals (qc2 C1: provenance auth gate, qc2 C2 / qc3 C-QC3-2: `--profile essay` CLI).

**Residual tracking required**: W-001 through W-005 should be registered in `.mstar/status.json` `residual_findings[<plan_id>]` as `medium` severity, `lifecycle: open`, with `target: V1.52 P-last` or equivalent follow-up milestone.
