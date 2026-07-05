# QC Consolidated Decision — V1.67 P-sec Desktop Shell Polish

**plan_id**: 2026-06-26-v1.67-desktop-shell-polish
**Consolidated by**: @project-manager (2026-06-26)
**Working branch**: iteration/v1.67
**Review range**: P-sec code `cf48a8f1` + `bc8d4bea`, diff basis vs origin/main
**Consolidated verdict**: **Request Changes**

## Seat verdicts
| Seat | Verdict | Blocking findings |
|---|---|---|
| qc1 (arch/maintainability) | Approve | (0 Critical, 0 Warning, 4 Suggestion) |
| qc2 (security/correctness) | Approve | (0 Critical, 0 Warning) — TOCTOU, sidecar unwrap, restart_count reset, event emit all verified correct |
| qc3 (perf/reliability) | **Request Changes** | W-1 no fallback re-sync for missed daemon-status event; W-2 restart_count reset also on crash restarts → may never hit MAX_RESTART_ATTEMPTS |

No wire/contract change (qc1 verified `check-wire-drift` clean, no schema touch). ✓

## Fix wave (blocking — must fix before re-review)
1. **restart_count reset scope (qc3 W-2)**: reset `restart_count = 0` **only on manual `startDaemon`**, NOT on automatic crash restart. Otherwise repeated crash cycles never exhaust `MAX_RESTART_ATTEMPTS` → unbounded restart-loop risk. (Reliability/correctness.)
2. **Event-driven status fallback re-sync (qc3 W-1)**: the V1.66 polling was removed; a missed Tauri event leaves the UI stale indefinitely. Add a low-frequency fallback re-sync (e.g., periodic health re-poll at a calm interval like 5–15s, OR re-sync on a trigger like window focus / status-bar mount). Keep the event path primary; the fallback only catches the missed-event case.

## Deferred as residuals (register in status.json residual_findings[<plan>])
- `R-V167PSEC-QC1-S-UNMOUNT`: subscription-cleanup race in `daemon-status-bar.tsx` if unmount during async setup (qc1 S-1, low).
- `R-V167PSEC-QC1-S-BLOCKINGLOCK`: `set_app_handle` `blocking_lock()` from sync context — fragile against Tauri timing (qc1 S-2, low).
- `R-V167PSEC-QC1-S-BACKOFFOBS`: backoff jitter choice not logged (qc1 S-3, low).
- `R-V167PSEC-QC1-S-CI-SET EUO`: desktop-build CI fallback block lacks `set -euo pipefail` (qc1 S-4, low).

## Re-review after fix wave
Targeted re-review: **qc3 only** (qc1/qc2 Approve) → update `qc3.md` with `## Revalidation`.
