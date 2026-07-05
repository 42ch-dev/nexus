---
report_kind: qc-consolidated
plan_id: v1.65
iteration: V1.65
gate: Approve
generated_at: 2026-06-25T18:00:00Z
consolidated_by: project-manager
---

# V1.65 QC Consolidated Gate Decision

## Scope (character-identical across all 3 reviewers + re-review)
- **plan_id**: `v1.65`
- **Review range / Diff basis**: `merge-base 644acbc56856d03e8e3aaf2139f73dccfcf6ed54 (origin/main) ... HEAD 4f0793cc` (= `git diff origin/main...HEAD`; 112+ files, +9k/-0.4k)
- **Working branch**: `iteration/v1.65`
- **Iteration scope**: P0 (Local API chapter-content surface + work_profile + preset CRUD + codegen), P-sec (vitest 2→3 / vite 5→6 / wiremock 0.6 → rand 0.7.3 eliminated — 5 dependabot advisories resolved), P1 (apps/web test baseline, R-V164-QC1-S1-P1 closed), P2 (Outline & Structure Authoring UI), fix-wave-1 (4 QC blocking findings).

## Tri-review verdicts

| Reviewer | Focus | Initial verdict | Re-review (fix-wave-1) | Final |
|----------|-------|-----------------|------------------------|-------|
| qc1 (`qc-specialist`) | architecture / maintainability | Request Changes (W-1 high, W-2 medium) | **Approve** (W-1 resolved DB-first reorder + failpoint test; W-2 deferral accepted) | **Approve** |
| qc2 (`qc-specialist-2`) | security / correctness | **Approve** (0 Critical, 0 Warning; 4 Suggestions) | (not re-reviewed — no blocking findings) | **Approve** |
| qc3 (`qc-specialist-3`) | performance / reliability | Request Changes (4 Warnings) | **Approve** (all 4 resolved: keyset pagination, 10MiB body cap, outline PUT DB-first, keydown cleanup) | **Approve** |

## Consolidated gate decision: **Approve**

3/3 reviewers Approve after fix-wave-1. Zero unresolved Critical, zero unresolved Warning. Gate passes for merge-to-main (via PR per repo merge discipline).

## fix-wave-1 resolved (4 blocking findings)
| Finding | Severity | Fix | Commit |
|---------|----------|-----|--------|
| Outline PUT FS/DB atomicity (qc1 W-1 + qc3 W-3) | high | DB-first ordering (`update_outline_path` before file write) + failpoint regression test `put_outline_db_failure_does_not_write_file` | `1407b16a` |
| OFFSET → keyset pagination (qc3 W-1) | medium | keyset cursor on `(volume, chapter)` with `v2:` opaque cursor; PK-covered predicate; test `list_chapters_keyset_pagination` | `9c9945a7` |
| Unbounded body read (qc3 W-2) | medium | 10 MiB metadata cap in `read_guarded_file` → `CHAPTER_BODY_TOO_LARGE` (HTTP 413); test `get_chapter_body_rejects_oversized_file` | `15d5f145` |
| Leaking keydown listener (qc3 W-4) | medium | named handler + `useEffect` cleanup in `chapter-page.tsx`; escape-to-close + balance test | `6e14fb13` |

## Residuals (registered in `status.json` → `residual_findings["v1.65"]`)
| ID | Title | Severity | State | Target |
|----|-------|----------|-------|--------|
| `R-V165-QC1-W1` | Outline PUT FS/DB atomicity | high | **resolved** (fix-wave-1 `1407b16a`) | closed |
| `R-V165-QC1-W2` | Missing HTTP-level integration tests for chapter endpoints | medium | open | follow-up test-baseline slice / V1.66 |
| `R-V165-QC-SUGG-DEFENSE` | Write-path guard parity + `RuntimeLockGuard` dedup + post-rename fsync (qc1 S-1/S-4, qc2 S-001, qc3 S-3) | low | open | V1.66+ |
| `R-V165-QC3-VIRT` | Chapter table virtualization for large chapter counts (qc3 S-2) | low | open | V1.66 (when large-count Works appear) |
| `R-V165-QC-SUGG-DX` | DX/UX polish: `can_edit_outline` probe, title 400 DX, route nesting, enum_conversions discoverability, TipTap inline-deps pinning, save-error/protected-edit web coverage (qc1 S-2/S-3/S-5/S-6, qc3 S-1/S-4) | low | open | V1.66+ |

**Approve with residuals**: zero open Critical/Warning; the medium `R-V165-QC1-W2` (test-depth, not correctness) and low follow-ups are durably tracked. PR-ready pending QA.

## Validation evidence (merged iteration/v1.65 @ 4f0793cc)
- `cargo test -p nexus-daemon-runtime --lib` → 308 passed
- `cargo clippy --all -- -D warnings` → clean
- `cargo +nightly fmt --all --check` → clean
- `pnpm --filter nexus-contracts build` → clean (gitignored dist rebuild)
- `pnpm --filter web test` → 81 passed (stable x2 runs)
- `pnpm --filter web typecheck` → clean
- `pnpm --filter web build` → ✓ built
- schema-drift detection → 4/4 (all 11 chapter schemas bidirectional-aligned)

## Next
QA verification (runtime/behavior change — new Local API endpoints + UI), then P-last (spec promotion + Profile B compaction + PR `iteration/v1.65` → `main`).
