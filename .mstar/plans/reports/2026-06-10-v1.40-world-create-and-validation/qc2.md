---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: "2026-06-10-v1.40-world-create-and-validation"
verdict: "Approve"
generated_at: "2026-06-09"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: xai/grok-build-0.1
- Review Perspective: security and correctness risk
- Report Timestamp: 2026-06-09T22:30:00Z

## Scope
- plan_id: 2026-06-10-v1.40-world-create-and-validation
- Review range / Diff basis: iteration/v1.40..feature/v1.40-world-create-and-validation
- Working branch (verified): feature/v1.40-world-create-and-validation
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 11 (core: works.rs handler + novel_scaffold.rs + world_refs_validate.rs + CLI world.rs + tests + preset_gates.rs; supporting: narrative_write.rs create_world (no change), works.rs core, migrations)
- Commit range: abaf514e (HEAD) with 5 commits in range (464d0fba spec amendment, b76c5c1d mandatory binding, 68e4a807 world_refs legacy, a903efd8 initial world create/show, abaf514e tests)
- Tools run: git diff/log, direct file reads of changed paths + AGENTS.md crate files, grep for queries/logging/tests

## Findings

### 🔴 Critical
- (none)

### 🟡 Warning
- **W-01: create_world inside scaffold is not atomic with seed_chapters + patch_work (violates "no partial scaffold" requirement).**  
  In `novel_scaffold.rs:298-336`, when `create_world == true`, `nexus_local_db::create_world` (INSERT into narrative_worlds) is called *before* the `pool.begin()` tx at line 486 that does `seed_chapters_tx` + `patch_work_tx`. On failure after world creation (e.g., chapter seed error, FS error after world row committed), the world row is persisted (orphan world owned by the creator) while the Work is left incomplete or rolled back on the FS side only.  
  Spec §3.5.1.1 and plan T3 explicitly require atomic binding ("no partial scaffold"). The code comment at novel_scaffold.rs:296 claims "inside the same DB transaction" but the implementation does not deliver it.  
  -> Move the `create_world` call inside the seed+patch transaction (or use a single tx that also covers world creation when requested). Add a test that forces failure after world create and asserts the world row is absent.  
  (Source: novel_scaffold.rs:319 (create_world call), 484-561 (separate tx), 298-336 (resolved_world_id path); compare to create_work_atomic in works.rs:353 which does use a single tx.)

- **W-02: World existence FK check does not enforce creator/workspace ownership (cross-creator binding possible).**  
  Both the POST/PATCH guard (`works.rs:544-556`) and the scaffold pre-check (`novel_scaffold.rs:339-354`) use:
  ```rust
  sqlx::query_scalar!(
      r#"SELECT world_id AS "world_id!" FROM narrative_worlds WHERE world_id = ?"#,
      wid
  )
  ```
  This is correctly parameterized and compile-time checked (per daemon-runtime + local-db AGENTS.md). However, it only checks *existence*, with no filter on `owner_creator_id` or `workspace_id` (see migration 20260524_narrative_worlds.sql:7 and indexes on those columns).  
  A caller (authenticated as ctr_A in ws_A) can supply a guessed `world_id` owned by ctr_B / different workspace; the check passes, the Work is created under ctr_A with that foreign world_id, and subsequent World KB / event reads under the bound world may cross isolation boundaries.  
  The `create_world` write path correctly validates the *creator* FK for new worlds (narrative_write.rs:150-161), but the *binding* read path (now mandatory for all V1.40 creation) does not close the loop.  
  Pre-existing for optional world_id; now on the critical path for every new Work.  
  -> Either (a) tighten the existence query to also verify ownership under the caller's creator/workspace (preferred for isolation), or (b) explicitly document+test that cross-creator world binding is allowed and all downstream readers enforce it. Add a test that attempts to bind a world owned by a different creator and asserts the desired behavior.  
  (Source: works.rs:545, novel_scaffold.rs:340, narrative_write.rs:168 (the INSERT that sets owner), migration 20260524, no ownership filter in the SELECTs.)

