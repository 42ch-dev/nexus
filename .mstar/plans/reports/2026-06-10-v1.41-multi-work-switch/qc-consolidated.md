---
report_kind: qc-consolidated
plan_id: 2026-06-10-v1.41-multi-work-switch
verdict: Approve (after fix-wave re-review)
generated_at: 2026-06-10T21:50:00+08:00
initial_review_range: "merge-base: 55689706 → tip: f4b39d42"
fix_wave_tip: 9b6627dd
final_review_range: "merge-base: 55689706 → tip: 9b6627dd"
working_branch_verified: iteration/v1.41
review_cwd_verified: /Users/bibi/workspace/organizations/42ch/nexus
reviewers:
  - "@qc-specialist (1, architecture-coherence-maintainability) — initial Request Changes → fix-wave re-review Approve"
  - "@qc-specialist-2 (2, security-correctness) — initial Approve (no re-review)"
  - "@qc-specialist-3 (3, performance-reliability) — initial Request Changes → fix-wave re-review Approve"
---

# QC Consolidated Gate — V1.41 P0 (DF-60 multi-work lifecycle)

## Verdict (final, after fix-wave re-review)
**Approve** — 3 Critical + 4 Warning blockers addressed in fix wave (5 required fixes dispatched to `@fullstack-dev` on `feature/v1.41-multi-work-switch`; 1 optional fix deferred with residual). Targeted re-review by `qc-specialist` + `qc-specialist-3` both returned Approve.

## Roll-up (initial review)

| Reviewer | Verdict (initial) | Critical | Warning | Suggestion |
|----------|-------------------|----------|---------|------------|
| @qc-specialist (1) | Request Changes | 3 | 3 | 4 |
| @qc-specialist-2 (2) | Approve | 0 | 0 | 6 |
| @qc-specialist-3 (3) | Request Changes | 0 | 4 | 4 |
| **Consolidated (initial)** | **Request Changes** | **3** | **7** | **14** |

## Roll-up (after fix-wave re-review)

| Reviewer | Verdict (re-review) | Disposition |
|----------|---------------------|-------------|
| @qc-specialist (1) | **Approve** | F-001/F-002/F-003 resolved; F-004 defer with R-V141P0-01; F-005 resolved via spec amendment §3.2 |
| @qc-specialist-3 (3) | **Approve** | W1/W2/W3/W4 all resolved; no new findings |
| **Consolidated (final)** | **Approve** | All blockers closed; 12 residuals registered; ready for QA verification |

## Blocking findings (must fix or defer-with-tracking this round)

### From qc1 (architecture)
- **F-001 (Critical)**: Missing daemon routes for `POST /v1/local/works/pool` and `POST /v1/local/works/{work_id}/completion-lock/release`. CLI `creator works use` and `creator works completion-lock release` will 404.
- **F-002 (Critical)**: `--from-work` and `--set-default` flags in `creator run start` are silently dropped at the daemon boundary (no field on `CreateWorkRequest`, no handler logic). Lineage + pool insert never happen.
- **F-003 (Critical)**: `.completion-lock.json` is never written to disk. `write_completion_lock_for_work()` exists with **zero callers** in the daemon. Spec §3 file-level lock is non-functional.
- **F-005 (Warning)**: Dual SSOT ambiguity — DB `completion_locked_at` vs file `.completion-lock.json` — must be reconciled. Spec update + atomic release.
- **F-004 (Warning)**: `runtime_lock_holder` no TTL/stale recovery — crashed CLI leaves a permanent lock.

### From qc3 (performance/reliability)
- **W1 (Warning)**: `mark_work_completed` DB+file non-atomic. Compounded by F-003 (no caller), so this manifests as a real reliability gap, not a theoretical one.
- **W2 (Warning)**: `completion_lock.json` lacks `schema_version` field. Trivial preventive fix.
- **W3 (Warning)**: `creator works use` CLI 404 — duplicate of F-001. Same root cause.
- **W4 (Warning)**: `mark_work_completed` logs at `debug!` instead of `info!` — observability gap on a major lifecycle event. Trivial fix.

### From qc2 (security/correctness)
- **No Critical / Warning** — Approve from this perspective. The 6 Suggestions are forward-looking and tracked in residuals below.

## Fix wave (P0 closeout) — owned by @fullstack-dev

**Required fixes (5 work items)**

1. **Daemon routes + handlers** (resolves F-001, F-003, W3)
   - `POST /v1/local/works/pool` (action=`set_pool_active`): demote prior active → queued, promote target → active, in a transaction with rollback.
   - `POST /v1/local/works/{work_id}/completion-lock/release`: call `completion_lock::release_completion_lock()` + clear `works.completion_locked_at` atomically.
   - Wire `write_completion_lock_for_work()` into the supervisor's `WorkComplete` terminal transition (or the `get_work` auto-promote path) so the file is actually written.
   - Add a hermetic integration test that drives `creator works use` against the daemon and asserts pool state + 200 response.

2. **`CreateWorkRequest` extension** (resolves F-002)
   - Add `lineage_from_work_id: Option<String>` and `set_pool_active: Option<bool>` fields.
   - In `create_work` handler: persist `lineage_from_work_id` in the new `WorkRecord` row; on `set_pool_active=true`, upsert a `novel_pool_entries` row with `status='active'` (demote prior active).
   - Add a hermetic test that POSTs `--from-work` and asserts DB row + pool row.

