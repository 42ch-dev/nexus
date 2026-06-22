---
report_kind: qa
reviewer: qa-engineer
plan_id: "2026-06-22-v1.59-df47-manuscript-and-misc-capabilities,2026-06-22-v1.59-df12-outbox-consolidation"
verdict: "Pass with notes"
generated_at: "2026-06-22"
mode: "full verify"
---

# Mid-QA Report — V1.59 Wave 1 (P0 DF-47 + P1 DF-12)

## Reviewer Metadata
- Reviewer: @qa-engineer
- Runtime Agent ID: qa-engineer
- Runtime Model: minimax-cn-coding-plan/MiniMax-M3
- Review Perspective: Wave 1 mid-QA — build/test gate, AC verification, integration sanity
- Report Timestamp: 2026-06-22

## Scope
- plan_ids: `2026-06-22-v1.59-df47-manuscript-and-misc-capabilities` (P0) + `2026-06-22-v1.59-df12-outbox-consolidation` (P1)
- Review range / Diff basis: `merge-base: 578be523 + tip: f637c3ef`
- Working branch (verified): iteration/v1.59
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- HEAD (verified): f637c3ef632edaf58e2cbe7bd573ed10cf3ec91e (matches assignment)
- Files reviewed: integration spot-check across host_tool_handlers.rs, capability_registry.rs, host_tool_executor_tests.rs, outbox.rs (orchestration), outbox.rs (cloud-sync), schema.rs (daemon-runtime), acp-capability-set.md, capability-registry.md, outbox-consolidation.md, orchestration-engine.md, daemon-runtime.md
- Commit range: 578be523..f637c3ef (16 commits; all listed in `git log`)
- Tools run: git diff/log/branch, cargo test --all, cargo clippy --all -- -D warnings, cargo +nightly fmt --all -- --check, cargo test catalog_registry_invariant_all_ids_present, cargo test outbox_with_migration_managed_schema_roundtrip, grep/read for AC targets

## Alignment Checks

| Field | Assignment | Verified |
|-------|-----------|----------|
| Working branch | iteration/v1.59 | ✅ `iteration/v1.59` |
| HEAD | starts with `f637c3ef` | ✅ `f637c3ef632edaf58e2cbe7bd573ed10cf3ec91e` |
| Review cwd | /Users/bibi/workspace/organizations/42ch/nexus | ✅ `git rev-parse --show-toplevel` matches |
| Merge-base | 578be523 | ✅ confirmed via `git fetch origin main` (`578be5231f32463cd86ef38bd6af15c7f4bcc3f3`) |
| Plan files | both exist | ✅ both `.mstar/plans/2026-06-22-v1.59-*.md` present |
| QC reports | 6 reports, all Approve | ✅ qc1/qc2/qc3 for both plans (qc2 P0 was revalidated to Approve via commit `a9785239`) |

## Build + Test Gate

### `SQLX_OFFLINE=true cargo test --all`
- **Result**: PASS — 0 failed across all targets (workspace-wide)
- Notable aggregates:
  - `catalog_registry_invariant_all_ids_present`: ✅ 1 passed (filtered out from full run; verified via targeted invocation)
  - `outbox_with_migration_managed_schema_roundtrip` (cloud-sync, `--features legacy-sync`): ✅ 1 passed
  - 9 outbox capability tests (`outbox_flush_no_pool_returns_internal_error`, `outbox_flush_no_entries`, `outbox_flush_with_limit`, `outbox_flush_all_pending`, `outbox_compact_no_pool_returns_internal_error`, `outbox_compact_no_entries`, `outbox_compact_old_acked_removed`, `outbox_compact_recent_acked_preserved`, `outbox_compact_only_targets_acked`): ✅ 9 passed in `cargo test -p nexus-orchestration --lib outbox`
  - 4 capability registry tests: ✅ 4 passed in `cargo test -p nexus-orchestration --test capability_registry`
  - 20 host_tool test vectors (9 capabilities, 20 tests): ✅ all passed (no separate failures)
  - All `test result:` lines report `0 failed`
- **Tail excerpt**:
  ```
  running 0 tests
  test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
  ...
  test capability_registry::tests::catalog_registry_invariant_all_ids_present ... ok
  test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 292 filtered out; finished in 0.00s
  ...
  test outbox::tests::outbox_with_migration_managed_schema_roundtrip ... ok
  test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 190 filtered out; finished in 0.09s
  ```

### `SQLX_OFFLINE=true cargo clippy --all -- -D warnings`
- **Result**: PASS
- **Tail**:
  ```
  Checking nexus-cloud-sync v0.1.0
  Compiling nexus42 v0.1.0
  Checking nexus-moment-context-assembly v0.1.0
  Checking nexus-daemon-runtime v0.1.0
  Finished `dev` profile [unoptimized + debuginfo] target(s) in 16.17s
  ```
