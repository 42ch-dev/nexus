# QC/QA Consolidated Gate — V1.51 T-B P1 (Per-Row OCC)

**Plan**: `2026-06-18-v1.51-per-row-occ`
**Iteration**: V1.51 — KB Closure & Multi-Writer Concurrency
**Integration branch**: `iteration/v1.51`
**Topic branch**: `feature/v1.51-per-row-occ` (merged at `8a970292`)
**Consolidated at**: 2026-06-18
**PM owner**: `@project-manager`

---

## Tri-review + re-review + QA summary

| Seat | Reviewer | Verdict | Report path |
|---|---|---|---|
| QC1 (architecture / maintainability) | `@qc-specialist` | **Approve** (re-review after W-001 fix) | `plans/reports/2026-06-18-v1.51-per-row-occ/qc1.md` |
| QC2 (security / correctness) | `@qc-specialist-2` | **Approve** (re-review after W-001 + W-002 fixes) | `plans/reports/2026-06-18-v1.51-per-row-occ/qc2.md` |
| QC3 (performance / reliability) | `@qc-specialist-3` | **Approve** | `plans/reports/2026-06-18-v1.51-per-row-occ/qc3.md` |
| QA (full verification) | `@qa-engineer` | **Approve** | `plans/reports/2026-06-18-v1.51-per-row-occ/qa.md` |

**Aggregate verdict**: **Approve** (all 4 seats green).

---

## W-001 + W-002 + W-002 — closure trail

| Stage | Date | Commit | What |
|---|---|---|---|
| Initial QC1 raise | 2026-06-18 | `537dbeec` | W-001 (Warning): broken intra-doc link `cas_update_result` → `cas_check` in `crates/nexus-local-db/src/cas.rs:7` |
| Initial QC2 raise | 2026-06-18 | `630531de` | W-001 (Warning): `kb_adopt` discards `actual` from `VersionMismatch` → `VersionConflict { actual_version: None }`; W-002 (Warning): prescribed test target `cron_cas_retry` did not exist |
| Fix-wave W-001 (qc1) | 2026-06-18 | `ef16f12f` | `cas_update_result` → `cas_check` in module-level doc comment |
| Fix-wave W-001 (qc2) | 2026-06-18 | `18dd7390` | `kb_adopt` `map_err` closure on `mark_confirmed_in_tx_with_cas` extracts `actual` from `VersionMismatch`; threads `*actual` into `VersionConflict.actual_version` |
| Fix-wave W-002 | 2026-06-18 | `621f496d` | New `crates/nexus-daemon-runtime/tests/cron_cas_retry.rs` with 3 integration tests (happy/retry/exhaustion) |
| QC1 revalidation | 2026-06-18 | `b110e658` → `e64fd29f` | Verdict Request Changes → Approve |
| QC2 revalidation | 2026-06-18 | `fbd333fa` → `e115c3f4` | Verdict Request Changes → Approve |
| QA verification | 2026-06-18 | `92b7de00` | All 14 acceptance criteria pass + T-B P0 + T-A P0 + V1.50 regression + stress `--test-threads=8` |

---

## Acceptance criteria (14) — verification status

| # | Criterion | Status |
|---|---|---|
| 1 | `kb_extract_jobs.version` + `novel_pool_entries.version` columns (additive) | ✓ |
| 2 | `cas_check` + `with_cas_retry` helpers in `nexus-local-db` | ✓ |
| 3 | `E_VERSION` stable CLI code exit 76 | ✓ |
| 4 | Cron-side retry `with_cas_retry(3, 100)` in `try_fire_role` | ✓ |
| 5 | `kb_adopt` E_VERSION with `actual_version` surfaced (post W-001 fix) | ✓ |
| 6 | User-visible message includes actual_version (not `?`) | ✓ |
| 7 | 2 spec bodies authored | ✓ |
| 8 | Wire contracts unchanged | ✓ |
| 9 | PM clippy fix verified clean | ✓ |
| 10 | R-V151-MERGE-CLIPPY-01 stale residual acknowledged | ✓ |
| 11 | `cargo test -p nexus-daemon-runtime --test cron_cas_retry` runs + passes | ✓ (3/3) |
| 12 | T-B P0 advisory lock preserved | ✓ |
| 13 | T-A P0 LLM extraction preserved | ✓ |
| 14 | V1.50 cron preserved | ✓ |

---

## Stress / race fidelity — verified

- `cargo test -p nexus-local-db --test cas_migration_roundtrip -- --test-threads=8`: 5/5 pass
- `cargo test -p nexus-daemon-runtime --test cron_cas_retry -- --test-threads=8`: 3/3 pass
- V1.50 cron: `cron_supervisor` 22/22 pass

---

## Acquire-order discipline — verified

- `kb_adopt` path: file lock (T-B P0) → DB tx → CAS (T-B P1). Acquire order: file lock first, DB lock second, never reverse.
- `try_fire_role` path: file lock (T-B P0) → `mark_confirmed_in_tx_with_cas` (T-B P1) with `with_cas_retry` wrapper.
- Spec: `concurrency.md` §2.4 documents the dual exit-code contract (75=contention, 78=I/O); §7 documents OCC extension.

---

## Residuals — registration

- **R-V151Q1-08**, **R-V151Q1-09** (T-B P0 carry-over): still deferred to V1.51 P-last hygiene
- **R-V151Q1-10** (T-B P0 process note, archived): already archived
- **R-V150-WLA-DEFER-V1.51** (V1.50 carry-over): still deferred

---

## Plan status

**Plan `2026-06-18-v1.51-per-row-occ`** is now **eligible for Done** (all 4 gates green; residuals registered per policy). PM flips status `Todo` → `Done` in `status.json.plans[]` and proceeds to dispatch T-A P2 (missing-KB detection) per compass §4 step 5.

**Next dispatch**: T-A P2 (`feature/v1.51-missing-kb-detection` from `iteration/v1.51`) per compass §4 step 5.

---

PM `@project-manager` signs off. T-B P1 closed.