3. **Lockfile schema_version** (resolves W2)
   - Add `schema_version: u32` field to `CompletionLock` (default 1). Tolerate missing version on read with a one-time upgrade path.

4. **Lockfile/DB atomic write** (resolves W1 + F-005)
   - In `release_completion_lock`: clear DB `completion_locked_at` and delete the file in a single operation (best-effort with warning if one fails after the other; document split responsibility in the function doc).
   - Spec amendment: clarify DB column is SSOT; file is the on-disk artifact for cross-tool observation.

5. **`tracing::info!` on completion** (resolves W4)
   - One-line upgrade from `debug!` to `info!` in `mark_work_completed`, including `work_id`, `creator_id`, `completion_locked_at`, and `work_ref`.

**Optional fix** (can defer to V1.41 P-last or V1.42)

6. **`runtime_lock_holder` TTL/stale recovery** (resolves F-004). Cheap: 30-min idle TTL on the daemon side, with `tracing::warn!` + lock auto-clear + audit log. Defer if time-constrained; register as residual.

## Targeted re-review plan

After fix wave, dispatch **targeted re-review** to **both QC1 and QC3** (the two Request-Changes reviewers) in **one dispatch turn** (N=2 invokes):

- **QC1 re-review** (N=1, qc-specialist): confirm F-001, F-002, F-003 are addressed; reassess F-004, F-005 in light of fix wave.
- **QC3 re-review** (N=1, qc-specialist-3): confirm W1, W2, W3, W4 are addressed.
- **QC2** does **not** re-review (was Approve, no findings).
- Reviewer Assignment must contain `QC re-review: targeted — reviewers: qc-specialist, qc-specialist-3`.
- Each reviewer updates `qc1.md` and `qc3.md` **in place** (add `## Revalidation` section, update verdict) — do not create `qc1-rev2.md` etc.
- PM consolidates to `qc-consolidated.md` (this file) after the re-review.

## Residual register (open after this round, written to `.mstar/status.json` by PM)

Per `mstar-plan-artifacts/references/status-and-residuals.md`「Residual findings: severity (SSOT, machine field)」:

| ID | Severity | Source | Scope | Decision | Owner | Target |
|----|----------|--------|-------|----------|-------|--------|
| R-V141P0-01 | high | qc1 F-004 | `works.runtime_lock_holder` no TTL/stale recovery | defer | @fullstack-dev | V1.41 P-last or V1.42 |
| R-V141P0-02 | low | qc1 F-007 | `handle_status` ~200 lines duplicated from removed `run.rs` | accept | @fullstack-dev | V1.42 refactor |
| R-V141P0-03 | low | qc1 F-009 | `WorkPatch` 30 fields; consider builder pattern | defer | @fullstack-dev | V1.42 |
| R-V141P0-04 | low | qc1 F-010 | No CLI→daemon integration test for `creator works use` | defer | @fullstack-dev | after this fix wave |
| R-V141P0-05 | low | qc2 S2 | `--from-work` + `--reopen` + `--extend-chapters` combined path not implemented | defer | @fullstack-dev | V1.42 UX |
| R-V141P0-06 | low | qc2 S3 | `works status` shows DB but no on-disk missing-file hint for completion-lock | defer | @fullstack-dev | V1.42 UX |
| R-V141P0-07 | low | qc2 S4 | DB + on-disk completion-lock reconciliation helper missing | accept-with-fix | @fullstack-dev | in this fix wave |
| R-V141P0-08 | low | qc2 S5 | `WorkRef::new(validated)` newtype for path-traversal centralization | defer | @fullstack-dev | V1.42 |
| R-V141P0-09 | low | qc3 S1 | Partial index on `novel_pool_entries` to be verified via EXPLAIN when DF-61 ships | defer | @fullstack-dev | V1.41 P1 |
| R-V141P0-10 | low | qc3 S2 | `repeated_sweeps_remain_stable` pre-existing flakiness (timing-sensitive) | accept | @fullstack-dev | backlog |
| R-V141P0-11 | nit | qc3 S3 | `WorkSummary` list view omits `completion_locked_at` (UX choice) | accept | @fullstack-dev | V1.42 UX |
| R-V141P0-12 | low | qc3 S4 / completion-report §6 | `.sqlx/` offline cache not fully refreshed for 5 new columns + `novel_pool_entries` | defer | @fullstack-dev | V1.41 P-last (when sqlx-cli available) |

(F-005 is being fixed in the fix wave, not registered as a residual; F-007 is a Suggestion with explicit `accept` decision — same as R-V141P0-02.)

## Summary

| Severity | Count (initial) | Count (after re-review) |
|----------|------------------|------------------------|
| 🔴 Critical | 3 | 0 (all resolved) |
| 🟡 Warning | 7 | 0 (5 resolved, 1 defer + 1 accept-with-fix closed) |
| 🟢 Suggestion | 14 | 14 (forward-looking, tracked in residuals) |

**Initial verdict**: Request Changes
**Final verdict (after fix-wave re-review)**: **Approve**
