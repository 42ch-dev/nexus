---
report_kind: qc-consolidated
plan_id: "2026-06-09-v1.39-findings-and-review-routing"
verdict: "Request Changes"
generated_at: "2026-06-09T04:30:00+08:00"
initial_wave: 3
reports:
  - qc1 (qc-specialist, architecture)
  - qc2 (qc-specialist-2, security & correctness) — Approve
  - qc3 (qc-specialist-3, performance & reliability) — Request Changes
---

# V1.39 P1 Findings + Review Routing — QC Consolidated (Initial Wave)

## Reviewer Metadata
- Plan ID: `2026-06-09-v1.39-findings-and-review-routing`
- Plan path: `.mstar/plans/2026-06-09-v1.39-findings-and-review-routing.md`
- Iteration compass: `.mstar/iterations/v1.39-novel-auto-chain-and-quality-loop-delivery-compass-v1.md`
- Integration branch: `iteration/v1.39` (HEAD `111c3611` after P0 + P0.5 + P5 closeouts)
- Topic branch: `feature/v1.39-findings-and-review-routing` @ `137fefaf`
- Review cwd: `.worktrees/v1.39-p1`
- Review range / Diff basis: `merge-base: 111c3611` + `tip: 137fefaf` → `git diff 111c3611...137fefaf` (5 commits, 14 files, +1337 +10 / -4)
- Initial wave: 3 reports
- Reviewer verdict breakdown: qc1 Approve (with 2 W + 2 S), qc2 Approve (0 W + 4 S), qc3 Request Changes (1 C + 4 W + 2 S)
- Consolidated gate: **Request Changes** (1 spec violation)

## Scope
- plan_id: `2026-06-09-v1.39-findings-and-review-routing`
- Review range / Diff basis: `merge-base: 111c3611` (iteration/v1.39 HEAD with P0 + P0.5 + P5 closed) + `tip: 137fefaf` (feature/v1.39-findings-and-review-routing HEAD); equivalent to `git diff 111c3611...137fefaf` (run in the Review cwd). 5 commits, 14 files, +1337 +10 / -4.
- Working branch (verified): `feature/v1.39-findings-and-review-routing`
- Review cwd (verified): `.worktrees/v1.39-p1`

## Per-Reviewer Summary

| Reviewer | Focus | Critical | Warning | Suggestion | Verdict |
|---|---|---|---|---|---|
| @qc-specialist (qc1) | Architecture | 0 | 2 | 2 | Approve (with caveats) |
| @qc-specialist-2 (qc2) | Security + correctness | 0 | 0 | 4 | Approve |
| @qc-specialist-3 (qc3) | Performance + reliability | 1 | 4 | 2 | Request Changes |
| **Consolidated** | — | **1** | **6** (3 distinct) | **8** | **Request Changes** |

## Acceptance Criteria Mapping

| AC | Status | Evidence |
|---|---|---|
| AC1: Finding CRUD via daemon API with creator isolation | ✅ | 7 findings tests including `findings_creator_isolation_cross_creator_404` |
| AC2: Review stage completion can create at least one finding row | ✅ | `from-review` endpoint + `findings_from_review_endpoint_auto_create` test |
| AC3: Status lists open findings with severity and routing hint | ✅ | `findings_routing_hints_all_executors` test + CLI section |
| AC4: Findings do not fork auto-chain driver | ✅ | No change to `enqueue_auto_chain_schedule`; auto_chain tests still 21 green |

## Findings (deduplicated)

### 🔴 Critical (machine: critical / high)

- **C-1** (qc3) — *high*: **Missing composite index on `(work_id, chapter, status)`** — `novel-writing/quality-loop.md` §2.1 explicitly requires `Indexes: (work_id, status), (work_id, chapter, status).` P1's migration `202606090002_findings.sql` added only `(work_id, status)` and `(creator_id, status)`. The spec-required `(work_id, chapter, status)` is missing. At scale, chapter-scoped finding lookups (the review-stage hook's hot path) will full-table scan. → **Fix**: add `CREATE INDEX IF NOT EXISTS findings_work_chapter_status ON findings(work_id, chapter, status);` to the same migration.

### 🟡 Warning (6 deduped, 3 distinct)

- **W-1** (qc1) — *medium*: No server-side enum validation on severity/status/target_executor. The DAO accepts any TEXT, so a typo from a non-CLI caller could store an invalid value. → Defer to V1.40 hardening or fix in this slice.
- **W-2** (qc1) — *low*: Duplicated finding ID generation across handler and DB layer. → Defer to V1.40.
- **W-3** (qc3) — *low*: CLI status HTTP timeout unbounded — slow daemon could freeze CLI. → Defer to V1.40.
- **W-4** (qc3) — *low*: List query may skip index for cross-creator patterns. → Defer to V1.40.
- **W-5** (qc3) — *low*: Runtime sqlx query not compile-time checked (for `count_open_findings_by_severity`). → Defer to V1.40.
- **W-6** (qc3) — *low*: from-review hook error not explicitly logged. → Defer to V1.40.

### 🟢 Suggestion (8 deduped; non-blocking)

- S-1 (qc1): `from-review` endpoint reuses generic `CreateFindingRequest` — could be more specialized.
- S-2 (qc1): Add explicit test for cross-creator 403 vs 404 (current is 404, which is more privacy-preserving).
- S-3 (qc2): T3 severity floor for blocker-level findings.
- S-4 (qc2): ANSI stripping on CLI title output.
- S-5 (qc3): Add `created_at` index.
- S-6 (qc3): Use enums for severity/status (currently TEXT in DB).

## Decisions

- **C-1** (high) → **fix wave** before merge.
- **W-1** (medium) → defer to V1.40; PM may opt to fix in same slice (cheap).
- **W-2..W-6** (low) → defer to V1.40.
- **S-1..S-6** → defer; not blocking.

## Required Fix Wave (before merge)

The implementer must address **C-1** in a focused fix wave on the same `feature/v1.39-findings-and-review-routing` branch. Optionally also **W-1** (cheap, server-side enum validation in DAO). The fix wave is single-dev (not tri-review); after the fix, PM dispatches **targeted re-review** by qc3 (C-1 owner) and qc1 (W-1 if addressed) — per `mstar-review-qc` "targeted re-review" rule.

After targeted re-review Approve, PM merges `feature/v1.39-findings-and-review-routing` → `iteration/v1.39` and flips plan status to `Done` in `status.json`.

## Source Trace
- qc1: `b0d6d1ad qc(report): QC #1 architecture/maintainability review for V1.39 P1 findings-and-review-routing` (feature/v1.39-findings-and-review-routing)
- qc2: `0e1618c5 qc(qc-specialist-2): V1.39 P1 findings-and-review-routing security+correctness review (qc2.md)` (feature/v1.39-findings-and-review-routing)
- qc3: `91481965 qc(v1.39-p1): QC Review #3 — performance and reliability risk assessment` (feature/v1.39-findings-and-review-routing)

## Summary
| Severity | Count |
|---|---|
| 🔴 Critical | 1 (C-1 spec violation) |
| 🟡 Warning (medium) | 1 (W-1, optional fix) |
| 🟡 Warning (low) | 5 |
| 🟢 Suggestion | 8 |

**Verdict**: **Request Changes** — C-1 is a spec violation (novel-writing/quality-loop.md §2.1); must be fixed before merge.

---

*PM consolidated 2026-06-09. Next dispatch: P1 fix wave to `@fullstack-dev` (single dev, scope-locked to C-1 + optional W-1).*
