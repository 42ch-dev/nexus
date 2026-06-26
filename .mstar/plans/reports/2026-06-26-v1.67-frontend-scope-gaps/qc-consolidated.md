# QC Consolidated — V1.67 P1 Frontend Scope Gaps

**plan_id**: 2026-06-26-v1.67-frontend-scope-gaps · **Consolidated by**: @project-manager · **Working branch**: iteration/v1.67 · **Review range**: P1 `e74321db`+`963fa1ed`+`aeaaf91a`, diff basis `26e477ee`
**Consolidated verdict**: **Request Changes**

| Seat | Verdict | Blocking |
|---|---|---|
| qc1 (arch) | Request Changes | **C1** UI sends `work_profile:'game-bible'` but backend canonical is `'game_bible'` (3/4 options coincide); **W1** default `'novel'` vs V1.66 NULL drift |
| qc2 (security) | Approve | — |
| qc3 (perf) | Approve | — |

## Fix wave (blocking)
1. **C1 (Critical)**: reconcile `work_profile` canonical value. Verify the backend's canonical value (grep `WorkProfile` enum serde representation + existing `creator bootstrap --profile` accepted values); fix the UI `WORK_PROFILES` const to emit the canonical value (likely `game_bible` underscore) so all 4 options match. Add an assertion test covering all 4 values round-trip to the backend-accepted set.
2. **W1**: preserve V1.66 semantics — only send `work_profile` when the user explicitly selects (omit the field when the form is untouched → daemon stores NULL as before); default-display `novel` in the UI is fine as long as it isn't sent unless chosen.

## Deferred as residuals (register)
- `R-V167P1-QC1-S1`: narrow `useState<string>` to literal union (qc1 S1, low).
- `R-V167P1-QC1-S2`: extract `WORK_PROFILES` to a SSOT module (qc1 S2, low).
- `R-V167P1-QC3-S1`: refresh `adapter-contract.test.ts` 21→24 + preset-method parity guard (qc3 S1, low).
- `R-V167P1-QC3-S2`: preset detail/update/delete query keys + invalidation when V1.68 UI lands (qc3 S2, low → V1.68 canvas).

## Re-review after fix wave
Targeted: **qc1 only** (raised C1+W1). Update `qc1.md` `## Revalidation`.
