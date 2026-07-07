---
report_kind: qc-consolidated
plan_id: 2026-07-07-v1.95-implement-fixes
verdict: Approve with residuals
generated_at: 2026-07-07
reviewers:
  - qc-specialist
  - qc-specialist-2
  - qc-specialist-3
---

# QC Consolidated — 2026-07-07-v1.95-implement-fixes

## Scope (alignment — identical across all three reviewers)

- plan_id: `2026-07-07-v1.95-implement-fixes`
- Review range / Diff basis: `7c61c033..fe7a2730` (main..HEAD)
- Working branch: `feature/v1.95-implement-fixes`
- Review cwd: `/Users/bibi/workspace/organizations/42ch/nexus`
- Branch review-package: `.mstar/sdd/2026-07-07-v1.95-implement-fixes/branch-review.diff`

## Verdicts

| Reviewer | Focus | Verdict | Critical | Warning | Suggestion |
|----------|-------|---------|----------|---------|------------|
| qc1 (`3d71b3bd`) | Architecture coherence & maintainability | **Approve** | 0 | 0 | 6 |
| qc2 (`01440cbd`) | Security & correctness | **Approve** | 0 | 0 | 1 |
| qc3 (`b0d338e4` → revalidated `48bb0d5a`) | Performance & reliability | **Approve** (after revalidation) | 0 | 3→1 resolved, 2 PM-accepted | 1 |

**Tri-identity verified**: three distinct agent IDs (`qc-specialist`, `qc-specialist-2`, `qc-specialist-3`); alignment fields text-identical across all three reports.

## Fix round (one)

QC3 raised 3 Warnings. PM triage:

| # | Finding | Disposition | Rationale |
|---|---------|-------------|-----------|
| W1 | `pick_directory` used `blocking_pick_folder()` in an `async fn` | **FIXED** (`fe7a2730`) | Legitimate blocking-in-async anti-pattern. Fixed: `pick_folder()` async API bridged via `tokio::sync::oneshot`. Revalidated by QC3 → Resolved. |
| W2 | `reset_local_database` no atomicity/rollback | **PM-accepted → residual `R-V195QC3-W002`** | Recovery-wipe semantics — daemon recreates fresh DBs on boot regardless; partial wipe is recoverable. True multi-file-delete atomicity across a directory tree is not a standard pattern. V1.96 hardening candidate. |
| W3 | `set_workspace_path` direct write (no temp-file) | **PM-accepted → residual `R-V195QC3-W003`** | Matches the **existing** `write_setup_completed_at` / `write_agent_profile_at` pattern (all use `std::fs::write`). Fixing only this writer would create inconsistency; codebase-wide temp-file-then-rename hardening is V1.96. |

Targeted re-review (N=1, QC3 only) confirmed W1 resolved; W2/W3 acknowledged as PM-accepted deferred residuals.

## Gate decision

**Approve with residuals.** No unresolved Critical. No unresolved Warning (W1 fixed; W2/W3 PM-accepted as tracked residuals per `mstar-review-qc` §Residual Findings 留档门禁 — `Approve with residuals` allowed when no open Critical and residuals are documented).

## Residuals registered (→ `status.json` `residual_findings["2026-07-07-v1.95-implement-fixes"]`)

| ID | Severity | Title | Decision | Owner | Target |
|----|----------|-------|----------|-------|--------|
| R-V195-ARCH-DUPLICATE-DEFAULTS | low | Triplicate `resolve_default_workspace_path`/`default_workspace_root` (apps/nexus42, nexus-daemon-runtime, desktop src-tauri) — consolidate into nexus-home-layout | defer | fullstack-dev | V1.96 |
| R-V195-ARCH-STRERR-GAP | medium | SidecarManager discards `_rx` from `command.spawn()` — daemon stderr (real crash reason) never captured; wizard shows generic error | defer | fullstack-dev | V1.96 |
| R-V195QC3-W002 | low | `reset_local_database` deletes in a loop without atomicity/rollback; mid-deletion failure leaves partial state | defer | fullstack-dev | V1.96 |
| R-V195QC3-W003 | low | `set_workspace_path` (and sibling config writers) write directly without temp-file-then-rename; crash mid-write could corrupt config.toml | defer | fullstack-dev | V1.96 |
| R-V195QC3-S001 | low | Setup daemon step has no timeout — if sidecar never emits a status event, wizard hangs indefinitely (only useEffect cleanup unsubscribes) | defer | frontend-dev | V1.96 |
| R-V195QC1-S002 | nit | No dedicated malformed-TOML unit test for `write_workspace_path_at` (mirrors existing sibling-writer tests) | defer | fullstack-dev | V1.96 |

## Advisory suggestions (NOT tracked as residuals — code-polish, opportunistic)

- QC1 S-001: doc-comment the unconditional `set_workspace_path` Rust writer (policy lives TS-side).
- QC1 S-003: promote the local `errorMessage` helper to `desktop-capabilities.ts` (DRY with `asDesktopError`).
- QC1 S-004: surface `setWorkspacePath` rejection as a visible toast (currently `console.error`).
- QC2 S-001: same as QC1 S-002 (covered by `R-V195QC1-S002`).

## Next

Proceed to QA gate (`qa-engineer`), then plan `Done`, then Phase 3 iteration-close.
