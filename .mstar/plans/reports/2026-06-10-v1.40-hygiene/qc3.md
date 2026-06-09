---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: "2026-06-10-v1.40-hygiene"
verdict: "Request Changes"
generated_at: "2026-06-10"
---

# Code Review Report

## Reviewer Metadata
- **Reviewer**: @qc-specialist-3
- **Runtime Agent ID**: qc-specialist-3
- **Runtime Model**: k2p6
- **Review Perspective**: performance and reliability risk
- **Report Timestamp**: 2026-06-10T00:00:00Z

## Scope
- **plan_id**: `2026-06-10-v1.40-hygiene`
- **Review range / Diff basis**: `iteration/v1.40..feature/v1.40-hygiene` (equivalently `cece6439..76a5461d`)
- **Working branch (verified)**: `feature/v1.40-hygiene`
- **Review cwd (verified)**: `/Users/bibi/workspace/organizations/42ch/nexus`
- **Files reviewed**: 10 files changed, 300 insertions(+), 43 deletions(-)
- **Commit range**: `cece6439..76a5461d` (6 commits)
- **Tools run**: `cargo +nightly fmt --all -- --check`, `cargo clippy --all -- -D warnings`, `cargo test --all`, `sqlite3 :memory:`, `git diff`

## Findings

### 🔴 Critical

#### C-1: SQLite migration uses unsupported `ALTER TABLE ... ADD CONSTRAINT` syntax
- **Files**: `crates/nexus-local-db/migrations/202606100002_findings_check_constraints.sql`
- **Issue**: The migration attempts `ALTER TABLE findings ADD CONSTRAINT chk_findings_severity CHECK (...)` (and two more identical patterns). SQLite **does not support** `ALTER TABLE ... ADD CONSTRAINT`. Executing this migration on an existing database will produce a syntax error and abort the migration, leaving the schema in a partially-applied state.
- **Evidence**: Verified with `sqlite3 :memory:` — returns `Error: in prepare, near "CONSTRAINT": syntax error`.
- **Impact**: All existing installations will fail to migrate, breaking startup. The runtime `validate_finding_enums()` guard is also bypassed because the migration never completes.
- **Fix**: Use the SQLite table-recreation pattern: (1) `PRAGMA foreign_keys=OFF`, (2) `CREATE TABLE findings_new (... CHECK constraints ...)`, (3) `INSERT INTO findings_new SELECT * FROM findings`, (4) `DROP TABLE findings`, (5) `ALTER TABLE findings_new RENAME TO findings`, (6) re-create indexes, (7) `PRAGMA foreign_keys=ON`. Also verify existing data does not violate the constraints before copying.

### 🟡 Warning

#### W-1: Test compilation broken by new `PatchWorkRequest` field
- **Files**: `crates/nexus-daemon-runtime/tests/works_api.rs` (15 occurrences), `crates/nexus-daemon-runtime/src/api/handlers/works.rs:1332`
- **Issue**: Commit `c47d2125` added `auto_chain_interrupted: Option<bool>` to `PatchWorkRequest`, but 16 Rust struct literals (15 in integration tests, 1 in `#[cfg(test)]` block) were not updated. `cargo test -p nexus-daemon-runtime` fails with 16 `error[E0063]: missing field` errors.
- **Evidence**: `cargo test -p nexus-daemon-runtime --no-run` output shows 15 errors in `tests/works_api.rs` and 1 in `src/api/handlers/works.rs`.
- **Impact**: Tests cannot compile; CI gate would fail. Regression from base commit `cece6439`.
- **Fix**: Add `auto_chain_interrupted: None` to all 16 `PatchWorkRequest { ... }` struct literals. Alternatively, derive `Default` for `PatchWorkRequest` and append `..Default::default()` to existing literals.

#### W-2: sqlx compile-time macros regressed to runtime queries in supervisor
- **Files**: `crates/nexus-orchestration/src/schedule/supervisor.rs` (2 query sites)
- **Issue**: The PR converted two `sqlx::query_as!` (compile-time checked macros) to `sqlx::query_as::<_, ScheduleRow>()` (runtime queries) when adding the `WHERE status IN ('pending', 'running', 'paused')` filter. Per `crates/nexus-daemon-runtime/AGENTS.md`: *"All new sqlx queries MUST use compile-time checked macros... Do NOT use runtime `sqlx::query()` or `sqlx::query_as::<T>()` for static SQL."* The WHERE clause is constant and user-controlled; it is static SQL.
- **Evidence**: Diff shows `sqlx::query_as!` replaced by `sqlx::query_as::<_, ScheduleRow>` at both `tick_inner` call sites (lines ~156 and ~812). SAFETY comments claim "dynamic SQL" but the filter is a literal string.
- **Impact**: Loss of compile-time schema validation; drift risk if `creator_schedules` schema changes. No `.sqlx/` cache update was committed (confirmed: `git diff --stat` shows zero `.sqlx/` changes).
- **Fix**: Restore `sqlx::query_as!` macros with the `WHERE` clause, run `cargo sqlx prepare --workspace --all -- --all-targets`, and commit updated `.sqlx/` metadata.

