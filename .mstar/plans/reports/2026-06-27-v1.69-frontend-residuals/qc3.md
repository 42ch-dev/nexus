---
plan_id: 2026-06-27-v1.69-frontend-residuals
reviewer: qc-specialist-3
focus: performance-and-reliability
date: 2026-06-27
diff_basis: iteration/v1.69...feature/v1.69-frontend-residuals
verdict: Approve
critical: 0
warning: 0
suggestion: 0
---

# QC3 — V1.69 P1 Frontend Residuals — Performance & Reliability Review

## Findings

None

## Assessment by residual

### C1 — work_profile literal union

Approved. The `WorkProfile` narrowing is reliable at runtime and preserves the existing submission semantics.

- `CreateWorkDialog` initializes and resets `workProfile` from `WORK_PROFILES[0].value`, so normal component state is always populated with a valid literal value.
- `isWorkProfile(value: string)` checks membership in the closed `WORK_PROFILE_VALUES` array. For the actual DOM event path, `e.target.value` is always a string; unexpected values such as `''` are rejected without throwing.
- Invalid DOM/select values are ignored before they enter typed state. This leaves the last valid state intact and does not mark `workProfileTouched`, so an unexpected invalid value cannot cause an invalid `work_profile` payload to be submitted.
- `null`/`undefined` are not reachable through the current React `ChangeEvent<HTMLSelectElement>` path. If a future non-DOM caller tried to bypass TypeScript and call the guard with those values, the declared `string` signature would be violated; no current consumer does that.
- Existing backward compatibility is preserved: untouched forms still omit `work_profile`, so the daemon can store NULL as before.

Reliability note: the guard silently drops an invalid select value. That is acceptable for this residual because it prevents inconsistent state and avoids throwing in the UI. A future developer-facing warning could improve diagnosability, but it is not required for safe runtime behavior.

### C2 — WORK_PROFILES SSOT module

Approved. The SSOT extraction introduces negligible runtime/bundle risk and no import-cycle concern.

- `apps/web/src/lib/work-profiles.ts` is a small, dependency-free module exporting constants, a literal type, derived selector options, and a pure guard.
- The dialog imports the module one-way; the SSOT module imports nothing from React, the dialog, query code, or the Nexus client, so there is no cycle risk.
- Runtime cost is bounded to constructing a four-item derived array at module load and a four-item `includes` check on select changes. This is not a hot-path performance concern.
- The extraction removes the profile list from the dialog and keeps the canonical frontend list in one place. Grep found no second frontend consumer-side profile list in `apps/web/src/`; the test's `BACKEND_ACCEPTED_WORK_PROFILES` set is an explicit backend oracle for contract parity, not a UI duplicate.
- Backward compatibility is preserved for existing imports used by production code. The only previous test import from the dialog was updated to import the SSOT module directly; no production API was removed beyond moving an implementation detail out of the dialog file.

### C3 — adapter-contract test parity

Approved. The new adapter-contract tests are deterministic and passed under both targeted and full web test runs.

- The new get/update/delete preset contract test uses a local injected `fetchImpl`, records calls synchronously in an array, and returns fixed `Response` objects. It has no timer, network, random, or shared global dependency.
- The Tauri transport parity test uses a local `Set` and injected fetch stub, so method coverage is deterministic and isolated per test invocation.
- The preset-method parity guard is a static curated list with `satisfies readonly (keyof NexusClient)[]` plus runtime `typeof client[method] === 'function'` checks. That combination catches interface rename/removal at typecheck and adapter drift at test time.
- Existing MSW-based tests continue to use per-test handler registration through `useHandlers`; I saw no new shared-state or mock-leakage risk from this delta.
- Verification passed:
  - `pnpm --filter @42ch/nexus-contracts run build`
  - `pnpm --filter web run typecheck`
  - `pnpm --filter web run test -- adapter-contract create-work-dialog --run` — 15 files / 121 tests passed. Although the command name targeted the changed files, the current Vitest invocation also ran the full web suite because the extra `--run` argument was forwarded after the script's existing `vitest run`.
  - `pnpm --filter web run test` — full rerun, 15 files / 121 tests passed again.

The only stderr output was React Router future-flag warnings already emitted by existing tests; no failing or flaky behavior appeared.

### C4 — preset query-key structure

Approved. The hierarchical preset detail key factory is stable for TanStack Query cache identity and prepares safe invalidation for V1.70.

- The staged keys are `['presets', 'detail']` and `['presets', 'detail', presetId]`, matching the existing hierarchical style used by `works`, `chapters`, and other resources.
- The list key remains `['presets', 'list']`, so list and detail caches do not collide.
- The shared prefix `queryKeys.presets.all` (`['presets']`) can invalidate all preset-related entries when V1.70 wires detail consumers; `queryKeys.presets.detail(id)` can target one preset without touching unrelated ids.
- `presetId` is appended as a single array element rather than concatenated into a string, so ids containing slashes (for example `user/foo`) do not collide with other key segments.
- No current mutation was changed to invalidate a narrower key prematurely. Existing invalidation still targets the list key for current list-only consumers, while the code comment documents the future detail invalidation model.

Stale-cache risk is low for this PR because no detail query consumer exists yet. When V1.70 introduces one, mutations that update/delete an individual preset should invalidate either `queryKeys.presets.all` or the relevant detail/list keys consistently.

## Verdict rationale

Approve. The implementation closes C1–C4 without introducing performance or reliability regressions. The profile guard fails closed, the SSOT module is dependency-free and cheap, the new tests are deterministic and passed under rerun, and the preset query-key shape is collision-resistant and compatible with future broader invalidation.

Scope note: `git log iteration/v1.69..HEAD` on the assigned branch currently includes the three implementation commits (`5eacda0c`, `77f26bb3`, `4b1f6433`) plus earlier QC report commits (`4bd038f3`, `33a3cdf5`). I reviewed the implementation files from the three expected implementation commits and treated the prior QC reports as existing review artifacts, not implementation changes.
