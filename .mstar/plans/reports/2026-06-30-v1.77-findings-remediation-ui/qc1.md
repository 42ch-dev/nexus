---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-06-30-v1.77-findings-remediation-ui"
verdict: "Approve"
generated_at: "2026-06-30"
---

# Code Review Report

## Reviewer Metadata

- Reviewer: @qc-specialist
- Runtime Agent ID: `qc-specialist`
- Runtime Model: minimax-cn-coding-plan/MiniMax-M3
- Review Perspective: Architecture coherence and maintainability risk (Reviewer #1)
- Report Timestamp: 2026-06-30
- Deep review: **triggered** — change size (10 commits, ~600 LOC of net core code across 11 web app files + 3 spec files + 2 DESIGN files), sensitive module (Control-Room findings = primary triage UX surface), new domain (first TanStack optimistic-mutation consumer-side implementation in the web app, new client-side state-machine mirror module), multi-module coupling (clients × types × queries × panel × page × tokens × adapter-contract test × 3 specs).

Lenses applied (single-reviewer, no subagents):

- Architecture coherence (default)
- Module-boundary / cohesion (does the new module set respect `apps/web` structure?)
- SSOT-duplication (does the client-side lifecycle mirror threaten the DAO as the transition authority?)
- API-contract (does `getFinding` / `updateFinding` follow the established `NexusClient` promotion pattern?)
- Mutation semantics (does the TanStack `useUpdateFinding` follow the existing query-keys hierarchy + cancel/snapshot/optimistic/rollback pattern?)
- Spec ↔ implementation drift (does `findings-lifecycle.md`, `web-ui.md §23`, `local-api-surface-conventions.md §11` agree with the code?)
- Maintainability (naming, modular size, error handling, defensive checks)

## Scope

- plan_id: `2026-06-30-v1.77-findings-remediation-ui`
- Review range / Diff basis: `git diff ba71d9167f6269cd0175b86f202baa3e19b517a6...a2571381b2a9865c6a98ffec461d4a99051a39f0` (10 commits; merge-base `ba71d916` = origin/main, tip `a2571381` = HEAD)
- Working branch (verified): `iteration/v1.77`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus`
- Files reviewed: 11 web app source files + 3 spec files + 2 DESIGN files + 1 plan + status.json + AGENTS.md + CONCEPTS.md (29 changed paths total under this plan; P1 plan B3 (`world-kb-canvas.tsx`) is reviewed separately under `qc1.md` for P1)
- Commit range (matches Review range exactly)
- Tools run: `git rev-parse --show-toplevel` (confirms cwd); `git branch --show-current` (confirms `iteration/v1.77`); `git log --oneline ba71d916..HEAD` (10 commits); `git diff --stat ba71d916...HEAD` (29 files); `grep` + targeted `read` for each surface; structural verification against `crates/nexus-local-db/src/findings.rs:172-189` (`is_valid_transition`) and `schemas/local-api/findings/update-finding-request.schema.json` (wire truth for `rule_suggestion`)

## Findings

### 🔴 Critical

(none)

### 🟡 Warning

(none)

### 🟢 Suggestion

- **S-001 — `useFinding` (detail hook) is staged forward but currently unused.** `apps/web/src/api/queries.ts:158` exports `useFinding(workId, findingId)` (TanStack `useQuery` against `queryKeys.findings.detail`), but a `grep` across `apps/web/src/**/*.{ts,tsx}` shows no consumers. The `FindingDetailPanel` (`apps/web/src/components/findings/finding-detail-panel.tsx:209`) is fed by `findings-page.tsx:54-57` selecting the selected row from the list cache. The hook is intentional staging for an eventual detail-page surface (e.g. dedicated `/findings/:id` route or a future Inspector whose source of truth is the detail endpoint instead of the list cache). The doc comment on `useFinding` is accurate. Recommended action: add a one-line `@remarks` or a tracked follow-up so the next maintainer does not assume it is dead code. **Not blocking** — this is a coherent forward-staging hook, not a regression.

- **S-002 — `finding-detail-panel.tsx` is 305 lines, exceeding the 250-line module-size discipline referenced in the assignment.** The compass claim that `findings-page.tsx` is 224 lines is verified. `finding-detail-panel.tsx` (305) is intentionally a single component for the three remediation affordances (status transitions / `target_executor` / inline edit) and is well-sectioned with banner comments (`{/* ── Status transitions ── */}` etc.). Splitting is feasible but adds file-count for marginal gain at this iteration. Recommended follow-up (non-blocking for V1.77): if a future iteration adds bulk-edit or autocomplete affordances, split into `status-transitions-panel.tsx` + `target-executor-selector.tsx` + `inline-edit-form.tsx` (each < 200 lines) instead of growing this file further.

- **S-003 — `apps/web/src/lib/findings-lifecycle.ts` (client-side adjacency mirror) is a **justified** duplication of the server-side DAO `is_valid_transition()` (D1a LOCKED — defense-in-depth + UX). The drift risk is currently mitigated by:
  - The mirror module's doc comment explicitly names the DAO source (`crates/nexus-local-db/src/findings.rs:172`) and states "the server is the authority" (`apps/web/src/lib/findings-lifecycle.ts:11-12`).
  - The companion `findings-lifecycle.test.ts` pins every adjacency row (8 it-blocks) **plus** the server-side `is_valid_transition_matches_lifecycle_diagram` test in `crates/nexus-local-db/src/findings.rs:2337-2380` pins the same shape against a Rust unit test.
  - The findings-lifecycle.md spec §2.2 is the human-facing SSOT and matches both implementations.

  Recommended **non-blocking** follow-up at V1.78+ hygiene: a single golden test (currently absent) that reads the client `TRANSITIONS` table and asserts equality with the server's adjacency, would close the small remaining drift hole. Today, the two test suites are independent and could both pass while disagreeing on a row.

- **S-004 — Snapshot fan-out vs. apply fan-out is correct but asymmetric.** `useUpdateFinding` in `apps/web/src/api/queries.ts:247-294`:
  - Snapshot reads via `qc.getQueriesData<FindingsListData>({ queryKey: queryKeys.findings.list(vars.workId) })` (line 252-254) — **all** matches under the `findings.list` branch for the work.
  - Optimistic apply via `qc.setQueriesData<FindingsListData>({ queryKey: queryKeys.findings.list(vars.workId) }, …)` (line 260-274) — same scope.
  - Rollback restores per-key via `qc.setQueryData(queryKey, data)` in a loop (line 278-282) — **exact** key match.

  TanStack's `setQueryData` ignores filters that are not part of the **exact** key (the `query ?? {}` arg in `queryKeys.findings.list` becomes part of the key tuple), so a stale snapshot for `{limit: 20}` rolls back the entry for `{limit: 20, status: 'open'}` only if the keys are identical. Today the optimistic apply path uses the `vars.workId` branch as the query-key prefix, so it correctly fans out to every `status` / `severity` filter under the same `workId` (the line-260 `setQueriesData` matches all of them). Functionally correct as written. Worth a follow-up comment in V1.78+ hygiene if pagination cursors become page-scoped keys: the snapshot-vs-apply scope must remain co-extensive or rollback can lose context.

### Verdict-supporting notes (informational; not findings)

- **D1a–D4 LOCKED decisions reflected in code:**
  - D1a (server-enforced adjacency + UI defense-in-depth): `isValidTransition` / `nextStatuses` / `isTerminalStatus` consume the mirror table and disable invalid transitions; `FindingDetailPanel` renders `<span>Terminal — no further transitions.</span>` for terminal states and **omits** the "Not reachable:" block when terminal (`finding-detail-panel.tsx:127-158`). `mappings-card.test.tsx:75-82` verifies the terminal rendering; `findings-mutation.test.tsx:106-138` verifies the rollback path on a 422 `INVALID_TRANSITION`.
  - D1b (last-writer-wins, no OCC): no revision/version field requested; no conflict modal; optimistic-update safety holds because there is no concurrent-author scenario.
  - D1c (update-only): the `NexusClient` interface gains only `getFinding` + `updateFinding`; `createFinding` / `deleteFinding` are intentionally absent (verified by `grep` on `lib/nexus/types.ts`).
  - D2 (codegen wiring already done): `apps/web/src/lib/nexus/types.ts:36-83, 76, 168-179, 277` imports `FindingDetailResponse` + `UpdateFindingRequest` from `@42ch/nexus-contracts`. `packages/nexus-contracts/src/generated/index.ts:67, 71, 237, 241` confirms both are barrel-exported; both `lib/nexus/types.ts` and `lib/nexus/browser-client.ts` import them directly. No `pnpm run codegen` churn.
  - D3 (`wire_contracts_changed: FALSE`): confirmed by `git diff` — no files under `schemas/` are touched.
  - D4 (detail-panel + row-action hybrid): `findings-page.tsx:141-219` renders a 2-column grid (`minmax(0,1fr)_360px`) with the table on the left and the Inspector `Card` on the right; row-level assignment `Select` lives in the table (`findings-page.tsx:172-185`); detail panel mounts beside (`findings-page.tsx:199-218`).

- **`NexusClient` interface contract — fits the established promotion pattern.** `getFinding` / `updateFinding` add **2 methods** (25 → 27 in the interface, then 27 in `adapter-contract.test.ts` line 183 after the test was updated to expect 27 paths). The placement follows V1.67 G2 (`getPreset` / `updatePreset` / `deletePreset`) and V1.70 / V1.71 / V1.72 / V1.73 / V1.74 promotions. URL routing, `encodeURIComponent` on both `workId` and `findingId`, and method verbs (GET / PATCH) all match the established convention (`browser-client.ts:191-205`). The `adapter-contract.test.ts` parity guard at lines 454-522 mirrors the V1.67 G2 findings-method guard pattern (compile-time `satisfies readonly (keyof NexusClient)[]` + runtime typeof + path/verb spot-checks); it explicitly verifies `TauriClient` inherits both methods via thin-over-`BrowserClient` without an override.

- **TanStack state management — pattern-consistent.** `useUpdateFinding` follows the existing mutation idiom in `apps/web/src/api/queries.ts`:
  - `useCreateWork` / `usePatchWork` / `useScaffoldPreset` / `useReloadPreset` invalidate `queryKeys.<resource>.lists()` on success.
  - `useUpdateFinding` goes a step further with optimistic apply + rollback (the V1.77 affordance requires UI consistency on a single screen until settle). The mutation structure is correctly modelled: `cancelQueries` → `getQueriesData` (snapshot) → `setQueriesData` (optimistic, defined-only fields per `Object.fromEntries(Object.entries(patch).filter(...))` to avoid clobbering cached values with `undefined`) → return context. On error, restore snapshots. On success, success toast. On settle, `invalidateQueries` for both list and detail. The detail invalidation is essential because the panel could later be sourced from `useFinding`.
  - Invalidation keys (`queryKeys.findings.lists()`, `queryKeys.findings.detail(workId, findingId)`) match `queryKeys.findings.list` / `queryKeys.findings.detail` (introduced in `apps/web/src/lib/nexus/query-keys.ts:32-40`), preserving the `all ⊃ lists() ⊃ list(workId, query)` hierarchy.

- **Spec ↔ implementation drift — none.** Verified end-to-end:
  - `findings-lifecycle.md` §2.2 adjacency table ↔ DAO `is_valid_transition` ↔ client `TRANSITIONS` (3-way mirror, all identical).
  - `findings-lifecycle.md` §4.1 PATCH field semantics (incl. A6 correction at commit `98471e34`) ↔ `schemas/local-api/findings/update-finding-request.schema.json` (which declares `rule_suggestion` as `{"type": "string"}`, non-nullable) ↔ client `buildPatch` (`finding-detail-panel.tsx:50-63`) which treats `finding.rule_suggestion ?? ''` as the cache baseline and `form.ruleSuggestion` as the new value (an empty form value therefore sends an empty string on the wire, which clears the field — matching the A6 spec wording: "empty string clears, omitting leaves it unchanged").
  - `web-ui.md` §23 (V1.77 stage description) ↔ `apps/web/DESIGN.md` §Findings Remediation (token table + interaction rules) ↔ `apps/web/DESIGN.dark.md` (same token names with dark values) ↔ `apps/web/src/components/status-badge.tsx:100-137` (`findingStatusClasses` returns the same 6 color classes) ↔ `finding-detail-panel.tsx:125` (`<FindingStatusBadge status={finding.status} />`).
  - `local-api-surface-conventions.md` §11 (V1.77 amendment) ↔ `apps/web/src/lib/nexus/adapter-contract.test.ts:488-522` (path/method parity for `getFinding` / `updateFinding`). The new §11 explicitly distinguishes non-OCC PATCH from the §7 canvas OCC convention — a useful piece of architecture documentation for future maintainers adding similar non-OCC resources.

- **Maintainability — naming + error handling:**
  - Naming: `useUpdateFinding`, `useFinding`, `useFindings`, `getFinding`, `updateFinding`, `FINDING_STATUSES`, `FindingStatus`, `SEVERITY_OPTIONS`, `TARGET_EXECUTOR_OPTIONS` — all consistent with the surrounding `usePatchWork`, `useScaffoldPreset` etc., and consistent with the `WorldKb*` / `Chapter*` / `Preset*` family naming.
  - Error handling: `useUpdateFinding` rolls back via stored snapshot; `useErrorToast` already covers `NexusClientError` and `Error` uniformly (`apps/web/src/api/queries.ts:186-197`); the optimistic patch's `Object.fromEntries(... .filter(([, v]) => v !== undefined))` guard prevents an `undefined` field in the patch from clobbering a cached value. The 422 path is explicitly tested in `findings-mutation.test.tsx:106-138`.
  - Comments document intent rather than narrate code: `finding-detail-panel.tsx:1-12` names the three affordances and references `web-ui.md §23` + `findings-lifecycle.md §4`; `findings-lifecycle.ts:1-15` explicitly names the DAO enforcement site and the V1.77 context.

## Source Trace

| Finding ID | Source Type | Source Reference | Confidence |
|-----------|-------------|------------------|------------|
| S-001 | manual-reasoning | `apps/web/src/api/queries.ts:158-165`; `grep -n "useFinding\b" apps/web/src/ -r` returns only the declaration | High |
| S-002 | git-diff + file length | `wc -l apps/web/src/components/findings/finding-detail-panel.tsx apps/web/src/pages/findings-page.tsx` → 305 / 224; user-supplied 250-line discipline referenced in Assignment | High |
| S-003 | doc-rule + manual-reasoning + static-analysis | `apps/web/src/lib/findings-lifecycle.ts:1-15` (doc + DAO line ref); `findings-lifecycle.test.ts` (8 it-blocks); DAO test `is_valid_transition_matches_lifecycle_diagram` at `findings.rs:2337`; DAO enforcement at `findings.rs:172-189` | High (justification solid; gap explicitly limited to a future cross-impl golden test) |
| S-004 | manual-reasoning | `apps/web/src/api/queries.ts:247-294` (snapshot fan-out line 252; apply fan-out line 260; per-key rollback line 278-282) | Medium (TanStack Query v5 semantics; bug risk re-emerges if filters get page-cursor scope under the same `workId` branch) |

Architectural coherence and locked decisions verified against (all High confidence):

| Decision | Source | Verification |
|----------|--------|--------------|
| D1a (server-enforced + UI defense-in-depth) | `crates/nexus-local-db/src/findings.rs:172-189`; `apps/web/src/lib/findings-lifecycle.ts:50-65`; `finding-detail-panel.tsx:95-157`; `finding-detail-panel.test.tsx:75-82` | Client mirror matches server; rendering hides invalid transitions; roll-back path tested with 422 |
| D1b (last-writer-wins, no OCC) | `findings-lifecycle.md` §4.4; `local-api-surface-conventions.md` §11.2; `apps/web/src/api/queries.ts:228-237` (doc comment) | No revision/expected_version field is sent; `useUpdateFinding` doc explicitly states no conflict modal |
| D1c (update-only) | `apps/web/src/lib/nexus/types.ts:159-179`; `findings-page.tsx` (no create/delete UI) | `createFinding` / `deleteFinding` are not in the interface or in any consumer |
| D2 (codegen wiring done) | `packages/nexus-contracts/src/generated/index.ts:67, 71, 237, 241`; `apps/web/src/lib/nexus/types.ts:36-83, 76, 168-179`; `apps/web/src/lib/nexus/browser-client.ts:23, 55, 191-205` | Types barrel-exported and imported at the only two layers that need them |
| D3 (`wire_contracts_changed: FALSE`) | `git diff --stat ba71d916...HEAD -- schemas/` (empty) | No schema files touched |
| D4 (detail-panel + row-action hybrid) | `web-ui.md` §23.1; `apps/web/src/pages/findings-page.tsx:141-219` | 2-col grid + row-level assignment selector in table; detail inspector in right column; existing `Table` + `StatusBadge` reused |

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 4 |

**Verdict**: Approve