### 🟢 Suggestion

#### S-1: Build-time verification for `preset_version_for_id` drift
- **Files**: `crates/nexus-orchestration/src/auto_chain.rs`
- **Issue**: `preset_version_for_id()` hardcodes version numbers (e.g., `novel-writing` → 7) that must be manually kept in sync with `embedded-presets/*/preset.yaml` `version:` fields. There is no automated guard against drift.
- **Impact**: If a preset YAML is bumped but this mapping is forgotten, the DB stores the stale version (fallback 1), causing loader compatibility issues.
- **Fix**: Add a compile-time or test-time check that reads each `preset.yaml` and asserts the mapping matches. A `build.rs` script or a `#[test]` that scans `embedded-presets/` would prevent silent drift.

#### S-2: Consider `#[serde(default)]` on `auto_chain_interrupted` for API backward compatibility
- **Files**: `crates/nexus-daemon-runtime/src/api/handlers/works.rs`
- **Issue**: `PatchWorkRequest` does not mark `auto_chain_interrupted` with `#[serde(default)]`, unlike the `force` field. JSON payloads omitting the field will fail deserialization.
- **Impact**: Minor — existing API consumers (e.g., e2e tests, scripts) that construct `PatchWorkRequest` JSON without the new field will receive a deserialization error.
- **Fix**: Add `#[serde(default)]` above `auto_chain_interrupted` to make the field optional in JSON deserialization.

## Source Trace

| Finding ID | Source Type | Source Reference | Confidence |
|------------|-------------|------------------|------------|
| C-1 | manual-reasoning + static-analysis | `sqlite3 :memory:` syntax error + migration SQL file | High |
| W-1 | compiler-error | `cargo test -p nexus-daemon-runtime --no-run` (16× E0063) | High |
| W-2 | static-analysis + doc-rule | `supervisor.rs` diff + `AGENTS.md` sqlx policy | High |
| S-1 | manual-reasoning | `auto_chain.rs` `preset_version_for_id()` + `preset.yaml` diff | Medium |
| S-2 | manual-reasoning | `works.rs` `PatchWorkRequest` struct definition | High |

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 1 |
| 🟡 Warning | 2 |
| 🟢 Suggestion | 2 |

**Verdict**: Request Changes

**Rationale**:
- **C-1 (Critical)**: The CHECK constraint migration is invalid SQLite and will break all existing databases at startup. This is a hard reliability blocker.
- **W-1 (Warning)**: Tests do not compile. While the fix is trivial, uncompiled tests block CI and prevent regression verification.
- **W-2 (Warning)**: Regressing from compile-time `sqlx::query_as!` to runtime `sqlx::query_as::<T>()` violates project policy and removes schema safety.

Once C-1 is resolved (migration rewritten using table recreation) and W-1 is fixed (test struct literals updated), the PR can be re-reviewed. W-2 should ideally be addressed by restoring compile-time macros; if the team accepts the runtime tradeoff for pragmatic reasons, it must be explicitly documented in the PR with PM sign-off.

## Checklist (performance + reliability)

- [x] Is the CHECK constraint migration additive and fast? → **No — migration syntax is invalid SQLite (C-1)**
- [x] Does the runtime enum validation add measurable overhead per insert? → No — 3× slice `contains` on 4 items each; negligible O(1) cost.
- [x] Does the ULID suffix have proper entropy (avoid brute force)? → Yes — 24-bit monotonic counter + millisecond timestamp provides sufficient collision resistance for single-process local use.
- [x] Does the resume synchronous tick add latency to the resume path? → Yes, bounded — `tick()` is guarded against re-entrancy and scoped to actionable schedules only. Acceptable UX tradeoff.
- [x] Does the scoped `tick_inner` SELECT reduce query cost? → Yes — `WHERE status IN ('pending', 'running', 'paused')` is backed by index `creator_schedules_by_status`. Reduces memory and CPU as schedule history grows.
- [x] Does the CLI status timeout value (e.g., 5s, 10s) make sense for a local daemon? → Verified — `DaemonClient` enforces `DEFAULT_REQUEST_TIMEOUT = 30s` (`daemon_client.rs:43`). Acceptable for local daemon.
- [x] Are tests fast and hermetic? → **No — tests fail to compile (W-1)**
- [x] `cargo +nightly fmt --all -- --check` clean? → Yes.
- [x] `cargo clippy --all -- -D warnings` clean? → Yes (lib/bins; tests fail to compile separately).
- [x] Is there logging overhead from new `tracing::warn!` calls? → No — `warn!` in `create_finding_from_review` error path and `patch_work` resume tick error path are both cold paths.
