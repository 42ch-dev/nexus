# P3 — Frontend Consolidation & UI Polish Sweep Spec

> Iteration: V1.111. Status: **PM draft — architect scopes residual list at
> §5.2**. Primary consumer: plan
> `2026-07-12-v1.111-frontend-consolidation-sweep.md`.

## Problem

A cluster of frontend residuals has accumulated across V1.106–V1.110, all
deferred with `lifecycle: open` and `decision: defer`/`accept`, targeting
`post-V1.106`/`post-V1.107`/`post-V1.108`/`post-V1.109`/`post-V1.110`. They are
drift risk (duplicate implementations, stale comments, prop contracts) and UX
polish opportunities. The user direction explicitly names 遗留问题 (leftovers),
UI bug, and UI/UX美观. This plan pays down that cluster in one consolidation
pass so the next iteration inherits a clean frontend.

## Candidate residual cluster — FINAL (architect-scoped 2026-07-12)

> Verified against `.mstar/status.json` + current source. Toast + guardrails
> findings corrected against present-day code. See plan
> `## Architecture locks` for the full disposition table.

| Residual | Disposition | Rationale (evidence) |
|----------|-------------|----------------------|
| `R-V1106P0-001` (toast dup) | **Dismiss with reason** | `use-toast.tsx` is a 7-line re-export barrel from `@42ch/nexus-ui` — sole impl; 40+ call-site migration = churn, deferred. |
| `R-V1106P0-002` (lucide-react dep doc) | **Close by fix** | Add lucide-react to `@42ch/nexus-ui` AGENTS.md runtime-dep record. Doc-only. |
| `R-V1109-P0-QC1-W001` (Studio chrome mirror) | **Dismiss with reason** | Static-mirror is the LOCKED direction (RF boundary); P2 extends it. Not a gap. |
| `R-V1107QC1-W002` (ConnectDaemonFormChrome props) | **Close by fix (document)** | Document `state` vs `matrixState` + deferred props in chrome docstring/AGENTS. |
| `R-V1107QC1-ARCH` (`shell-header-chrome.tsx`) | **Dismiss with reason** | No FB criterion; no second consumer. Intentionally deferred. |
| `R-V1106P2-001` (stale `/settings/connection` comments) | **Close by fix** | Sweep stale comments in `connect-daemon-form.tsx` + test. |
| `R-V1106P2-002` (`scrollIntoView #setup`) | **Dismiss (defer)** | Enhancement not bug; hash sets correctly. → roadmap-next. |
| `R-V1100P2QC3-S001` (guardrails stale comment) | **Verify + close** | Script evolved; verify `:75-76`, fix or dismiss. |
| UX polish (architect-scoped) | **One task** | Spacing/density/contrast audit on P0 palette + P1 sidebar vs DESIGN.md. |

**Plan cap = 9 residuals.** Overflow → roadmap-next.

## Open questions for architect — RESOLVED (2026-07-12)

1. **Final residual list** — **Resolved: 9 items** (table above). 5 close-by-fix
   / verify, 4 dismiss-with-reason. Capped; overflow → roadmap.
2. **Toast consolidation direction** — **Resolved: already consolidated.**
   Canonical = `packages/nexus-ui/src/components/toast.tsx`; App-local
   `use-toast.tsx` is a re-export barrel (V1.99 pattern), NOT a duplicate. **No
   40+ call-site migration** this iteration — deferred to a future hygiene pass
   if the barrel is removed.
3. **UX aesthetics scope** — **Resolved: ONE task** — spacing/density/contrast
   audit on the new P0 palette + P1 sidebar surfaces vs DESIGN.md token tables.
   NOT a whole-app sweep.
4. **Verification** — each close/dismiss verified against `status.json`
   `residual_findings[<plan-id>]`; archived per Profile B;
   `tech_debt_summary` refreshed via `tech-debt-rollup.sh`.

## User value

- **FB-CS-000** — Toast residual `R-V1106P0-001` closed/dismissed with reason:
  already consolidated (App-local `use-toast.tsx` is a re-export barrel; sole
  impl is `@42ch/nexus-ui`; 40+ call-site migration deferred).
- **FB-CS-001** — Stale comment/copy drift swept (closes `R-V1106P2-001`,
  `R-V1106P0-002`, `R-V1100P2QC3-S001`).
- **FB-CS-002** — ConnectDaemonFormChrome prop contract reconciled/documented
  (dispositions `R-V1107QC1-W002` + `R-V1107QC1-ARCH`).
- **FB-CS-003** — UX polish: spacing/density/contrast audit on P0 palette + P1
  sidebar vs DESIGN.md token tables (architect-scoped, one task).
- **FB-CS-004** — All closed residuals archived to
  `archived/residuals/<plan-id>.json`; `status.json` `residual_findings` updated.

## Non-goals

- Mass archival of V1.91–V1.105 tech-debt residuals (separate spec-hygiene pass).
- A full design-system / token redesign.
- Backend/wire changes — `wire_contracts_changed: false`.
- Toast API behavior changes (consolidation only, not a rewrite).

## DoD

All FB-CS-000..004 accepted; every residual in the architect-scoped list is
either closed-by-fix (verified) or dismissed-with-reason (documented); closed
residuals archived; `status.json` `residual_findings` updated and
`tech_debt_summary` refreshed; QC tri Approve; QA gate passed.
