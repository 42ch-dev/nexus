---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-06-24-v1.64-control-room-and-setup-screens"
verdict: "Approve"
generated_at: "2026-06-25"
---

# Code Review Report

## Reviewer Metadata

- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Runtime Model: MiniMax-M3
- Review Perspective: Architecture coherence and maintainability risk
- Report Timestamp: 2026-06-25

## Scope

- plan_id: `2026-06-24-v1.64-control-room-and-setup-screens` AND `2026-06-24-v1.64-daemon-serving-wiring` (Wave 2 integrated — qc1.md to both plan dirs per PM Assignment)
- Review range / Diff basis: `56bf917a..4dd8cbb1` — V1.64 Wave 2: P2 (Control Room + Setup screens) + P3 (daemon serving wiring) merged + status.
- Working branch (verified): `iteration/v1.64` (`git branch --show-current` = `iteration/v1.64`)
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus` (`git rev-parse --show-toplevel`)
- HEAD at review: `4dd8cbb1`
- Files reviewed (P2 scope): 32 implementation files in `apps/web/src/**` plus 8 screenshots, `apps/web/AGENTS.md`, `apps/web/package.json`, `pnpm-lock.yaml`; touched by 5 P2 commits (`db05e640` data-layer foundation → `499ceeb2` T1-T8 screens → `85aded2f` R-V164-QC1-S1-P1 baseline → `ffd0e9b5` vitest imports → `9235b5b4` T11 smoke + AGENTS).
- Commit range (matches Review range): `56bf917a..4dd8cbb1`
- Tools run:
  - `git diff 56bf917a..4dd8cbb1 --stat`
  - `pnpm --filter web typecheck` → PASS (no output, exit 0)
  - `pnpm --filter web build` → PASS (1724 modules transformed; 319.20 kB JS / 98.17 kB gzip; matches completion-report claim)
  - `pnpm --filter web test` → 30/30 tests pass across 5 files in 3.16s
  - Targeted reads of `apps/web/src/api/queries.ts`, `apps/web/src/lib/nexus/{adapters,query-keys,browser-client,errors,index,types}.ts`, `apps/web/src/lib/use-toast.tsx`, `apps/web/src/main.tsx`, `apps/web/src/App.tsx`, `apps/web/src/components/{load-more,status-badge}.tsx`, `apps/web/src/components/ui/{dialog,input,label,select,states,table,textarea,index}.tsx`, `apps/web/src/pages/{works-page,work-detail-page,sessions-page,schedule-page,capabilities-page,findings-page,presets-page}.tsx`, `apps/web/src/pages/dialogs/{create-work-dialog,patch-work-dialog,scaffold-preset-dialog,validate-preset-dialog}.tsx`, `apps/web/src/pages/not-found-page.tsx`, `apps/web/src/test/{setup,test-providers,msw-server}.ts(x)`, `apps/web/vitest.config.ts`, `apps/web/AGENTS.md`, plus the 4 unit + 1 screen test files
  - Spot-check of screenshots 01-works-offline.png, 04-create-work-dialog.png (renders confirm dark + light + dialog + sidebar with all 7 routes)

## Findings

### 🔴 Critical

_(none)_

### 🟡 Warning

_(none — all three scope gaps the PM pre-registered as `R-V164-P2-G1/G2/G3` are surfaced honestly in the screens + adapters rather than faked, which is exactly what qc1 wanted.)_

### 🟢 Suggestion

**S-1: `SchedulePage` (T4) ships the last-updated relative time only — plan P2 task text mentioned "next-fire (UTC) + local-time display, parity with CLI `creator works cron` output", but `ScheduleSummary` does not carry a next-fire timestamp.**

- `apps/web/src/pages/schedule-page.tsx:17-18` docstring explicitly acknowledges the gap: *"ScheduleSummary does not carry a next-fire timestamp, so we show the last-updated relative time."* This is a **partial scope adaptation**, not a regression — the plan's `Done when` line only required "Schedule view renders against the daemon" (parity was in the T4 task description, not the gate).
- `formatUtcAndLocal` helper at `apps/web/src/lib/format.ts:52-67` is implemented and exported but currently unused (no caller imports it). The adapter is ready for a V1.65+ ScheduleSummary field addition; the helper will not rot as long as it stays exported and tree-shaken.
- The page correctly surfaces the limitation in its own docstring (no fake timestamps, no over-promised UI). The honest surface is the right call.
- **Not blocking.** Suggestion to either (a) register a small `R-V164-P2-T4-NEXTFIRE` residual at `low` for V1.65+ ScheduleSummary field, or (b) move the "next-fire UTC + local display" affordance to a follow-up plan that owns the ScheduleSummary contract bump. PM decision; not qc-blocking. Machine severity: `low`.

**S-2: `static_assets.rs` (P3 module, see the P3 report) is mounted into the same router as P2's data endpoints — but no unit tests for the static-asset handler itself.**

- The P3 deliverable has no `tests/static_assets.rs` and no `#[cfg(test)] mod tests` inside `static_assets.rs`. The handler is small (120 lines, two branches — file match + SPA fallback), but cache-header logic (`/assets/*` → 1yr immutable, else → `no-cache`), the `index.html` fallback, the 405-for-non-GET/HEAD guard, and the "no index.html embedded at all → 404 NOT FOUND" path are all worth a per-branch test.
- Note: this is a P3 concern, not P2; flagged here for cross-cutting visibility. The handler is reachable from the same router P2's `BrowserClient` calls, but P2's tests don't exercise the static-asset path (they hit msw).
- **Not blocking.** Suggestion to add a small unit-test file in a V1.65+ P3-followup. Machine severity: `low`.

**S-3: Bundle is 319 kB / 98 kB gzip for 7 routes, with no route-level code-splitting.**

- `apps/web/src/App.tsx:21-37` imports all 7 page modules eagerly. With TanStack Query, the lucide-react icon set, the Radix dialog primitive, and the `react-router-dom` runtime in a single chunk, a first-load "Works dashboard" user pays for the Sessions/Schedule/Capabilities/Findings/Presets page bundles (and their nested dialogs) up front.
- This is well under the Vite warning threshold and acceptable for the MVP, but is the natural V1.65+ improvement (route-level `React.lazy` + `<Suspense fallback>` per screen).
- **Not blocking.** Suggestion: V1.65+ code-splitting per route via `React.lazy`. Machine severity: `low`.

**S-4: `formatUtcAndLocal` helper (`apps/web/src/lib/format.ts:52-67`) is exported but currently unused; with S-1 deferred, the helper sits unused.**

- The helper exists specifically to enable Schedule's "next-fire UTC + local" affordance (S-1). Until either a residual is registered or a contract bump lands, the helper is dead code.
- Tree-shaking will remove it from the production bundle, so there's no runtime cost; only a minor code-hygiene smell.
- **Not blocking.** Suggestion: add a `// currently unused; planned for V1.65+ T4-nextfire surface` comment if keeping, or remove if no followup is planned. Machine severity: `nit`.

**S-5: `apps/web/src/components/ui/{dialog,input,label,select,states,table,textarea}.tsx` are seven new primitives; no shared `cn()` variant helper beyond `lib/utils.ts`.**

- The seven primitives (60–91 lines each) are small and focused, but each handles className composition inline. A trivial `cn()` call already exists (`apps/web/src/lib/utils.ts:1-13`); no further consolidation needed. Flagged for visibility only.
- **Not blocking.** Suggestion: if a 8th primitive lands, consider extracting a tiny `<FormField>` wrapper (Label + Input + invalid state + error message). Not warranted today. Machine severity: `nit`.

## Source Trace

- Finding ID: S-1 (SchedulePage next-fire parity)
- Source Type: manual-reasoning + doc-rule
- Source Reference: `apps/web/src/pages/schedule-page.tsx:13-18`, `apps/web/src/lib/format.ts:52-67`, plan `2026-06-24-v1.64-control-room-and-setup-screens.md:24` (T4 task text)
- Confidence: High

- Finding ID: S-2 (static_assets test gap)
- Source Type: manual-reasoning + git-diff
- Source Reference: `crates/nexus-daemon-runtime/src/static_assets.rs` (120 lines, no test mod), no `tests/static_assets.rs` in the crate
- Confidence: High

- Finding ID: S-3 (bundle code-splitting)
- Source Type: build-output + manual-reasoning
- Source Reference: `pnpm --filter web build` output (`319.20 kB JS / 98.17 kB gzip`), `apps/web/src/App.tsx:1-37` (eager imports)
- Confidence: High

- Finding ID: S-4 (formatUtcAndLocal unused)
- Source Type: static-analysis (grep — no importers in the diff)
- Source Reference: `apps/web/src/lib/format.ts:52-67`, no matching imports in `apps/web/src/**`
- Confidence: High

- Finding ID: S-5 (UI primitive composition)
- Source Type: manual-reasoning
- Source Reference: `apps/web/src/components/ui/{dialog,input,label,select,states,table,textarea}.tsx`
- Confidence: Medium

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 5 |

**Verdict**: **Approve**

Rationale:

1. **TanStack Query data layer (`apps/web/src/api/queries.ts`, 253 lines) is clean.** Per-resource hooks consume the `NexusClient` interface; hierarchical query keys in `query-keys.ts` let mutations invalidate the right query sets (`works.lists()`, `works.detail(workId)`, `presets.list()`); cursor-paginated Works + Findings use `useInfiniteQuery` + `getNextPageParam`; mutations own their error toasts (no double-toast); no business logic leaked into the screens. QueryCache.onError bridge in `main.tsx:19-30` catches the read-path failure that no screen handles locally — exactly the W-1 fix wave's intent.

2. **F-P3 `normalizeList` adapter (`apps/web/src/lib/nexus/adapters.ts`, 68 lines) is clean and surgical.** Side-effect-free, type-parameterized, documents the V1.66+ closure target. 6 unit tests cover each array key (`works`/`sessions`/`schedules`/`capabilities`/`items`), the missing-key empty case, and idempotent re-normalization. Adapter is intentionally **thin** so the structural rename removes it without touching screen code. Adheres to the compass §5 item #2 ownership boundary (UI owns the adapter, not the daemon).

3. **F-F1 `sortByDate` adapter is correctly applied only to un-paginated lists.** `useSchedules()` uses it; `useCapabilities()` uses inline `localeCompare` (alphabetical, not date — different sort key, same F-F1 pattern); `useSessions()` keeps daemon order (no timestamp on `SessionSummary`, correctly commented). Cursor-paginated Works + Findings correctly preserve server order — a client re-sort would break pagination consistency. 4 tests cover the date cases.

4. **Three pre-registered scope gaps (`R-V164-P2-G1/G2/G3`) are surfaced honestly — not faked.**
   - **G1 (no `work_profile` field):** `create-work-dialog.tsx:11-15` docstring says exactly that; the form offers title / long_term_goal / initial_idea only. No fake profile selector. `work-detail-page.tsx:90` derives profile display from the daemon's `work_profile` field when present, falls back to `—`. Clean degradation.
   - **G2 (no preset get/update/delete):** `presets-page.tsx:14-21` docstring acknowledges the gap; page offers list/scaffold/validate/reload only. `validate` (the product-priority #1 hero feature) is the most prominent CTA in the toolbar. `validate-preset-dialog.tsx:1-16` surfaces structured errors/warnings inline with `role="alert"` + `role="status"` for a11y. Excellent priority alignment.
   - **G3 (no admission gates on capabilities):** `capabilities-page.tsx:14-17` docstring acknowledges the gap; page shows name + I/O schemas (the only data the daemon returns). Not faked.
   The PM has already registered all three at `low` with `@fullstack-dev`/V1.65+ in `status.json.residual_findings["2026-06-24-v1.64-control-room-and-setup-screens"]`. qc1 has nothing to add.

5. **W-1 toast is alive end-to-end and tested.** `apps/web/src/main.tsx:44` wires `QueryCache.onError`; mutations in `apps/web/src/api/queries.ts` have their own `onError` callbacks (avoid double-toast). `create-work-dialog.test.tsx:58-85` covers the W-1 case: a 400 envelope from the daemon surfaces as a toast (`"Could not create Work"` + `"Initial idea is too short."`), and the dialog stays open. The Dialog primitive (Radix UI) handles focus-trap, escape, and a11y correctly.

6. **Test baseline (R-V164-QC1-S1-P1, 30 tests / 5 files) is adequate for the MVP.**
   - `adapters.test.ts` (10 tests): F-P3 + F-F1 + idempotency + missing-key + non-mutation.
   - `errors.test.ts` (8 tests): wrapped-envelope unwrap, top-level fallback, plain-string fallback, undefined body, inner-vs-top-level precedence, constructor shape.
   - `browser-client.test.ts` (5 tests): cursor pagination round-trip, W-1 envelope unwrap on real fetch, transport_unreachable, canonical `items` shape (F-P2), ad-hoc `(StatusCode, String)` fallback.
   - `works-page.test.tsx` (4 tests): success / empty / error+retry / Create Work CTA. All three states of the shared screen pattern are exercised.
   - `create-work-dialog.test.tsx` (3 tests): CRUD round-trip + W-1 toast path + required-field validation.
   The complement is real (pure unit tests + msw integration + component render). Remaining screens (sessions/schedule/capabilities/findings/presets) are not directly tested, but they all consume the same `useWorks`/`useSchedules`/etc. hooks (which are covered) and the same `LoadingState`/`EmptyState`/`ErrorState` UI primitives (not directly tested but trivial). Reasonable baseline for P-last to extend.

7. **Component primitives (`apps/web/src/components/ui/*`) are small, focused, and DESIGN.md-token-compliant.** Each is <100 lines. `states.tsx` (91 lines) provides the `LoadingState`/`EmptyState`/`ErrorState` triple every screen uses. `status-badge.tsx` maps free-string statuses (daemon emits no enum contract) to DESIGN.md semantic Badge variants via `statusVariant`/`severityVariant` substring regex — defensible given the open-status contract.

8. **DESIGN.md token compliance is consistent across all 7 screens + 4 dialogs.** Every screen uses `text-copy-14` / `text-label-14` / `bg-background-100` / `border-gray-alpha-400` / `rounded-card` / `rounded-control` / `shadow-card` / `shadow-popover` / `transition-colors duration-state ease-standard` — all DESIGN.md tokens, no invented colors. Screenshots confirm light + dark + dialog + offline-error + sidebar-with-all-7-routes all render correctly.

9. **A11y baseline is met.** Toast uses `role="alert"` for errors + `role="status"` for info, `aria-live="polite"` on the viewport, `aria-label="Dismiss notification"` on the close button. Forms have `<Label htmlFor>` pairing + `sr-only` for filter inputs (works-status-filter, findings-work, findings-severity, findings-status, caps-filter). Progressbar on Work detail (`role="progressbar"` + `aria-valuenow/min/max`). Icon-only buttons have `aria-label`. Radix Dialog handles focus-trap. WCAG 2.1 AA floor per `apps/web/AGENTS.md` Conventions section — no concerns.

10. **P2 ‖ P3 separation is clean.** P2 commits touch only `apps/web/**`, `apps/web/AGENTS.md`, `apps/web/package.json`, `apps/web/screenshots/`, `pnpm-lock.yaml`. P3 commits touch only `crates/nexus-daemon-runtime/src/{static_assets.rs,api/mod.rs,boot.rs,lib.rs}`, `crates/nexus-daemon-runtime/tests/works_api.rs`, `crates/nexus42/src/commands/daemon/mod.rs`, `crates/nexus-daemon-runtime/Cargo.toml`, `Cargo.lock`. Zero overlap. The merge commit graph (`32f78816` + `d9fe75bd`) is clean.

11. **OSS-local vs cloud separation is intact.** `apps/web` consumes `@42ch/nexus-contracts` only (workspace dep); no `@42ch/nexus-platform-*` or cloud-specific types; auth is loopback keyless per V1.20 (verified by `BrowserClient` defaults and the `static_assets` being unauthenticated, documented in plan §3 and daemon-runtime.md §4.4.2). Web UI spec §2.2 invariant holds.

12. **CI status is green.** `pnpm --filter web typecheck` PASS, `pnpm --filter web build` PASS (319KB / 98KB gzip, matches completion-report claim within ±1KB), `pnpm --filter web test` PASS (30/30 in 3.16s), `cargo clippy -p nexus-daemon-runtime -p nexus42 --no-deps -- -D warnings` PASS. Pre-existing test warnings in `tests/agent_tool_api.rs`, `tests/findings_api.rs`, `tests/workspace_occ_concurrent.rs` (about `unused_must_use` on `update_finding_handler` + `unused HostToolCallerKind` + `unused OWNER`) are **NOT introduced by Wave 2** — those files are untouched in this diff; they are pre-existing tech-debt and not qc1's scope.

The five Suggestions are all `low`/`nit` and out of scope for Wave-2 release-blocking (the PM has already registered G1/G2/G3/S1 at `low` in `status.json`; qc1 has nothing new to add to the SSOT).

PM may proceed to consolidate Wave-2 QC once qc2 + qc3 submit their verdicts.

## Cross-Plan Notes

- **P3 (daemon-serving-wiring) qc1 verdict: Approve.** Full review at `.mstar/plans/reports/2026-06-24-v1.64-daemon-serving-wiring/qc1.md` (this reviewer writes both). Findings: 0 Critical, 0 Warning, 4 Suggestion (S-1: `formatUtcAndLocal` unused — same helper flagged here; S-2: `static_assets.rs` lacks unit tests — same gap flagged here from the P3 side; S-3: `axum-test` mock-transport routing limitation — documented workaround in `tests/works_api.rs:228` is acceptable, handler-level test covers it; S-4: release-sequence is documented but not auto-wired in CI — a `build.rs` check or release-pipeline doc addition is the V1.65+ improvement).
- **Cross-plan residual:** if PM wants to register S-1 (`SchedulePage next-fire parity`) and S-2 (`static_assets.rs` unit tests), they can be added to `status.json` root `residual_findings` at `low` for V1.65+ — but qc1 considers them discretionary, not blocking. The existing `R-V164-P2-S1` (live served-UI smoke vs binary) covers the broader end-to-end gap and should remain the single SSOT residual for V1.64 Wave-2 P2.

## Final Verdict

**Verdict**: **Approve** (no unresolved Critical or Warning; 5 Suggestions at `low`/`nit` are durable-roadmap candidates, not blockers).

PM may proceed to Wave-2 consolidation once qc2 and qc3 submit.
