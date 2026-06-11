---
report_kind: qc-consolidated
consolidated_by: project-manager
plan_id: "2026-06-11-v1.42-multi-volume"
verdict: "Request Changes"
generated_at: "2026-06-11"
---

# QC Consolidated Decision — V1.42 P1 Multi-Volume

## Scope
- plan_id: `2026-06-11-v1.42-multi-volume`
- Review range / Diff basis: `merge-base: c249c902` (P0 QA-merge) + `tip: HEAD` of `iteration/v1.42` (`929fe5bd` at consolidation time). Covers 9 commits: `9fefdfbc`, `398d0ba2`, `b63543e1`, `0bbf1581`, `1a6fd97c`, `856f8cd3`, `0d6f5287`, `c32321d1` (PM merge), `929fe5bd` (status).
- Working branch: `iteration/v1.42` (integrated HEAD `929fe5bd`)
- Review cwd: `/Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.42-p1-qc` (detached HEAD, read-only analysis)
- All three reviewers' individual scope lines copy-pasted the same `plan_id` and `Review range / Diff basis` — alignment verified character-level.

## Reviewer Matrix

| Reviewer | Index | Focus | Verdict | Commit | Critical | Warning | Suggestion |
|----------|-------|-------|---------|--------|----------|---------|------------|
| @qc-specialist | 1 | Architecture coherence and maintainability risk | **Request Changes** | `5417a7b6` | 1 (F-001) | 3 (F-002, F-003, F-004) | (see report) |
| @qc-specialist-2 | 2 | Security and correctness risk | **Approve** | `c626f150` | 0 | 2 (carry-forwards from base) | 5 |
| @qc-specialist-3 | 3 | Performance and reliability risk | **Request Changes** | `ce6060b4` | 0 | 2 (W-01, W-02) | 4 |
| **Totals** | — | — | **2 Request Changes, 1 Approve** | — | **1** | **5** | **9+** |

Per `mstar-review-qc` rule "存在未解决的 `Critical` 或 `Warning` → `Request Changes`": qc1's F-001 (Critical: dead code that breaks cross-volume auto-chain in production) is unresolved. qc3's W-01 + W-02 are unresolved. Per the strict rule, the consolidated verdict must be **Request Changes**.

## Blocking Findings (must fix before Approve)

| F# | Source | Severity | Title |
|----|--------|----------|-------|
| F-001 | qc1 | **critical** | `evaluate_after_persist_volume_aware` is dead code — never wired into supervisor or boot auto-chain paths. Multi-volume auto-chain will NOT cross volume boundaries in production. Plan Goal 4 / AC2 not fulfilled at the integration level. Hermetic test exercises the function in isolation only. |
| W-01 | qc3 | medium (Warning) | Migration DDL lacks `DROP TABLE IF EXISTS` guard for idempotent retry. Re-running migration on already-migrated DB will fail or lose data. |
| W-02 | qc3 | medium (Warning) | `next_chapter_volume_aware` query may filesort due to missing volume in index. Add `(work_id, status, volume, chapter)` covering index. |

## Non-Blocking (track as residuals)

| F# | Source | Severity | Title |
|----|--------|----------|-------|
| F-002 | qc1 | low (Warning) | `is_work_completed` uses flat `current_chapter` comparison — correct under current invariants but architecturally fragile |
| F-003 | qc1 | low (Warning) | `reconcile_from_filesystem` hardcodes `volume=1` — multi-volume chapter files silently skipped |
| F-004 | qc1 | low (Warning) | Supervisor `NextChapter` match arm ignores `next_volume` — volume context lost before schedule enqueue |
| 4 Suggestions | qc3 | nit | (pagination, E2E test gap, reconcile volume default, path format) |
| 5 Suggestions | qc2 | nit | (positive observations) |

## Process Gap (Documented, Risk-Accepted — Carried from P0)

- **R-V142P0-PROC** (severity: high, decision: risk-accepted, owner: @project-manager): the Cursor (`Auto.Wood`) agent did P0 closeout + migration with process violations. Same pattern may apply to P1 — if Cursor did any unauthorized direct commits to `iteration/v1.42` during P1 implement/QC window, the user has accepted this and PM continues to consolidate on the integration branch as-is.

## Consolidated Decision

**Decision**: **Request Changes**

**Blocking Items**: F-001 (critical, qc1) + W-01 + W-02 (warning, qc3)

**Residual Findings** (new for P1; open list, severity enum canonical):
- R-V142P1-QC1-F-001 (critical) — dead `evaluate_after_persist_volume_aware`; needs wiring into supervisor/boot
- R-V142P1-QC3-W-01 (medium, defer) — migration idempotency guard
- R-V142P1-QC3-W-02 (medium, defer) — index coverage for `next_chapter_volume_aware`
- R-V142P1-QC1-F-002 (low, defer) — `is_work_completed` flat current_chapter
- R-V142P1-QC1-F-003 (low, defer) — `reconcile_from_filesystem` hardcoded volume=1
- R-V142P1-QC1-F-004 (low, defer) — supervisor NextChapter match arm ignores next_volume
- R-V142P1-QC3-S-* (nit) — qc3 suggestions (4)
- R-V142P1-QC2-S-* (nit) — qc2 suggestions (5)

**Assigned Fix Owners**:
- R-V142P1-QC1-F-001: @fullstack-dev — must fix in fix wave (blocking)
- R-V142P1-QC3-W-01, W-02: @fullstack-dev — must fix in fix wave (blocking)
- R-V142P1-QC1-F-002/003/004, *-S-*: @fullstack-dev — defer to V1.42 P-last or future (non-blocking)

**Next Step**: **Fix wave dispatch to @fullstack-dev** (N=1) to address F-001, W-01, W-02. After fix: **targeted QC re-review** of qc-specialist (qc1) + qc-specialist-3 (qc3) only (N=2, same dispatch turn); qc-specialist-2's Approve stands. Then **QA verification** (N=1). Then PM closure.