- **W-03: PATCH can clear world_id on a previously-bound Work (bypass for mandatory binding after creation).**  
  The new guard in `works.rs:544` is:
  ```rust
  if let Some(Some(ref wid)) = non_stage_patch.world_id { ... existence check ... }
  ```
  This only triggers for `Some(Some(wid))`. Supplying `Some(None)` (clear) or omitting the field entirely allows a PATCH to set `world_id = NULL` on a V1.40-created world-bound Work. Nothing in the changed code rejects clearing for novel-profile Works.  
  Combined with the legacy `is_world_bound: false` warn-only path in `world_refs_validate.rs:121-129`, this creates a post-creation downgrade vector that turns a mandatory-bound Work back into a legacy worldless one.  
  -> Add an explicit rule in the PATCH handler (and/or in the works patch layer): for V1.40+ novel Works, reject attempts to clear an existing non-null world_id (or require an explicit "unbind" audit path with --force semantics). Update tests.  
  (Source: works.rs:544 (the if let), 615 (the set clause for world_id), novel_scaffold.rs:530 (patch path), world_refs_validate.rs:121 and its legacy test at 236.)

- **W-04: Insufficient negative/adversarial test coverage for world_id values at the mandatory binding and FK paths.**  
  New tests (works_api.rs: create_work_without_world_id_returns_error; novel_scaffold tests for missing+create_world) cover the happy/missing cases. world_refs_validate has good coverage for empty/duplicate/trim/invalid refs.  
  However, there are no tests exercising the FK existence path (or the creation guards) with malicious or edge `world_id` values: `'`, `--`, `../`, control characters, extremely long strings, non-wld_ prefix, etc. Parameterization makes injection impossible, but absence of negative tests is a correctness/regression gap now that world binding is mandatory and on every creation surface.  
  -> Add a small matrix of adversarial world_id strings to the hermetic tests (both scaffold and works POST) asserting "rejected with clear remediation, no panic, no leak".  
  (Source: works_api.rs diff + novel_scaffold tests; world_refs_validate.rs:164 tests; no adversarial cases in the review range.)