- **Pre-existing claim protocol**: Verified clippy is also clean on `origin/main` (`578be523`):
  ```
  Checking nexus42 v0.1.0
  Finished `dev` profile [unoptimized + debuginfo] target(s) in 15.74s
  ```
  **No pre-existing clippy errors** in `nexus42` or `nexus-agent-host` (or any other crate) on either iteration HEAD or origin/main. No PM-override needed.

### `cargo +nightly fmt --all -- --check`
- **Result**: PASS (no output = clean)

### `cargo check -p nexus42`
- **Result**: PASS — confirms `.sqlx/` cache is intact and `nexus42` compiles after PM's restoration of 6 entries in commit `95d3595c`.
  ```
  Checking nexus-daemon-runtime v0.1.0
  Checking nexus42 v0.1.0
  Finished `dev` profile [unoptimized + debuginfo] target(s) in 4.97s
  ```

## P0 (DF-47) Acceptance Criteria Verification

| AC | Result | Evidence |
|----|--------|----------|
| 9 capabilities have `host_tool` bindings in `host_tool_registry()` | ✅ | `grep -c "fn registry_" host_tool_handlers.rs` = **30** total (21 pre + 9 new). New: `registry_manuscript_list`, `registry_manuscript_read_range`, `registry_manuscript_write`, `registry_manuscript_phase_get`, `registry_manuscript_phase_set`, `registry_workspace_paths`, `registry_research_query`, `registry_runtime_health`, `registry_trace_correlation` |
| Host tool count = 30 (was 21) | ✅ | `host_tool_handlers.rs`: 30 `registry_*` functions; 28 `nexus.*` + 2 `fs/*` baseline = 30 (matches `is_shipped_host_tool` match list documented in qc1) |
| `catalog_registry_invariant_all_ids_present` passes | ✅ | Targeted run: `test capability_registry::tests::catalog_registry_invariant_all_ids_present ... ok` |
| Each capability has ≥1 success + ≥1 failure test vector | ✅ | 20 test vectors across 9 capabilities (≥1 success + ≥1 failure each, per capability): `manuscript_list_{returns_manuscripts,rejects_without_active_creator}`, `manuscript_read_range_{returns_bounded_content,rejects_missing_chapter}`, `manuscript_write_{writes_content,rolls_back_word_count_on_rename_failure,rejects_body_path_outside_workspace,rejects_oversized_content}`, `manuscript_phase_get_{returns_current_phase,rejects_cross_creator}`, `manuscript_phase_set_{advances_phase,rejects_invalid_phase}`, `workspace_paths_{returns_allowed_roots,rejects_without_workspace}`, `research_query_{returns_reference_sources,rejects_unknown_reference_id}`, `runtime_health_{returns_agent_visible_status,rejects_without_active_creator}`, `trace_correlation_{propagates_correlation_id,rejects_without_active_creator}` |
| `acp-capability-set.md` §4: 9 rows flipped catalog-only → shipped | ✅ | Section header `## 4. Capability roster (V1.59)`; lines 111, 130-134, 139-141 each carry `Status = shipped, Shipped in = V1.59 P0, Registry row ref = host_tool` for: `workspace.paths`, `manuscript.list`, `manuscript.read_range`, `manuscript.write`, `manuscript.phase.get`, `manuscript.phase.set`, `research.query`, `trace.correlation`, `runtime.health` (9 total) |
| Fix-wave: `execute_manuscript_write` tx-wrapped (W-001) | ✅ | `host_tool_handlers.rs:2236+`: `state.pool().begin()` → UPDATE → `tokio::fs::rename(tmp_file, &abs_body)` → `tx.commit()`; on rename failure `tx` is dropped (rollback). Comment explicitly cites "Mirrors the execute_manuscript_chapter_update C-002 pattern". Regression test `manuscript_write_rolls_back_word_count_on_rename_failure` exercises the rollback path. |
| Fix-wave: canonicalize guard (W-002) | ✅ | `host_tool_handlers.rs:2014-2034` (read_range) and `host_tool_handlers.rs:2182-2202` (write) both perform: `if abs_body.exists() { canonicalize both; check starts_with(workspace_root) } else { lexical prefix check }`. Regression test `manuscript_write_rejects_body_path_outside_workspace` confirms behavior. |
| Fix-wave: deterministic test (W-003) | ✅ | `host_tool_executor_tests.rs` `workspace_paths_rejects_without_workspace` no longer uses conditional accept pattern — calls `result.expect_err(...)` unconditionally with `error_code() == "INVALID_INPUT"`. |

## P1 (DF-12) Acceptance Criteria Verification

