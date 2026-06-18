# QC/QA Consolidated Gate — V1.51 T-B P0 (Advisory Lock)

**Plan**: `2026-06-18-v1.51-advisory-lock`
**Iteration**: V1.51 — KB Closure & Multi-Writer Concurrency
**Integration branch**: `iteration/v1.51`
**Topic branch**: `feature/v1.51-advisory-lock` (merged at `d22075ef`)
**Consolidated at**: 2026-06-18
**PM owner**: `@project-manager`

---

## Tri-review + re-review + QA summary

| Seat | Reviewer | Verdict | Report path |
|---|---|---|---|
| QC1 (architecture / maintainability) | `@qc-specialist` | **Approve** (after W-001 + W-002 fix + re-review) | `plans/reports/2026-06-18-v1.51-advisory-lock/qc1.md` |
| QC2 (security / correctness) | `@qc-specialist-2` | **Approve** | `plans/reports/2026-06-18-v1.51-advisory-lock/qc2.md` |
| QC3 (performance / reliability) | `@qc-specialist-3` | **Approve** | `plans/reports/2026-06-18-v1.51-advisory-lock/qc3.md` |
| QA (full verification) | `@qa-engineer` | **Approve** | `plans/reports/2026-06-18-v1.51-advisory-lock/qa.md` |

**Aggregate verdict**: **Approve** (all 4 seats green).

---

## W-001 + W-002 Warnings — closure trail

| Stage | Date | Commit | What |
|---|---|---|---|
| Initial QC1 raise | 2026-06-18 | `55eaf382` | W-001 (incomplete CLI mutation-path coverage) + W-002 (I/O errors silently mapped to `Locked`) |
| W-002 fix | 2026-06-18 | `3444d046` | `FileLockError { Locked, Io }`; `try_acquire` surfaces I/O error (no `.ok()` swallow); CLI mapping: `Locked` → exit 75, `Io` → exit 78 + `E_LOCK_IO`; `concurrency.md` §2.4 dual exit-code contract |
| W-001 fix | 2026-06-18 | `6dccee36` | `creator run` + `creator world kb adopt` acquire `Works/<work_ref>/.lock` before mutating; 6 new hermetic tests |
| QC1 revalidation | 2026-06-18 | `3a6950d5` + `79141a67` | Verdict flipped Request Changes → Approve; both Warnings concretely resolved |
| QA verification | 2026-06-18 | `a00fab24` | All 13 acceptance criteria pass; 59 lock-relevant tests green (including stress `--test-threads=8`) |

---

## Acceptance criteria (13) — verification status

| # | Criterion | Status |
|---|---|---|
| 1 | `Works/<work_ref>/.lock` flock + heartbeat; hermetic acquire+release | ✓ |
| 2 | `FileLockGuard` RAII drops on scope exit | ✓ |
| 3 | `FileLockError { Locked, Io }`; `try_acquire` surfaces I/O error | ✓ |
| 4 | Daemon cron-side acquires lock before enqueue; CLI returns E_LOCK exit 75 on contention | ✓ |
| 5 | `creator works cron set` returns E_LOCK exit 75 on daemon-held lock | ✓ |
| 6 | `creator run` acquires lock before mutating (W-001 fix) | ✓ |
| 7 | `creator world kb adopt` acquires lock before DB transaction (W-001 fix) | ✓ |
| 8 | `FileLockError::Io` → exit 78 + `E_LOCK_IO` (W-002 fix) | ✓ |
| 9 | Hermetic test asserts exit 78 (NOT 75) for I/O failure (W-002 fix) | ✓ |
| 10 | `knowledge/specs/concurrency.md` Master §1-§6 authored | ✓ |
| 11 | Spec §2.4 documents dual exit-code contract (75 vs 78) | ✓ |
| 12 | `creator works status --json` includes `lock_holder` field (nullable) | ✓ |
| 13 | R-V149P1-01 advisory-lock portion closed in status.json with `closure_evidence` | ✓ |

---

## Stress / race fidelity — verified

- `cargo test -p nexus-local-db --test file_lock -- --test-threads=8`: 3/3 pass
- `cargo test -p nexus-daemon-runtime --test cron_lock_integration -- --test-threads=8`: 3/3 pass
- V1.50 regression: `cron_supervisor` 22/22 + `review_cron_e2e` 2/2 pass

---

## Residuals — registration

- **R-V149P1-01** (low; advisory-lock portion only): closed in fix-wave (`8f625a7f` per qc3 closure_evidence note); `lifecycle: resolved`; closure note specifically calls out advisory-lock portion vs spec-reconciliation portion (which V1.49 P-last closed separately)
- **S-V151Q1-08** (qc3 suggestion; `tempfile::TempDir::into_path()` deprecated): deferred to V1.51 P-last hygiene
- **S-V151Q1-09** (qc3 suggestion; heartbeat write non-atomic): deferred to V1.51 P-last; corruption self-heals on next acquire
- **S-V151Q1-10** (qc1 process note; spec bundled under `qc:` commit): process hygiene note for future rounds, non-blocking

---

## Plan status

**Plan `2026-06-18-v1.51-advisory-lock`** is now **eligible for Done** (all 4 gates green; residuals registered per policy). PM flips status `Todo` → `Done` in `status.json.plans[]` and proceeds to dispatch T-B P1 (per-row OCC) per compass §4 dispatch order.

**Next dispatch**: T-B P1 (`feature/v1.51-per-row-occ` from `iteration/v1.51`) per compass §4 step 4.

---

PM `@project-manager` signs off. T-B P0 closed.