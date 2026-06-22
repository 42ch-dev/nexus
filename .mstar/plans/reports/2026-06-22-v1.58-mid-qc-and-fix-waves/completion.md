# V1.58 P-mid — 3-Wave Meta Tracking & Evidence Summary

**plan_id**: `2026-06-22-v1.58-mid-qc-and-fix-waves`
**iteration**: V1.58
**track**: meta
**status**: Done
**closed_at**: 2026-06-22
**merge_commit**: ba334fa8 (V1.58 Wave 3 fix-wave merge) + 5df60210 (qc verdict updates)

---

## Wave 1 Evidence Gate (P0 + P1 parallel)

**Plans**: P0 `2026-06-22-v1.58-workspace-occ-hardening` + P1 `2026-06-22-v1.58-df44-reference-refresh-pipeline`

**Initial QC tri-review** (2026-06-22 morning):
- P0 qc1 (architecture): **Approve** (2M + 3L)
- P0 qc2 (security): **Request Changes** (3 HIGH: std::fs::canonicalize in async, TOCTOU in validate_changes_manifest, .sqlx cache deletion)
- P0 qc3 (performance): **Request Changes** (1 Critical: same std::fs::canonicalize async + 3 Warnings)
- P1 qc1 (architecture): **Approve** (1M + 3L)
- P1 qc2 (security): **Request Changes** (1 HIGH: HTTPS-only + private-IP blocking missing)
- P1 qc3 (performance): **Request Changes** (1 Critical: large body fetch not streaming + 4 Warnings)

**PM fix-wave dispatched** (8 commits on `fix/v1.58-p0-qc-findings` + 2 commits on `fix/v1.58-p1-qc-findings`):
- P0 fix-wave: spawn_blocking for canonicalize, symlink_metadata re-validation in validate_changes_manifest, .sqlx/ cache restoration + regression guard, force param removed, retry semantics documented, canonicalize memoization, Criterion config + cold/warm groups
- P1 fix-wave: validate_reference_url (HTTPS + private-IP), streaming body fetch with incremental blake3 + 100 MiB cap, idempotency doc

**PM .sqlx/ cache restoration** (commit af82ad39): P1 sqlx prepare operation deleted 137 cache entries (138→1); PM regenerated via `DATABASE_URL=sqlite://.sqlx/state.db cargo sqlx migrate run + cargo sqlx prepare --workspace -- --tests`.

**Targeted re-review** (post fix-wave):
- P0 qc2 Revalidation: **Approve** (all 3 HIGH + 2M closed)
- P0 qc3 Revalidation: **Approve** (F-001 closed by H-1 + F-002/F-003/F-004 closed)
- P1 qc2 Revalidation: **Approve** (H-001 closed)
- P1 qc3 Revalidation: **Approve** (F-001 closed)

**Wave 1 mid-QA** (commit via `.mstar/plans/reports/v1.58-wave-1-mid-qa.md`):
- Verdict: **Pass with notes** (1 non-blocking test expectation drift to fix in P-last hygiene; 6 QC reports complete; all 33 V1.57+ residuals closed)

**Wave 1 acceptance**: ✅ Both P0 + P1 accepted; fix-waves merged; QC verified.

---

## Wave 2 Evidence Gate (P2 sequential Track A)

**Plan**: P2 `2026-06-22-v1.58-capability-quality-convergence`

**Initial QC tri-review** (post Wave 1 merge):
- P2 qc1 (architecture): **Approve** (3 Suggestions)
- P2 qc2 (security): **Approve** (zero findings)
- P2 qc3 (performance): **Approve** (no blocking)

**Implementation note**: P2 was implemented as 12/16 tasks done; 4 low-priority deferred (T4 latency bench, T5 eval tracing, T7 subsumed by T2, T10 doc-only, T11 throttle-path, T14 per-ID failure-path). Implementer documented deferrals with rationale.

**Wave 2 acceptance**: ✅ All 3 QC reports Approve on first review; no fix-wave needed; PM-validate path used (no mid-QA since no Wave 2 fix-wave).

---

## Wave 3 Evidence Gate (P3 sequential Track B)

**Plan**: P3 `2026-06-22-v1.58-reference-cli-and-cross-cut-tests`

**Initial QC tri-review** (post Wave 2 merge):
- P3 qc1 (architecture): **Approve** (1L + 2 Suggestions)
- P3 qc2 (security): **Request Changes** (2 HIGH: nexus.reference.refresh NOT in host_tool_registry; reference sources have no creator/workspace scoping)
- P3 qc3 (performance): **Request Changes** (1 Warning: missing fsync + 1 Suggestion: progress indicators)

