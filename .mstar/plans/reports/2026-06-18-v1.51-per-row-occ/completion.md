# Completion Report v2 — V1.51 T-B P1: Per-Row OCC + CAS Generalisation

**Agent**: fullstack-dev-2
**Task**: V1.51 T-B P1 — version columns + CAS generalisation + `E_VERSION` / `E_LOCK` codes + retry-on-conflict
**Status**: **InReview**
**Scope Delivered**: All 10 acceptance criteria (AC #1-#10)
**Plan**: `.mstar/plans/2026-06-18-v1.51-per-row-occ.md`
**Working branch used**: `feature/v1.51-per-row-occ`

---

## Summary

Adds per-row optimistic concurrency control to `kb_extract_jobs` and `novel_pool_entries` via `version INTEGER NOT NULL DEFAULT 0` columns, a reusable CAS helper module (`nexus-local-db::cas`), retry-on-conflict for cron-fire enqueue paths, and `E_VERSION` stable CLI error code (exit 76). Complements V1.51 T-B P0 advisory lock with CAS protection inside the file-lock scope.

---

## Artifacts

### New files (created)

| File | Lines | Purpose |
|------|-------|---------|
| `crates/nexus-local-db/migrations/202606190001_kb_extract_jobs_and_pool_version.sql` | 11 | ADD COLUMN version NOT NULL DEFAULT 0 on both tables |
| `crates/nexus-local-db/src/cas.rs` | 443 | CAS helper: `cas_check()` + `with_cas_retry()` + unit tests (5) |
| `crates/nexus-local-db/tests/cas_migration_roundtrip.rs` | 222 | Migration roundtrip + CAS unit tests (5) |
| `crates/nexus42/tests/cli_version_error.rs` | 113 | E_VERSION display + exit code mapping tests (4) |
| `crates/nexus42/tests/kb_adopt_cas.rs` | 157 | KB adopt CAS integration tests (4) |

### Modified files

| File | Changes | Lines ± |
|------|---------|---------|
| `crates/nexus-local-db/src/error.rs` | Add `VersionMismatch` variant + Display impl | +18 |
| `crates/nexus-local-db/src/lib.rs` | Add `pub mod cas;` | +1 |
| `crates/nexus-local-db/src/kb_extract_job.rs` | Add `version` field to `KbExtractPromotion`; add `mark_confirmed_in_tx_with_cas()`; update 4 SELECT queries to include `version` | +79 |
| `crates/nexus42/src/errors.rs` | Add `VersionConflict` variant + Display impl + `#[allow]` justification | +22 |
| `crates/nexus42/src/main.rs` | Map `VersionConflict` → exit code 76 | +4 |
| `crates/nexus42/src/commands/creator/world/kb.rs` | Use `mark_confirmed_in_tx_with_cas` with preimage version in `kb_adopt`; add `#[allow]` justification | +14 |
| `crates/nexus-orchestration/src/schedule/cron_supervisor.rs` | Add CAS retry loop (3 attempts, 100ms) in `try_fire_role`; add `#[allow]` justification | +20 |
| `.mstar/knowledge/specs/concurrency.md` | Author §7 OCC extension; renumber §7→§8 Status Visibility | +80 |
| `.mstar/knowledge/world-kb-runtime-architecture.md` | Author §6.1 OCC protection subsection | +31 |

**Total**: 5 new files, 9 modified files, ~1215 lines added.

---

## Spec Bodies Authored

1. **`concurrency.md` §7 OCC extension** — Rationale, versioned tables, CAS helper API, KB-side integration (adopt/rescan), cron-side CAS retry, exit code contract, anti-patterns.
2. **`world-kb-runtime-architecture.md` §6.1 OCC protection** — ASCII-art adopt flow with CAS, cross-chapter rescan integration notes, lock ordering.

---

## CAS Integration Map

| Path | CAS usage | Status |
|------|-----------|--------|
| `creator world kb adopt` | `mark_confirmed_in_tx_with_cas(job_id, candidate.version)` — version from promotion-row preimage | **Implemented** |
| `cron_supervisor::try_fire_role` | Retry loop (3 attempts, 100ms backoff) catching `VersionMismatch` from `enqueue_cron_schedule` | **Implemented** (dormant until future T-A paths touch versioned tables) |
| `upsert_pending_candidate` (V1.50 T-B P2) | Version column available on `KbExtractPromotion`; T-A P1 (cross-chapter rescan) caller **must** pass the preimage version | **Infrastructure ready** — doc comment in `kb_extract_job.rs` |
| `insert_pending_with_llm` (T-A P0) | INSERT with `version=0` (default); no CAS needed for INSERT-only paths | **Compatible** |
| `novel_pool_entries::promote_to_active` | Version column added; caller can CAS-guard in future V1.52+ | **Column only** (V1.51 scope: column exists; CAS at call-sites V1.52+) |

### T-A P1 + T-A P2 integration points (documented for parallel implementer)

- T-A P1 (cross-chapter rescan): `upsert_pending_candidate` reads the existing row's `version` via `KbExtractPromotion`. Pass it through the `UPDATE ... WHERE version = ?` guard.
- T-A P2 (missing-KB detection): `insert_pending_with_llm` for new rows (no CAS needed); `mark_confirmed` for adopt (use `mark_confirmed_in_tx_with_cas` with the promotion row's `version`).

---

## Acquire-Order Discipline Verification

- **File lock → DB lock → CAS** (concurrency.md §2.4, §7.5).
- CAS is applied **inside** the file-lock scope in `try_fire_role` (cron_supervisor.rs line ~360).
- `kb_adopt` acquires file lock before DB transaction (existing T-B P0 code, unchanged).
- No reverse acquire-order paths introduced. No `unsafe` code.

---

## Verification

### Test Results (all PASS)

| Command | Result |
|---------|--------|
| `cargo test -p nexus-local-db --test cas_migration_roundtrip` | 5 passed |
| `cargo test -p nexus42 --test cli_version_error` | 4 passed |
| `cargo test -p nexus42 --test kb_adopt_cas` | 4 passed |
| `cargo test -p nexus-local-db --lib` | 245 passed |
| `cargo test -p nexus-local-db --test file_lock` (regression) | 3 passed |
| `cargo test -p nexus42 --test cli_lock_contention` (regression) | 3 passed |
| `cargo test -p nexus-orchestration --test cron_supervisor` (regression) | 22 passed |
| `cargo test -p nexus-orchestration --lib -- llm` (regression) | 50 passed |
| `cargo clippy --all -- -D warnings` | **PASS** (0 errors) |
| `cargo +nightly fmt --all --check` | **PASS** (0 diffs) |

### Acceptance Criteria

| # | Criterion | Status |
|---|-----------|--------|
| 1 | `version` columns added via additive migration; existing rows get `version=0` | ✅ `cas_migration_roundtrip` tests verify |
| 2 | CAS helper: `cas_check()` + `with_cas_retry()`; mismatch test | ✅ `cas.rs` 5 tests |
| 3 | `creator world kb adopt` on stale preimage returns `E_VERSION` exit 76 | ✅ `kb_adopt_cas` + `cli_version_error` tests |
| 4 | Cron-fire enqueue uses retry loop (3 attempts, 100ms) | ✅ retry loop in `try_fire_role`; `with_cas_retry` tested in `cas.rs` |
| 5 | `world-kb-runtime-architecture.md` §6 OCC subsection authored | ✅ §6.1 |
| 6 | `concurrency.md` §7 OCC extension body authored | ✅ §7 + renumber §7→§8 |
| 7 | Wire contracts unchanged | ✅ No `schemas/` changes |
| 8 | No `#[allow(…)]` without justification comment | ✅ 3 justified allows: `too_many_lines` ×2 (pre-existing +5/+8 lines), `too_many_arguments`+`too_many_lines` (pre-existing +12 lines) |
| 9 | No race condition regressions | ✅ All regression tests pass; `--test-threads=1` used for CAS tests |
| 10 | T-B P0 advisory lock integration preserved | ✅ `file_lock` and `cli_lock_contention` tests pass; CAS inside file-lock scope |

---

## Risks / Follow-Ups

1. **CAS retry on cron path is dormant**: today `enqueue_cron_schedule` only touches unversioned `creator_schedules`. The retry activates when future T-A P1/P2 paths within the fire scope write to `kb_extract_jobs`. Documented in code comment.
2. **`novel_pool_entries` CAS call-sites deferred**: the `version` column exists, but `promote_to_active` / `archive_pool_entry` / `mark_completed` do not yet CAS-guard their writes. V1.52+ scope.
3. **`.sqlx/` cache refresh**: migration added; CI runners will regenerate `.sqlx/` on first build. Local developers may need `cargo sqlx prepare --workspace` if schema-discrepancy errors appear.
4. **No deviations from plan** — all tasks T1-T8 completed as specified.

---

## Git Context

```
Worktree path: /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.51-t-b-p1
Working branch: feature/v1.51-per-row-occ
Base: iteration/v1.51 (HEAD 388602d2)

Commit:
99df7b70 feat(nexus-local-db,nexus42,nexus-orchestration): per-row OCC + CAS generalisation + E_VERSION code

git log --oneline iteration/v1.51..HEAD:
99df7b70 feat(nexus-local-db,nexus42,nexus-orchestration): per-row OCC + CAS generalisation + E_VERSION code
```
