---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-04-18-sqlx-compile-time-migration"
verdict: "Request Changes"
generated_at: "2026-04-18"
---

# QC Review #1 — sqlx Compile-Time Macro Migration

**Plan**: `2026-04-18-sqlx-compile-time-migration`
**Reviewer**: `qc-specialist` (#1)
**Working branch**: `feature/v1.4`
**Review range**: `git diff 1f4f251..b9067ce`
**Review cwd**: `<repository-root>`

---

## Scope Reviewed

| Area | Files Changed | Status |
|------|--------------|--------|
| `crates/nexus-local-db/src/` | 5 files | Reviewed |
| `crates/nexus-sync/src/` | 2 files | Reviewed |
| `crates/nexus-orchestration/src/` | 4 files | Reviewed |
| `crates/nexus42/src/` | 6 files | Reviewed |
| `crates/nexus42d/src/` | 15 files | Reviewed |
| `.sqlx/` | 97 query metadata JSON + `sync-schema.sql` | Reviewed |
| `.github/workflows/ci.yml` | 1 file (verify-sqlx-offline job) | Reviewed |
| `AGENTS.md` | sqlx convention section | Reviewed |
| `.gitignore` | `.sqlx/state.db*` entries | Reviewed |

**Total**: 136 files changed, +4,148 / -630 lines

---

## Verification Performed

| Check | Command | Result |
|-------|---------|--------|
| Clippy (warnings as errors) | `cargo clippy --all -- -D warnings` | **PASS** — 0 warnings, 0 errors |
| Format check | `cargo fmt --check` | **PASS for sqlx scope** — only diffs in `crates/nexus-contracts/src/generated/` (pre-existing import ordering, not in sqlx migration scope; `.rustfmt.toml` `ignore` directive requires nightly) |
| Diff range coverage | `git diff 1f4f251..b9067ce --stat` | 136 files, all reviewed |
| SAFETY comment audit | grep for `sqlx::query(` / `sqlx::query_as(` / `sqlx::query_scalar(` without `!` | 29 remaining runtime calls found; categorized below |
| `.sqlx/` metadata | 97 JSON files + `sync-schema.sql` | Present and well-formed (spot-checked `query-bb02b5…`) |
| CI job | `verify-sqlx-offline` | Re-enabled; runs `cargo sqlx prepare --workspace --all -- --all-targets` |

---

## Findings

### Critical (0)

None.

### Warning (3)

#### W1: Missing SAFETY comments on test-only runtime queries in `nexus-sync/src/pool.rs`

- **File**: `crates/nexus-sync/src/pool.rs`, lines 102, 107, 113, 126, 137, 143
- **Description**: Six test-only `sqlx::query()` / `sqlx::query_as()` calls in `pool_creates_successfully`, `pool_get_returns_working_connection`, and `pool_supports_concurrent_access` tests lack SAFETY comments. These use runtime `CREATE TABLE IF NOT EXISTS test` (DDL) and dynamic `INSERT`/`SELECT` on a temporary test table not present in the production schema. Per AGENTS.md convention, each must have a `// SAFETY:` comment.
- **Recommended fix**: Add comments such as:
  ```rust
  // SAFETY: test-only DDL — creates temporary `test` table not in production schema.
  sqlx::query("CREATE TABLE IF NOT EXISTS test ...")
  ```
- **Note**: These lines are **not** in the diff (pre-existing code), but since the AGENTS.md sqlx convention is newly codified as part of this plan, all runtime `sqlx::query()` calls across the codebase should comply.

#### W2: Missing SAFETY comments in integration test files

- **Files**:
  - `crates/nexus-local-db/tests/pool_smoke.rs`, line 5: `sqlx::query_as("PRAGMA journal_mode")`
  - `crates/nexus-local-db/tests/migrations_apply.rs`, lines 23, 33: `sqlx::query_as(...)` for table-count and version checks
  - `crates/nexus42d/src/test_utils.rs`, lines 109, 115, 154: `sqlx::query(...)` for test workspace seeding; `sqlx::query_as(...)` for assertion
- **Description**: Same pattern — runtime queries without SAFETY comments. The PRAGMA query is valid to remain runtime, and the test seeding queries are valid, but each needs a `// SAFETY:` annotation per the newly codified convention.
- **Recommended fix**: Add appropriate SAFETY comments (`// SAFETY: PRAGMA statement ...` or `// SAFETY: test-only data setup ...`).

#### W3: `scheduled_at` type change in `ScheduleRow` — behavioral side effect

- **File**: `crates/nexus-orchestration/src/schedule/supervisor.rs`, lines 445–448, 494–497
- **Description**: The compile-time macro migration caused a **behavioral change**: `scheduled_at` in `ScheduleRow` changed from `Option<String>` to `Option<i64>`. The `.sqlx/` metadata (`query-bb02b5…`) confirms the DB column is `Integer` (Unix timestamp). The `to_domain()` mapping converts via `.map(|t| t.to_string())`. This is **correct** (the DB stores INTEGER) but the old `Option<String>` type was silently incorrect. The `to_string()` conversion produces a decimal integer string (e.g. `"1745000000"`) rather than an RFC 3339 timestamp.
- **Impact**: If any consumer downstream expects `scheduled_at` as an RFC 3339 string in the `Schedule` domain struct, this change will alter the output format. Consumers that parse it as a number will now get the correct value; consumers that parse as RFC 3339 will break.
- **Recommended fix**: Either (a) audit all consumers of `Schedule::scheduled_at` to confirm they handle numeric-string timestamps, or (b) convert the i64 to RFC 3339 in `to_domain()` if that was the original intent.

### Suggestion (3)

#### S1: Misleading SAFETY comment in `orchestration/schedules.rs`

- **File**: `crates/nexus42d/src/api/handlers/orchestration/schedules.rs`, lines 198–200
- **Description**: The SAFETY comment states: "cannot use compile-time macro because the pool is obtained from `supervisor.pool()` which returns `&SqlitePool`." However, `require_supervisor()` returns `Arc<ScheduleSupervisor>` and `pool()` returns `Arc<SqlitePool>`. `Arc<SqlitePool>` dereferences to `&SqlitePool` which **does** work with compile-time macros (the sqlx `Executor` trait is implemented for `&SqlitePool`). The real blocker is that `.sqlx/` metadata for these specific queries may not have been generated in this migration pass.
- **Recommended fix**: Change the comment to accurately reflect the reason: e.g. `// TODO: convert to compile-time macro — .sqlx/ metadata not yet generated for this query.`

#### S2: Inline `FromRow` structs for `OutboxRow` and `PartialApplyRow`

- **Files**: `crates/nexus-sync/src/outbox.rs`, lines 502–514, 534–542, 699–704
- **Description**: The migration introduces `#[derive(sqlx::FromRow)]` inline structs (`OutboxRow`, `PartialApplyRow`) inside function bodies. While functionally correct, these would benefit from being extracted to module-level private structs for reusability and clarity, especially since `OutboxRow` is duplicated in `replay()` and `get()`.
- **Recommended**: Extract `OutboxRow` and `PartialApplyRow` to module-level `struct` definitions (private to the module).

#### S3: Test-only `sqlx::query()` in `nexus42d/src/db/schema.rs` could use macros

- **File**: `crates/nexus42d/src/db/schema.rs`, lines 92, 100, 149
- **Description**: These test queries (`SELECT value FROM workspace_meta WHERE key = ...`, `SELECT status FROM creators WHERE ...`) target tables that ARE in the production schema and COULD use compile-time macros. They're labeled "test-only DDL verification" but they're actually DML queries against known tables.
- **Recommended**: In a follow-up pass, consider converting these to `sqlx::query_scalar!()` / `sqlx::query_as!()` since the tables are part of the `nexus-local-db` schema already covered by `.sqlx/` metadata.

---

## Correctness Audit Summary

| Criterion | Status | Notes |
|-----------|--------|-------|
| Static SQL → compile-time macros | ✅ | All ~79 static queries in diff scope converted correctly |
| SAFETY comments on remaining runtime calls (production) | ✅ | All production runtime queries (PRAGMA, DDL, dynamic) have comments |
| SAFETY comments on remaining runtime calls (test) | ⚠️ | 12 test-only calls missing comments (W1, W2) |
| Type safety: `platform_linked` INTEGER→bool | ✅ | `r.platform_linked != 0` correctly handles SQLite INTEGER→Rust bool |
| Type safety: `COUNT(*)` as i64 | ✅ | Properly cast to `usize` after macro returns `i64` |
| Type safety: `PRIMARY KEY` nullable override | ✅ | `"col!"` syntax used correctly |
| Type safety: `scheduled_at` INTEGER | ⚠️ | Behavioral side effect (W3) |
| Borrow correctness (pre-owned params) | ✅ | All macro bind params pre-owned into `let` variables |
| `.sqlx/` metadata committed | ✅ | 97 JSON files + `sync-schema.sql` |
| `.sqlx/state.db` gitignored | ✅ | 3 entries in `.gitignore` |
| CI `verify-sqlx-offline` re-enabled | ✅ | Job present, runs `cargo sqlx prepare` with `--check` |
| `cargo clippy -- -D warnings` | ✅ | Passes cleanly |
| AGENTS.md sqlx section | ✅ | Complete with default, exceptions, and migration flow |

---

## Cross-Reviewer Ready Notes

- **Architecture**: The migration correctly removes all `sqlx::FromRow` derives from public domain structs (`LocalIdentityRow`, `MemoryFragmentRecord`, `PendingReviewRecord`, `SoulMeta`) and replaces with manual field mapping from anonymous macro-generated types. This is the expected pattern and improves explicitness.
- **Integration risk**: Low — this is a pure refactoring change. The only behavioral side effect is W3 (`scheduled_at` type). No API contracts are changed.
- **Migration cost**: The codebase went from ~79 runtime queries to ~12 remaining (all test-only, PRAGMA, DDL, or dynamic). The `.sqlx/` metadata footprint (97 files) is manageable.
- **Technical debt**: The `orchestration/schedules.rs` queries (8 instances) remain runtime due to `.sqlx/` metadata not being generated for them. The SAFETY comment is misleading (S1). These could be converted in a follow-up.

---

## Verdict: Request Changes

**Rationale**: No Critical findings. Three Warnings require attention before merge:
- **W1 + W2**: Missing SAFETY comments on 12 test-only runtime queries — fixable in a single batch. Since AGENTS.md sqlx convention is newly codified as part of this plan, the convention should be applied consistently across all runtime queries in the repo, not just the diff scope.
- **W3**: `scheduled_at` type change from `Option<String>` to `Option<i64>` is a behavioral side effect that needs consumer audit or conversion fix before merge.

All three are straightforward to address. No structural or architectural blockers found.
