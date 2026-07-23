# Spec: Legacy residual burn-down

**Status:** product-reviewed, architect-locked, writing-hygiene done (2026-07-23)

## Product outcome

1. **Bugs that still hurt creators** in the fix-now set are closed with evidence (restart reliability, profile auto-select completeness, display-name XSS mitigation, worlds list totals, etc.).
2. **The residual ledger is trustworthy:** stale “missing feature” claims are corrected and archived; deferred nits carry current rationale instead of expired “V1.12x+ polish” labels; open counts match reality after rollup.

## Problem

~64 open residuals accumulated across V1.92–V1.132. Many were marked for polish in future iterations but never closed. Some describe issues that no longer exist (stale). A few represent actual bugs or incomplete features marked Done without full verification. Users keep hitting the real bugs; planning keeps tripping over false debt.

## Scope

**In scope:**
- PM triage of all open residuals against current codebase state
- Fix-now: actual bugs or incomplete features (~5-8 items) with user-observable or security/reliability impact
- Stale-close: issues that no longer exist (~5-10 items)
- Defer-with-rationale: nits/polish with updated notes (~40-50 items)
- Human-verification smokes remain open with explicit note (not fake-closed)
- `status.json` + `archived/residuals/` lifecycle updates
- Tech-debt summary refresh

**Out of scope:**
- Full 64-residual burn-down (only high-impact + stale ledger repair)
- New features or visual redesign
- Human-verification smokes execution (Dock, Overlay) — keep open
- P0 brace-param sweep (separate plan)
- Compass non-goals (engine auto-start, DF-70/71, Timeline/Fork/Computable, platform/cloud)

## Fix-now candidates

See plan `2026-07-23-v1.133-p1-legacy-residual-burndown.md` § "Fix-now candidates" for the preliminary list (includes user-observable failure column). PM finalizes at plan lock after verifying each against current code.

## Acceptance criteria

| ID | User / product check | Ledger / engineering check |
|----|----------------------|----------------------------|
| **AC-5** | — | All open residuals classified (fix-now / stale-close / defer / human-verify) |
| **AC-6** | Fix-now failure modes no longer reproduce (or reclassified with evidence) | Code + regression evidence; residual resolved/archived |
| **AC-7** | — | Stale items corrected and closed; no false “missing IPC/feature” rows remain for closed set |
| **AC-8** | — | Deferred items have current non-boilerplate rationale |
| **AC-9** | — | `status.json` + `archived/residuals/` match triage outcomes |
| **AC-10** | — | Tech-debt summary refreshed via `tech-debt-rollup.sh` |

## Architecture decisions

- **PM triage is PM-thread:** Verifying residual relevance against current code is orchestration, not implementation.
- **Surgical fixes:** Each fix-now residual gets a minimal code change + regression test. No piggyback refactoring. Fix-now items are independent — no cross-cutting architecture decisions are needed.
- **Stale-close is PM-thread:** Correcting descriptions and archiving is metadata maintenance; requires code evidence before archive.
- **Human smokes stay open:** Dock/Overlay residuals are not Done without human verification.
- **No wire contract changes:** All fix-now items are internal behavior/rendering fixes (port cleanup, timeout deadline, Drop guard, TypeScript helper extraction, JSX escaping, pagination caching).
- **Coordination with P0:** The P0 plan touches `crates/nexus-daemon-runtime/src/api/mod.rs` (route strings). If any fix-now residual also touches daemon code, the P0 and P1 branches should be merged sequentially into `iteration/v1.133` to avoid conflicts. P0 route changes and P1 handler changes are in different files and should not conflict.