| AC | Result | Evidence |
|----|--------|----------|
| `outbox-consolidation.md` Draft spec exists | ✅ | `.mstar/knowledge/specs/outbox-consolidation.md` (Status: Draft (V1.59); Document class: Draft overlay); covers §1 problem summary, §2 single-writer rule, §3 schema ownership boundary, §4 flush semantics, §5 compact semantics, §6 legacy deprecation; cites `orchestration-engine.md` §5.2/§5.7 + `daemon-runtime.md` §10 |
| `outbox.flush` + `outbox.compact` wired to real pool-backed impls | ✅ | `crates/nexus-orchestration/src/capability/builtins/outbox.rs` defines `OutboxFlush::with_pool(SqlitePool)` + `OutboxCompact::with_pool(SqlitePool)` constructors; `run()` methods operate on `outbox_entries` via `sqlx::query!` / `sqlx::query_scalar!` against the injected pool. Doc comment confirms "Pool-backed (production): transitions `staged`/`ready` entries to `acked` state. Platform is paused — this is a local-only DB operation." |
| Legacy `outbox` table deprecation marker (not dropped) | ✅ | `crates/nexus-daemon-runtime/src/db/schema.rs:64-71`: deprecation comment + `tracing::warn!` message referencing `outbox-consolidation.md §6`. Table still created by `crates/nexus-local-db/migrations/20260417_000001_initial.sql:57` (`CREATE TABLE IF NOT EXISTS outbox (...)`). Verified 0 active Rust consumers (qc1 S3 confirmed via grep). |
| Sync CLI regression test passes (`outbox_with_migration_managed_schema_roundtrip`) | ✅ | Targeted run: `cargo test -p nexus-cloud-sync --features legacy-sync outbox_with_migration_managed_schema_roundtrip` → `test outbox::tests::outbox_with_migration_managed_schema_roundtrip ... ok` |
| `orchestration-engine.md` §5.7 amended | ✅ | Lines 337-338 list `outbox.flush` / `outbox.compact` as **Shipped (V1.59)** with `see §5.7` cross-reference. Lines 360, 385-393 contain §5.7 (header `### 5.7 Outbox consolidation (V1.59)`) documenting single-writer rule + flush/compact semantics + deprecation note. |
| `daemon-runtime.md` §11 amended | ✅ | Section `## 11. Outbox flush/compact invocation path (V1.59 P1)` at line 358, references orchestration §5.7 and `CapabilityRegistry::get("outbox.flush") / get("outbox.compact")`. |

## Integration Sanity

| Item | Result | Notes |
|------|--------|-------|
| Both plans' code coexists | ✅ | `host_tool_handlers.rs` (P0) and `outbox.rs` (P1) modified independently; merge conflict resolved by `04fa78d0` (P0 fix-wave) + `baabe536` (P1 merge). Both files compile in same workspace; full `cargo test --all` passes. |
| `.sqlx/` cache intact | ✅ | All 6 restored entries present (`0a467979…`, `17a6af47…`, `185e42a8…`, `84fa6429…`, `8de9605b…`, `d1564e91…`). `cargo check -p nexus42` passes (uses cache). |
| Pre-existing clippy errors | None | `cargo clippy --all -- -D warnings` clean on both `iteration/v1.59` and `origin/main` |
| Both plans registered in status.json | ✅ | `residual_findings[2026-06-22-v1.59-df47-…]` = 2 entries (R-V159P0-001, R-V159P0-002); `residual_findings[2026-06-22-v1.59-df12-…]` = 6 entries (R-V159P1-001..006). All flagged as `severity: low`, matching qc1/qc2/qc3 reports. |

## QC Tri-Review Status (Verification)

| Plan | qc1 | qc2 | qc3 |
|------|-----|-----|-----|
| P0 DF-47 | **Approve with residuals** (`68481e63`) | **Approve** (initial Request Changes in `ef6065b8`; revalidated Approve in `a9785239` after `666eaba5` fix-wave) | **Approve** (`fa7faf8e`) |
| P1 DF-12 | **Approve** (`464b5bf7`) | **Approve** (`c6a42030`) | **Approve** (`40ca5bd8`) |

All 6 reports locked-in `Approve` per `.mstar/plans/reports/2026-06-22-v1.59-*/`. PM consolidated the Warnings as residual findings (R-V159P0-001/002, R-V159P1-001..006) in commit `f637c3ef` — `tech_debt_summary` total_open = 22 (was 14). All open residuals are `low` severity → no blocking findings carried into mid-QA.

## Findings

### 🔴 Critical
- None.

### 🟡 Warning
- None.

