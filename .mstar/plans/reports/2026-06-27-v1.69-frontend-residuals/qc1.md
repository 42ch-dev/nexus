---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-06-27-v1.69-frontend-residuals"
verdict: "Approve"
generated_at: "2026-06-27"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Runtime Model: MiniMax-M3
- Review Perspective: Architecture coherence and maintainability risk
- Report Timestamp: 2026-06-27

## Scope
- plan_id: 2026-06-27-v1.69-frontend-residuals
- Review range / Diff basis: `iteration/v1.69...feature/v1.69-frontend-residuals` (3 commits: `5eacda0c`, `77f26bb3`, `4b1f6433`; ~5 files, +183/−31)
- Working branch (verified): `feature/v1.69-frontend-residuals`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus`
- Files reviewed: 5 (4 source + 1 test fixture)
- Commit range: `5eacda0c..4b1f6433` (matches diff basis verbatim)
- Tools run: `git diff` / `git log`, `pnpm typecheck` (clean), `pnpm vitest run` (121/121 pass — full web app), `rg` for SSOT uniqueness, manual read of every changed file plus the `BrowserClient`/`TauriClient`/`queryKeys` callers

## Findings

### 🟡 Warning

**W-1 — `queryKeys.presets.details()` and `queryKeys.presets.detail(id)` are exported but unconsumed in this PR (R-V167P1-QC3-S2 staged structure).**

The new `details()` / `detail(id)` keys in `apps/web/src/lib/nexus/query-keys.ts` are correctly shaped to match the existing `works` pattern (hierarchical `all` → `details()` → `detail(id)`) and the inline comment explicitly says the actual `invalidateQueries` wiring lands in V1.70 when the canvas consumes them. This is a deliberate "stage the structure now, wire later" decision, anchored to the plan acceptance criteria and called out in C4's risk.

**Why this is `Warning` and not `Suggestion`:** it is intentional and consistent with the plan, but unconsumed exports are a small maintainability risk — a future reader may wonder if they are dead code and could be tempted to delete them. The plan ID + R# comment inside the file already mitigates this. Acceptable, but flagging so the PM keeps a tight link from this PR to the V1.70 canvas implement that consumes them (if V1.70's design diverges and the keys are no longer needed, they should be removed then, not left dangling).

**Disposition:** keep; track as part of V1.70's canvas implement gate.

### 🟢 Suggestion

**S-1 — `isWorkProfile` guard at the `Select.onChange` boundary is dead code at runtime.**

The dialog renders only `WORK_PROFILES.map(...)` as `<option>` children, so `e.target.value` is always a member of the union at runtime. The guard therefore never falls through — it is defense-in-depth for the typed state, which is a reasonable posture, but the current implementation silently *drops* an invalid value (no `workProfileTouched` change, no log). If a future contributor adds an `<option value="…">` typo, the field would silently do nothing.

**Optional, low-cost improvement:** if a future value falls through, prefer `console.warn` (dev-time signal) over silent drop, so the bad `<option>` is surfaced during development. Not blocking — the current code is correct and the comment makes the trade-off explicit.

**Disposition:** leave as-is. If V1.70 extends the dialog, consider a `console.warn` inside the guard.

**S-2 — `WorkProfile` literal union is defined independently of `WORK_PROFILE_VALUES`.**

```ts
export type WorkProfile = 'novel' | 'essay' | 'game_bible' | 'script';
export const WORK_PROFILE_VALUES = [...] as const satisfies readonly WorkProfile[];
```

The two are hand-aligned with a `satisfies` check at the array site only. An alternative: derive the union from the array:

```ts
export const WORK_PROFILE_VALUES = [...] as const;
export type WorkProfile = (typeof WORK_PROFILE_VALUES)[number];
```

This is a minor trade-off — the current design is more grep-friendly (the union lives on a single line for any future author scanning), and `game_bible` is the kind of typo the literal union form surfaces at the type site. **Not blocking.** Both patterns are common; the current form is slightly more readable for a backend-canonical enum.

**Disposition:** no change needed. Mentioning because the diff is on the type-design boundary.

**S-3 — `BACKEND_ACCEPTED_WORK_PROFILES` Set in the test is a parallel list of the same four values.**

`apps/web/src/pages/dialogs/create-work-dialog.test.tsx:191` defines:

```ts
const BACKEND_ACCEPTED_WORK_PROFILES = new Set(['novel', 'essay', 'game_bible', 'script']);
```

The dev's framing (and the test author's comment) is that this is an **oracle**, not a consumer: the test asserts `WORK_PROFILES ⊆ BACKEND_ACCEPTED_WORK_PROFILES`, treating the SSOT as the thing under test and the Set as the authoritative backend snapshot. I agree with that framing — it is the correct test posture for a cross-boundary wire contract. The oracle's value is that adding a UI-only entry would fail the test (forcing a deliberate update of both with a clear reason), rather than silently passing.

**Maintenance hint (not a code change):** if the backend CHECK constraint ever adds or removes a wire profile, this test will need to update in lockstep with the SQL migration. The comment already points at the migration path + Rust helpers, so the trail is complete.

**Disposition:** keep as-is. The dev's framing is correct.

## Verdict per residual (reviewer #1, architecture focus)

| Residual | Verdict | Notes |
|---|---|---|
| **C1** `R-V167P1-QC1-S1` (literal union narrowing) | Approve | `useState<WorkProfile>` + `isWorkProfile` guard at the dialog boundary is the right shape; type-level rejection enforced. |
| **C2** `R-V167P1-QC2-S1` actually `R-V167P1-QC1-S2` (SSOT module) | Approve | `apps/web/src/lib/work-profiles.ts` placement matches the existing top-level utility convention (`format.ts`, `utils.ts`); not bundled into `lib/nexus/` (correct — it is a domain SSOT, not transport). The `WORK_PROFILES` selector array is derived from `WORK_PROFILE_VALUES` + `WORK_PROFILE_LABELS`, so the two cannot drift. |
| **C3** `R-V167P1-QC3-S1` (21→24 + parity guard) | Approve | `as const satisfies readonly (keyof NexusClient)[]` is a strong compile-time + runtime pattern; the runtime `typeof client[method]` check is a useful belt-and-suspenders against a future `class TauriClient` override that drops a method (the compile-time `implements NexusClient` would already catch removal, but the runtime check is cheap defense against re-binding the prototype). Test scope (16 tests, 100% green) is right-sized. |
| **C4** `R-V167P1-QC3-S2` (query-key detail structure) | Approve (with W-1 above) | Extending the existing `queryKeys.presets` (not a separate `presetKeys` module) is the right call — `works` follows the same pattern. The new keys are unused in V1.69 by design. |

## SSOT uniqueness check

```
$ rg -n "WORK_PROFILES|WorkProfile|isWorkProfile" apps/web/src/
apps/web/src/lib/work-profiles.ts:20:export type WorkProfile = 'novel' | 'essay' | 'game_bible' | 'script';
apps/web/src/lib/work-profiles.ts:27:export const WORK_PROFILE_VALUES = [
apps/web/src/lib/work-profiles.ts:35:export const WORK_PROFILE_LABELS: Record<WorkProfile, string> = {
apps/web/src/lib/work-profiles.ts:46:export const WORK_PROFILES: readonly { value: WorkProfile; label: string }[] =
apps/web/src/lib/work-profiles.ts:54:export function isWorkProfile(value: string): value is WorkProfile {
apps/web/src/pages/dialogs/create-work-dialog.tsx:7:import { WORK_PROFILES, isWorkProfile, type WorkProfile } from '@/lib/work-profiles';
apps/web/src/pages/dialogs/create-work-dialog.test.tsx:16:import { WORK_PROFILES } from '@/lib/work-profiles';
```

**Result:** zero duplicate profile lists. The test's `BACKEND_ACCEPTED_WORK_PROFILES` Set is an **oracle** for the comparison test, not a consumer — agrees with the dev's framing. Plan acceptance criterion "no duplicate profile list elsewhere in `apps/web/src/`" is satisfied.

## Surgical discipline check

- 5 files touched, all map cleanly to C1/C2/C3/C4.
- The only "extra" line is the test import update (`WORK_PROFILES` import moved from `create-work-dialog` to `@/lib/work-profiles`); this is a forced move, not a piggyback.
- No `index.css` / `tailwind.config.ts` edit (correct — no token change).
- No `nexus-contracts` change (correct — wire untouched).
- No design-system edit (correct — P0 owns DESIGN.md).
- The dialog file is shorter after the refactor (37-line `WORK_PROFILES` block removed, replaced by a 1-line import + a 5-line guard). Net: dialog becomes more legible, not less.

## Test + typecheck evidence

- `pnpm --filter web typecheck` — clean.
- `pnpm --filter web vitest run` — **15 test files, 121 tests, 100% pass** (1.93s). Includes the 8 dialog tests + 16 adapter-contract tests directly in scope, plus the existing 97 surrounding tests (no regression).
- The new `adapter-contract` test extends from 13 → 16 tests; the new `create-work-dialog` test extends from 5 → 8 tests. Both grow surgically to cover the new behavior.

## Architecture call-outs (reviewer #1 perspective)

1. **Type design (C1/C2)** is cohesive. The `WorkProfile` union is the canonical wire-value set; `WORK_PROFILE_VALUES` is the runtime iterable form (with `as const satisfies` so the two are guaranteed consistent); `WORK_PROFILE_LABELS` is keyed by the union so a missing label is a type error; `WORK_PROFILES` is derived from both. The `isWorkProfile` guard sits at the exact right boundary (the `Select.onChange` transition from `string` to `WorkProfile`).

2. **Test architecture (C3)** is well-conceived. The `as const satisfies readonly (keyof NexusClient)[]` pattern is the canonical TS idiom for compile-time-pinned runtime lists; pairing it with a `typeof client[method] === 'function'` runtime check is appropriate defense-in-depth (the runtime check is also what catches a future "we monkey-patched the method away" scenario, which compile-time cannot see). The test file is long but each new test is targeted and reads as a contract edge, not boilerplate.

3. **Query-key architecture (C4)** correctly extends the existing `queryKeys` module rather than introducing a parallel `presetKeys` namespace — this matches the `works` / `chapters` / `sessions` pattern and keeps invalidation helpers co-located. The hierarchical `all` → `details()` → `detail(id)` shape is consistent with `works.detail(workId)`. The deliberate "no consumer yet" staged structure is documented in code; the cost is a tiny W-1 risk (unconsumed exports look like dead code), which is acceptable given the V1.70 anchor.

4. **SSOT module placement** (`apps/web/src/lib/work-profiles.ts`, not `apps/web/src/lib/nexus/`) is correct: it is a domain enum, not part of the `NexusClient` transport. Sits alongside `lib/format.ts` and `lib/utils.ts` (top-level utilities). If V1.70's Strategy surface grows to consume a richer profile schema, this is the right home for it.

5. **No scope creep.** The 3 commits are tightly scoped: type narrowing + SSOT (C1+C2) → test coverage + parity guard (C3) → query-key stage (C4). No opportunistic refactors, no test reorganization, no formatting churn.

## Source Trace

- Finding ID: W-1 / S-1 / S-2 / S-3 (per-section, not aggregated)
- Source Type: manual-reasoning + git-diff + read (with test execution evidence)
- Source Reference: `apps/web/src/lib/nexus/query-keys.ts:38-46`, `apps/web/src/lib/work-profiles.ts:1-56`, `apps/web/src/pages/dialogs/create-work-dialog.tsx:122-131`, `apps/web/src/lib/nexus/adapter-contract.test.ts:372-421`, `apps/web/src/pages/dialogs/create-work-dialog.test.tsx:179-208`
- Confidence: High

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 1 (W-1: unconsumed query-key exports, deliberately staged for V1.70) |
| 🟢 Suggestion | 3 (S-1: silent drop in guard, S-2: type-vs-array design choice, S-3: oracle/parallel-list note) |

**Verdict**: **Approve**

The four residuals (C1–C4) are closed surgically and the architecture is coherent. The `WorkProfile` literal union + `WORK_PROFILES` SSOT module is well-shaped; the `isWorkProfile` guard sits at the correct boundary; the `adapter-contract` 21→24 extension plus the `as const satisfies ... + typeof` parity guard is a maintainable pattern; the staged preset query-key structure matches the `works` pattern and is explicitly documented as a V1.70 anchor. SSOT uniqueness is verified — no duplicate profile list anywhere in `apps/web/src/`. Typecheck clean, all 121 web-app tests pass. The single Warning (unconsumed exports) is intentional and tracked by the plan; nothing here blocks the merge.

## Plan Update / Handoff

- **Recommendation to PM:** mark R-V167P1-QC1-S1, R-V167P1-QC1-S2, R-V167P1-QC3-S1, R-V167P1-QC3-S2 in the V1.67 plan's `residual_findings` as `lifecycle: resolved` with `resolution.plan_id: 2026-06-27-v1.69-frontend-residuals` once all three QC reports + QA pass. W-1 should be re-evaluated when V1.70 lands the canvas invalidation wiring (if the keys are no longer needed, remove them then).
- **No code change required from implementer before merge.** S-1 / S-2 / S-3 are non-blocking and can be addressed opportunistically.
- **No residual left open by this review.**
