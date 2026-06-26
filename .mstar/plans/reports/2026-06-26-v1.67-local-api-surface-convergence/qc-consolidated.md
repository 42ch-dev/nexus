# QC Consolidated Decision — V1.67 P0 Local API Surface Convergence

**plan_id**: 2026-06-26-v1.67-local-api-surface-convergence
**Consolidated by**: @project-manager (2026-06-26)
**Working branch**: iteration/v1.67
**Review range**: P0 feat `ea94b028`, diff basis vs origin/main
**Consolidated verdict**: **Request Changes**

## Seat verdicts
| Seat | Verdict | Blocking findings |
|---|---|---|
| qc1 (arch/maintainability) | Request Changes | W-1 works.rs hand-written DTOs; W-2 sort test gap; W-3 503/422 envelope e2e test |
| qc2 (security/correctness) | Request Changes | W-1 F-F1 unknown-key emits `bad_request` not `<resource>_sort_invalid` |
| qc3 (perf/reliability) | Request Changes | W-1 works/schedules in-memory sort before pagination; W-2 sort_invalid hidden as bad_request; W-3 sort coverage |

## CASING conformance — RESOLVED (no fix)
qc2 verified the uppercase "internal classification codes" (`DATABASE_ERROR`, `INTERNAL_ERROR`, …) are **Internal-only** — they never appear in the wire `ApiErrorDetail.code` (which is always `self.error_code()` → lowercase). Compass §5 #2 (global lowercase) is **conformant** for the wire surface. No fix needed; the partial-casing PM flag is closed.

## Fix wave (blocking — must fix before re-review)
1. **works.rs DTO dedup**: migrate `ListWorksQuery`/`ListWorksResponse`/`WorkSummary` to the generated DTOs (siblings already migrated) — single-source-of-truth (AGENTS.md).
2. **F-F1 `<resource>_sort_invalid` code**: unknown sort key must emit the resource-specific code on the wire (works→`work_sort_invalid`, etc.), not generic `bad_request`. (qc2 W-1 + qc3 W-2)
3. **F-F1 sort push-down for works/schedules**: works + schedules have DB `ORDER BY` support — parameterize the sort into the SQL (not in-memory materialize-before-paginate). sessions/capabilities may stay in-memory (small bounded sets). (qc3 W-1)
4. **Sort test coverage** (AC7): `parse_sort_terms` unit tests + `?sort=` integration tests on the 4 endpoints + invalid-key → `<resource>_sort_invalid` assertion. (qc1 W-2 + qc3 W-3)

## Deferred as residuals (register in status.json residual_findings[<plan>])
- `R-V167P0-QC1-S-COMPARE`: triplicate `compare_*` closures — extract shared comparator (qc1 S-1, low).
- `R-V167P0-QC1-S-DOCS`: ACP SDK mirror-type naming doc-comment (qc1 S-2, low).
- `R-V167P0-QC1-S-CLI`: `--sort` flag in `nexus42 schedule list` CLI (qc1 S-3, low).
- `R-V167P0-QC1-S-AGENTS`: error-envelope single-source rule in `nexus-daemon-runtime/AGENTS.md` (qc1 S-4, low).
- `R-V167P0-QC1-ENVELOPE-E2E`: 503/422 `IntoResponse`→envelope e2e test (qc1 W-3, low — non-blocking; fix if cheap in the wave, else defer).

## Re-review after fix wave
Targeted re-review: **all 3 seats** (each raised blocking findings) → update `qc1.md`/`qc2.md`/`qc3.md` with `## Revalidation`.
