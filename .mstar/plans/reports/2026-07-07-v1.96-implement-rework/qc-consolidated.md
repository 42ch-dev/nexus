---
report_kind: qc-consolidated
plan_id: 2026-07-07-v1.96-implement-rework
reviewers: [qc1, qc2, qc3]
verdict: Approve
generated_at: 2026-07-07T20:00:00+0000
fix_waves: 1
---

# V1.96 Plan QC Consolidated

## Scope (shared across qc1/qc2/qc3)

- **plan_id**: `2026-07-07-v1.96-implement-rework`
- **Working branch**: `feature/v1.96-implement-rework`
- **Review range / Diff basis**: `merge-base: f9b73d27` + `tip: HEAD (acbbbe1a after fix-wave 1)`
- **Branch review-package**: `.mstar/sdd/2026-07-07-v1.96-implement-rework/branch-review-package.md`
- **Compass**: `.mstar/iterations/v1.96-setup-wizard-rework-and-daemon-diagnostic-delivery-compass-v1.md`

## Tri-review verdicts

| Seat | Focus | Initial verdict | After fix-wave 1 | Report |
|------|-------|-----------------|------------------|--------|
| qc1 | Architecture / maintainability | Approve (0C/0W/4S) | — (no fix needed) | `qc1.md` |
| qc2 | Security / correctness | Approve (0C/1W-accepted/2S) | — (no fix needed) | `qc2.md` |
| qc3 | Performance / reliability | **Request Changes** (1C/2W/2S) | **Approve** (C-1 fixed) | `qc3.md` + `## Revalidation` |

**Consolidated verdict: Approve** — all 3 seats approve after fix-wave 1.

## Fix-wave 1

- **qc3 C-1 (Critical)**: `setup-wizard-page.tsx:54` `finish()` + `setup-gate.tsx:50` used the old `err instanceof Error ? err.message : <fallback>` pattern → real Tauri invoke errors silently dropped. Fixed in `acbbbe1a`: swapped to `errorMessage(err) || <fallback>` in both files. qc3 revalidation confirmed ✅.

## Residuals registered (status.json `residual_findings[2026-07-07-v1.96-implement-rework]`)

| ID | Title | Severity | Decision | Owner | Target | Source |
|----|-------|----------|----------|-------|--------|--------|
| R-V196QC3-W001 | Stderr drain task race window on fast-crash (intermediate `stderr_tail` read before drain finishes processing `Terminated`) | low | defer | @fullstack-dev | V1.97 | qc3.md §W-1 |
| R-V196QC1-S002 | `rounded-control` inlined in setup-step-welcome.tsx where DESIGN.md supplies `input-row-rounded` token | low | defer | @frontend-dev | V1.97 | qc1.md §S-002 |
| R-V196QC1-S004 | `format_error_detail` composes free-form String rather than structured envelope (future "copy error details" support button) | low | defer | @fullstack-dev | V1.97+ | qc1.md §S-004 |
| R-V196QC3-S001 | Multi-line stderr test asserts textContent but not `getComputedStyle().whiteSpace === 'pre-wrap'` (would not catch className regression) | nit | defer | @frontend-dev | V1.97 | qc3.md §S-1 (from T4 revalidation) |
| R-V196QC1-S001 | `path-context-menu.tsx:74-79` has near-duplicate inline `errorMessage()` (slight variant: non-null guard, `'Action failed.'` fallback) | low | defer | @frontend-dev | V1.97 | T1 review §S2 + qc1 §S-001 |

## Accepted (no residual — design decisions)

- **qc2 W-1 (accepted)**: daemon stderr surfaced verbatim in `DaemonStatus.detail` (paths/config/migration text) — acceptable within local-first trust boundary (user-owned process, 2 KiB newline-capped tail, only on Error, generic fallback preserved, no network exposure).
- **qc3 W-2**: `errorMessage()` checks `'message' in err` before `instanceof Error` — counterintuitive but correct (Tauri invoke errors are plain objects; the ordering is the fix for the `[object Object]` bug).

## Acceptance criteria coverage (compass §5 → implementation)

- ✅ Desktop app opens without "The string did not match the expected pattern." (V1.95 ClientProvider fix preserved — full suite green)
- ✅ Step 1 Browse no longer shows `[object Object]` (T1 + T2 + fix-wave 1 swept all sites)
- ✅ Step 1 layout: Browse inline with location (T5), errors via toast (T2), Continue wide bottom (T6)
- ✅ Step 2 wizard never hangs in "Starting daemon…" (T4 mount-probe + starting branch + 25s timeout)
- ✅ Step 2 surfaces real daemon error (T3 stderr capture + T4 detail render with `whitespace-pre-wrap`)
- ✅ Layout centered + integrated (T7); circle/label aligned (T7 StepIndicator fix)
- ✅ DESIGN.md "Setup Wizard Surface" at Level 3 Production (Phase 1 D1); components consume tokens (T5 wired)
- ⏳ Author wipe-and-relaunch smoke (compass §5) — **deferred to QA / manual** (implementer lacks desktop env)

## Verification snapshot

- `pnpm --filter web run test` → 546 passed (75 files)
- `pnpm --filter web run typecheck` → clean
- `pnpm --filter web run build` → clean
- `cd apps/desktop/src-tauri && cargo test` → 36 passed
- `cargo clippy --all -- -D warnings` → clean
- `cargo +nightly-2026-06-26 fmt --all --check` → clean

## PM decision

**Approve for QA**. 5 low/nit residuals registered for V1.97 polish; no blocking findings remain. Closes residuals `R-V195-ARCH-STRERR-GAP` (medium) and `R-V195QC3-S001` (low). Proceed to QA verification, then plan Done + merge to `iteration/v1.96`.