### 🟢 Suggestion
- The choice of `400 Bad Request` with code `WORLD_ID_REQUIRED` (instead of spec's suggested 422) is consistent with existing error style in the handler. For monitoring/alerting this is fine (distinct code), but consider documenting the deviation in the error surface or OpenAPI if 422 is the semantic convention elsewhere.
- No user-supplied `world_id` appears in structured tracing logs with raw interpolation in the changed paths (cloud-sync has some unrelated uses). The remediation messages are clean (only CLI commands, no FS paths or internal IDs beyond the user-supplied wid in the error text itself, which is expected for client errors).
- `--force` handling for world_refs finalize is present and tested (world_refs_validate.rs:221). The escape hatch is gated and documented in the validator.
- Legacy V1.39 worldless Works are correctly left on a warn-only path; creation paths now prevent producing new ones. The risk is the post-creation clear vector (W-03) rather than direct creation bypass.
- CLI additions (world show, --name alias, description) are small, use conflicts_with, and do not introduce new security surfaces.

## Source Trace
- Finding W-01: git diff ... novel_scaffold.rs (create_world call before tx); read of lines 298-336 and 484-561.
- Finding W-02: git diff ... works.rs:544 and novel_scaffold.rs:339; read of query_scalar sites + narrative_write create_world + migration 20260524_narrative_worlds.sql.
- Finding W-03: git diff ... works.rs:544 (the if let Some(Some)); cross-reference with world_refs_validate legacy path.
- Finding W-04: absence in the test diffs + explicit read of the new test bodies and world_refs tests.
- All other claims cross-checked against AGENTS.md files for the four crates (sqlx compile-time requirement, no hand-written DTOs, library-only constraints) and the plan + spec amendment commit 464d0fba.

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 4 |
| 🟢 Suggestion | 5 |

**Verdict**: Request Changes

(The four Warnings are directly tied to the mandatory binding security/correctness story: atomicity of world creation, ownership isolation at binding time, post-creation downgrade, and negative test coverage. No Criticals (no injection, no path traversal, no secret leaks, no bypass of creation gates themselves). Targeted re-review after fixes should focus on the tx scope, the FK query filter, the PATCH clear guard, and added adversarial tests.)

## Revalidation

**Fix context**: W-01 (atomicity of create_world with seed+patch), W-02 (ownership FK on world_id binding), W-03 (PATCH clear of world_id on V1.40+ novel Works), W-04 (adversarial world_id test matrix).

**Diff since previous review**: single commit `d3a18d14` ("fix(world): address QC1/QC2/QC3 findings — world_id validation, atomicity, 422 status"). Review range `iteration/v1.40..feature/v1.40-world-create-and-validation` on branch `feature/v1.40-world-create-and-validation` (HEAD `d3a18d14`). Verified cwd and branch at start of re-review.

**Re-verification** (targeted per finding, all evidence from `git log 9564918f..HEAD`, direct file reads, and required test runs):

- **W-01 atomicity**: `create_world_tx(&mut sqlx::Transaction<'_, sqlx::Sqlite>)` added in `narrative_write.rs:94-144` (validates creator FK, INSERTs narrative_worlds inside caller tx). Wired in `novel_scaffold.rs`:
  - Phase 1 (pre-tx, ~302-361): decide `should_create_world`, validate title/slug, early FK check for pre-existing world_id (with owner_creator_id filter).
  - Phase 2 (inside tx, ~499-604): `let mut tx = pool.begin()...`; if should_create_world: `nexus_local_db::create_world_tx(&mut tx, &inp.creator_id, ...)` (line 510) → `seed_chapters_tx` → `patch_work_tx` (with resolved_world_id) → `tx.commit()`. Explicit comment: "V1.40 (QC2 W-01 / QC3 W-1): create_world is also inside this transaction". FS ScaffoldTransaction remains independent (correct separation). `cargo test -p nexus-orchestration --test novel_project_init` (19/19 passed, including t7g_db_failure_rolls_back_filesystem_scaffold).
- **W-02 ownership FK**: Both binding paths now filter on creator:
  - `novel_scaffold.rs:345-346`: `SELECT EXISTS(SELECT 1 FROM narrative_worlds WHERE world_id = ? AND owner_creator_id = ?)` (binds creator_id; rejects cross-creator with clear remediation).
  - `works.rs:601-602` (PATCH `apply_non_stage_fields`): identical query with `AND owner_creator_id = ?`.
  - Test `create_work_with_other_creators_world_id_returns_error` (works_api.rs:1230): seeds ctr_other + world, POSTs with that world_id, asserts 422 + error. Cross-creator binding rejected at creation and PATCH.
- **W-03 PATCH clear**: Guard in `works.rs:578-597` (inside `apply_non_stage_fields`):
  ```rust
  if non_stage_patch.world_id == Some(None) {
      let current = works::get_work(...) ?;
      if current.world_id.is_some() {
          return Err(NexusApiError::BadRequest { code: "WORLD_CLEAR_FORBIDDEN", ... });
      }
  }
  ```
  Rejects `Some(None)` clear on any Work that currently has a non-null world_id. Test `patch_work_clearing_world_id_on_bound_work_returns_error` (works_api.rs:1285): creates bound novel Work, PATCH with `world_id: Some(None)`, asserts 422 + WORLD_CLEAR_FORBIDDEN in message, then verifies via direct `works::get_work` that world_id is still set. Also covers the non-stage PATCH path.
- **W-04 adversarial tests**: New inline test `create_work_with_adversarial_world_ids_returns_error` (works_api.rs:1354-1401) exercises a 7-value matrix on the POST create_work path (now mandatory world_id):
  - `"wld_' OR 1=1--"` (SQLi)
  - `"wld_; DROP TABLE works--"` (SQLi)
  - `"wld_../etc/passwd"` (path traversal)
  - `"wld_\x00null"` (control / null byte)
  - (very long string)
  - `"not_wld_prefix"`
  - `""`
  All return `is_err()` + 422 (UNPROCESSABLE_ENTITY) + `INVALID_WORLD_ID` in the error. No panics, no leaks, clear remediation. (Note: parameterization already prevents injection; this is regression/negative coverage.)

**Sanity gates (all run during re-review)**:
- `cargo build -p nexus42 -p nexus-daemon-runtime -p nexus-local-db -p nexus-orchestration --all-targets` → success (minor unrelated warnings only).
- `cargo test -p nexus-daemon-runtime --test works_api` → 29 passed (0 failed).
- `cargo test -p nexus-orchestration --test novel_project_init` → 19 passed (0 failed).
- `cargo clippy -p nexus42 -p nexus-daemon-runtime -p nexus-local-db -p nexus-orchestration -- -D warnings` → clean.
- `cargo +nightly fmt --all -- --check` → fmt_exit=0.

**Updated verdict**: Approve. All four prior Warnings (W-01..W-04) are resolved with code + targeted regression tests. No new Critical or Warning findings in the fix diff. HTTP status now consistently 422 for validation errors (aligns with spec and other gates). Security/correctness surface for mandatory world binding is now closed.