### 🟢 Suggestion
- **S-QA-1** (informational): The 9 new host_tool functions add minimal `.rustfmt.toml`-style line counts. Spot-check confirms `cargo +nightly fmt --all -- --check` clean. No drift detected.
- **S-QA-2** (informational): `nexus-cloud-sync` test for the new regression requires `--features legacy-sync` flag. Standard `cargo test --all` runs it without the feature flag but still exercises the migration-managed schema path (verified in targeted test). CI gate (`cargo test --all`) does not pass `--features legacy-sync`; relying on qc1's targeted run is acceptable since the test only requires `legacy-sync` for the `bundle` submodule path. No regression risk for V1.59 P1 closure.
- **S-QA-3** (informational): Two legacy `outbox` table audit points (S2 in qc1 DF-12 + R-V159P1-002 in status.json) suggest tightening spec §3.3 wording about pre-V1.59 data orphaning and moving `tracing::warn!` out of `#[cfg(test)]`. These are Suggestions-class, not blocking, and already tracked in `residual_findings`.

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 3 (informational only) |

**Verdict**: Pass with notes

All Wave 1 acceptance criteria are met for both P0 (DF-47) and P1 (DF-12). Build, test, clippy, and fmt gates all pass on `iteration/v1.59 @ f637c3ef`. No pre-existing clippy errors on `origin/main` (verified via detached-HEAD check). The 8 QC residual findings (R-V159P0-001/002, R-V159P1-001..006) are tracked in `status.json` at `low` severity and do not block Wave 1 closure. P0's qc2 Request-Changes → Approve flow is fully resolved by commit `666eaba5` (W-001/W-002/W-003 all green). P1's `.sqlx/` cache restoration (`95d3595c`) is verified intact.

Wave 1 is ready to proceed to the targeted fix-wave for any future-resolved residuals, or directly to P-mid consolidation if no further fixes are required.

## Notes for PM / Iteration Continuation
- The 8 low-severity residuals are best resolved at V1.59 P-last (hygiene/closeout) per the established V1.x iteration pattern. None of them gate P-mid or P-last.
- PM should verify that `status.json` `metadata.tech_debt_summary` properly attributes the 8 new residuals to the V1.59 row (confirmed: `total_open 14→22` in commit `f637c3ef`).
- Pre-existing claim protocol was not triggered (no clippy errors on origin/main), so no PM-override is needed for this Wave.

## Reproduction / Verification Commands

```bash
# Branch + HEAD alignment
git branch --show-current                          # iteration/v1.59
git rev-parse HEAD                                  # f637c3ef632edaf58e2cbe7bd573ed10cf3ec91e
git rev-parse origin/main                           # 578be5231f32463cd86ef38bd6af15c7f4bcc3f3 (= merge-base)

# Build + test gate
SQLX_OFFLINE=true cargo test --all                  # 0 failed
SQLX_OFFLINE=true cargo clippy --all -- -D warnings # clean
cargo +nightly fmt --all -- --check                # clean

# Targeted AC checks
SQLX_OFFLINE=true cargo test -p nexus-daemon-runtime \
  capability_registry::tests::catalog_registry_invariant_all_ids_present
SQLX_OFFLINE=true cargo test -p nexus-cloud-sync --features legacy-sync \
  outbox_with_migration_managed_schema_roundtrip
SQLX_OFFLINE=true cargo test -p nexus-orchestration --lib outbox   # 9 passed

# Pre-existing clippy verification
git checkout origin/main -- && cargo clippy --all -- -D warnings && git checkout iteration/v1.59 --

# Count host_tool entries
grep -c "fn registry_" crates/nexus-daemon-runtime/src/api/handlers/host_tool_handlers.rs   # 30
```

## Completion Report v2

**Agent**: qa-engineer
**Task**: Mid-QA verification for V1.59 Wave 1 (P0 DF-47 + P1 DF-12)
**Status**: Done
**Scope Delivered**: Full verify mode — build + test gate (test/clippy/fmt), P0 AC checklist, P1 AC checklist, integration sanity, QC tri-review status verification
**Artifacts**: `.mstar/plans/reports/2026-06-22-v1.59-mid-qa/mid-qa.md`
**Validation**: All 7 P0 ACs + 6 P1 ACs + 3 integration items verified. Build/test/clippy/fmt all pass on `iteration/v1.59 @ f637c3ef`. No pre-existing clippy errors on `origin/main`. 8 low-severity residuals tracked in `status.json`.
**Issues/Risks**: None blocking. 3 informational suggestions noted (S-QA-1/2/3). All residual findings (R-V159P0-001/002, R-V159P1-001..006) are `low` severity and best resolved at V1.59 P-last.
**Plan Update**: No plan update required — both P0 and P1 plans have their ACs met. Residual registration in `status.json` was completed by PM in commit `f637c3ef`.
**Handoff**: Wave 1 ready to proceed to P-mid consolidation or V1.59 P-last (hygiene/closeout). No Blocked state.
**Git**: This report will be `git add`'d and committed by `qa-engineer` on `iteration/v1.59` per `mstar-review-qc` rule (artifact Git-in).