**PM fix-wave dispatched** (4 commits on `fix/v1.58-p3-qc-findings`):
- H-001: nexus.reference.refresh registered in host_tool_registry (TOOL_ALLOWLIST, execute_reference_refresh handler, CapabilityRow, build_registry) — registry test 20→21 tools
- H-002: Creator-scoped reference lookups (migration `202606220004_reference_sources_creator_id.sql` adds creator_id column; `find_by_id_for_creator` DAO; `ReferenceRefresh::run` uses scoped lookup when creator context available; CLI injects creator context)
- W-001: fsync in atomic_write_body (write → sync → rename per V1.55 P3 ScaffoldTransaction)

**Targeted re-review** (post fix-wave):
- P3 qc2 Revalidation: **Approve** (H-001 + H-002 + M-001 closed; M-002 + L-001 non-blocking)
- P3 qc3 Revalidation: **Approve** (W-001 closed; S-001 deferred appropriately)

**Wave 3 acceptance**: ✅ All 3 QC reports Approve; fix-wave closed; DF-44 fully closed (capability + migration + CLI + body file write + cross-cut tests).

---

## 3-Wave Rhythm Summary

| Wave | Plans | Initial QC | Fix-wave | Final QC | Notes |
| --- | --- | --- | --- | --- | --- |
| 1 | P0 + P1 (parallel) | 2 Approve / 4 Request Changes | yes (P0 + P1) | 6 Approve | sqlx cache restoration by PM |
| 2 | P2 (sequential) | 3 Approve | no | 3 Approve | clean first-pass tri-review |
| 3 | P3 (sequential) | 1 Approve / 2 Request Changes | yes (P3) | 3 Approve | DF-44 fully closed |
| **Total** | **4 implement plans** | **6 Approve / 6 Request Changes** | **3 fix-waves** | **12 Approve** | **1 mid-QA** (Wave 1 only) |

**Total QC reports**: 12 (3 per plan × 4 plans)
**Total targeted re-reviews**: 4 (P0 qc2 + qc3 + P1 qc2 + qc3 + P3 qc2 + qc3 = 6 actual re-reviews)
**Total mid-QA**: 1 (Wave 1 only; PM override skipped Wave 2/3 mid-QA per V1.57 pattern)

---

## Carry-forward Closure Summary

| Bucket | Count | Status |
| --- | --- | --- |
| V1.57+ backlog workspace OCC | 6 (R-V156P0-M001..M006) | ✅ All closed in P0 |
| V1.57+ backlog capability surface | 10 (R-V156P1-M003..M005 + L001..L007) | ✅ All closed in P0 |
| V1.57+ backlog DF-56 independent slice | 7 (R-V156P2-M001..M003 + L001..L004) | ✅ All closed in P2 |
| V1.57+ backlog DF-56 dependent slice | 5 (R-V156P3-W001/W002 + S001/S002/S004) | ✅ All closed in P2 |
| V1.57+ backlog V1.57-new | 3 (R-V157P0-L001/L002 + R-V157P1-W001) | ✅ All closed (P0 T19/T20, P2 T13) |
| Process (sqlx hygiene + engine test fidelity + retry semantics) | 3 (R-V156-PROCESS-01 + R-V156P1-CACHE-01 + R-V156P2-CACHE-01) | ✅ All closed (P0 T18 paired; P2 T15) |
| **Total V1.58 absorption** | **34 of 35** | **32 V1.57+ + 2 V1.58+ + DF-44** |
| V1.52-era WL-A polish residuals | 14 | ⏸ Deferred to V1.59+ WL-A sweep (out of V1.58 scope per compass §6) |

**Total open at V1.58 ship**: 14 (V1.52-era WL-A residuals only)

---

## Cross-Plan Concerns

- **Dual `LazyLock<reqwest::Client>` (P0 + P1)**: P0 SHARED_CDN_CLIENT (registry) and P1 HTTP_CLIENT (reference_refresh) are separate connection pools with different configs. Documented as M-1 in P0 qc1; non-blocking. Recommend P-last consolidation.
- **`.sqlx/` cache hygiene**: Protocol documented in `daemon-runtime.md` + regression guard `sqlx_cache_intact.rs` added in P0. CI gate should run `SQLX_OFFLINE=true cargo check --workspace --tests`.
- **`host-call` smoke `#[ignore]` retention**: P2 T13 added 4 hermetic tests; 3 retained `#[ignore]` due to DaemonClient trait extraction boundary (P1 boundary). Documented.

---

## Pre-P-last Checklist

- [x] All 4 implement plans merged to `iteration/v1.58` (HEAD `5df60210`)
- [x] All 12 QC reports finalized (verdict: Approve)
- [x] All targeted re-reviews appended to qc2/qc3 reports
- [x] All V1.58-carry-forward residuals closed (33 of 35 + DF-44)
- [x] `cargo +nightly fmt --all -- --check` clean
- [x] `SQLX_OFFLINE=true cargo check --workspace --tests` clean
- [x] `cargo clippy --all -- -D warnings` clean
- [x] No platform-publish / MCP / Stripe / DF-12 / DF-13 / DF-16 / DF-49 / DF-55 / DF-59 opened

**P-mid Done.** Ready for P-last (`2026-06-22-v1.58-hygiene-and-closeout`).