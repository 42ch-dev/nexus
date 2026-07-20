# Spec — Control Room author-loop dogfood fixes (P0)

> **Iteration:** V1.127 · **Plan:** `2026-07-20-v1.127-p0-control-room-author-loop-fixes`
> **Status:** PM draft — pending product-manager seat 1 + architect seat 2 + writing-specialist seat 3.
> **Source evidence:** Predictive scan by `explore` subagent (V1.127 Phase 1); V1.126 residual cluster; manual verification of Setup-Continue hotfix on `main` HEAD.

## Problem

A manual tester running through the primary author loop in V1.126 (open app → Setup → Worlds list → Create World → enter World → Timeline → switch layers → open Work → Outline → agent test) hits five observable bugs in the first 30 minutes:

1. **Create World is a no-op** — `apps/web/src/pages/worlds-page.tsx::handleCreateWorldClick` (line 36) is an empty function. V1.125 shipped this stub; V1.126 P3 cleanup did not pick it up. Clicking the card does nothing.
2. **Cursor pagination missing** — `apps/web/src/components/global-timeline/global-timeline-view.tsx:40` and `apps/web/src/pages/worlds-page.tsx:24` call `useTimelineOverview()` without a cursor. V1.126 P2 backend returns `has_more` + `cursor` but the web consumer ignores them. Tester with >20 worlds sees only the first 20 with no Load More. Flagged by V1.126 P2 residual `R-V1126P2-QC-S-006`.
3. **Silent overview 5xx** — `apps/web/src/pages/worlds-page.tsx:24-34` checks `!overview.data` but never `overview.isError`. On backend error the UI silently shows "No recent activity" for every world. Flagged by V1.126 P2 residual `R-V1126P2-QC-S-008`.
4. **Selection submenu popover drift on resize/scroll** — `apps/web/src/components/selection-submenu/selection-submenu.tsx:93-102` has an outside-click listener but no `resize`/`scroll` listener. Popover position is computed once at render. Flagged by V1.126 P0 residual `R-V1126P0-QC-S-002`.
5. **Stale `renamingItem` on pathname change** — `apps/web/src/components/layout/sidebar.tsx:75-77` has `renamingItem` state independent of the chrome's `submenuItem` and is never cleared on navigation. Flagged by V1.126 P0 residual `R-V1126P0-QC-S-003`.

A sixth, adjacent item is the stale `R-HOTFIX-SETUP-CONTINUE-404` HIGH residual — the fix `e320e62d` shipped on `main` HEAD (verified via `git merge-base --is-ancestor e320e62d main`) but the residual row was never moved to `archived/`. This is a 5-minute admin task that closes the HIGH-severity open count.

## User value

**Before V1.127**, a dogfood tester running the primary author loop (open app → Worlds list → Create World → enter World → Timeline → Work → Outline → agent) hits five friction points in the first 30 minutes — and the open-residual dashboard keeps flagging a HIGH Setup-Continue row that was actually fixed weeks ago. None of these are show-stoppers individually; collectively they make the product feel broken on first contact.

**After V1.127**, the same tester:

- Clicks "Create World" on the Worlds page and either lands in the new World's Timeline (desktop) or sees a clear "use the desktop app" tooltip (browser) — never silence.
- Browses the Global Timeline and Worlds overview with a "Load More" control, so all worlds are reachable even past the first 20.
- Sees an inline "Couldn't load recent activity — Retry" banner on a 5xx instead of silent fallback text.
- Can resize the window or scroll the sidebar with a selection submenu open without the popover drifting off its trigger row.
- Can click Rename on a sidebar row, then navigate via another NavLink, and not see a stale rename input stranded in the new view.
- (PM sees the Setup-Continue HIGH row leave the open-residual count — bookkeeping noise gone.)

Net: the friction points the V1.127 predictive scan ranked as P0/P1 are no longer in the manual-test path; the next round of dogfood review tests the user's own creative surface, not ours.

## Scope

**In scope (P0):**

- T1 Create World wiring (desktop call OR browser graceful-disabled) — `worlds-page.tsx`
- T2 Cursor pagination UI (Load More control) — `global-timeline-view.tsx`, `worlds-page.tsx`, `queries.ts`
- T3 Overview error state (inline banner with Retry) — `worlds-page.tsx`
- T4 Selection submenu resize/scroll listener — `selection-submenu.tsx`
- T5 Clear stale `renamingItem` on pathname change — `sidebar.tsx`
- T6 Close `R-HOTFIX-SETUP-CONTINUE-404` residual (admin JSON move) — `status.json` + `archived/residuals/`

**Out of scope (roadmap):**

- `total_worlds` cleanup (scan item 7), dynamic-SQL refactor (scan item 8), NavItemLi memo (scan item 9), two-source identity refactor (scan item 10) — see V1.127 NG-4/5/6.
- Remaining 22 V1.126 nit residuals — see V1.127 NG-6.
- Daemon DELETE routes for works/worlds — see V1.127 NG-3.
- Selection-submenu Delete action — see V1.127 NG-11 (depends on DELETE routes).
- `AgentPicker` component refactor — see V1.127 NG-12.

## Acceptance criteria

See compass `## Acceptance Criteria` AC-V1127-1 through AC-V1127-6. Each AC maps to exactly one task.

## Architecture decisions (PM proposed — pending architect seat 2)

- **`wire_contracts_changed: false`** — all five product fixes are frontend-only; the residual close is admin only.
- **T1 fallback** — if `client.createWorld` is absent on every bridge (browser + desktop), ship graceful-disabled-with-tooltip only; log a V1.128+ roadmap entry for the desktop bridge wiring (do NOT silently add a Tauri command, which would be a wire change).
- **T2 pattern** — prefer TanStack Query's `useInfiniteQuery`; fallback to manual cursor state with `useTimelineOverview({ cursor })`. Single shared hook extension; do not duplicate cursor logic across consumer + Worlds page.
- **T3 scope** — banner scoped to overview sub-section only; world list (`useNarrativeWorlds`) keeps its own error handling.
- **T4 UX** — dismiss-on-layout-change (vs reposition); submenu is cheap to reopen.
- **T5 trigger** — clear on pathname change ONLY; do NOT clear on route-parameter change (e.g. `:workId` swap inside same route).
- **T6 archive shape** — mirrors `.mstar/archived/residuals/<plan-id>.json` V1.126 P3 pattern; new `closure_note` enum value `"Shipped — <commit-sha> on <branch> (<PR-link>)"` extends V1.126 P3's four-value enum.

## Open questions (resolved — architect seat 2)

- **AQ-1 (T1 — resolved):** `client.createWorld` **absent** on every bridge. `apps/desktop/src-tauri/src/lib.rs:983-1003` — the `invoke_handler` lists 18 Tauri commands; `createWorld` / `create_world` is not among them. `apps/web/src/lib/nexus/types.ts:141-454` — the `NexusClient` interface has no `createWorld` method. `apps/web/src/lib/nexus/create-world.ts:4-7` — `hasCreateWorldClient` is a feature-detect guard that always returns `false` because no bridge exposes the method. **Verdict:** T1 ships graceful-disabled-with-tooltip variant ONLY. No desktop IPC wiring in this iteration (would be a wire contract change — NG-16). Log a V1.128+ roadmap entry for adding `create_world` Tauri command + `NexusClient.createWorld`.
- **AQ-2 (T2 — resolved):** `useTimelineOverview` already accepts `cursor?: string`. `apps/web/src/api/queries.ts:211` — `export function useTimelineOverview(cursor?: string)`. The hook signature is ready; T2 only adds the pagination accumulator + Load More UI. No hook refactor needed.
- **AQ-3 (T3 — resolved):** `ErrorState` component exists at `apps/web/src/components/ui/states.tsx:112`. It is already imported and used in `worlds-page.tsx:9,65` for the world-list error state. `global-timeline-view.tsx:61-69` uses `GlobalTimelineListChrome` (not `ErrorState`) — that is a list-chrome pattern, not a reusable inline banner. T3 reuses `ErrorState` (scoped to overview sub-section above the world list), with `title` + `onRetry` callback. No new component needed.
- **AQ-4 (T4 — resolved):** Sidebar scroll container is the `ul` element at `apps/web/src/components/layout/presentational/shell-sidebar-chrome.tsx:181-182` (`className="flex flex-1 flex-col gap-4 overflow-auto py-1"`). It is NOT `window`. T4 attaches `scroll` listener to this `ul` element (via `scrollRef` prop or by querying the DOM element inside the chrome) and `resize` to `window`. The chrome already has `overflow-auto` so it scrolls independently of `window`.
- **AQ-5 (T5 — resolved):** `useLocation` **already imported**. `apps/web/src/components/layout/sidebar.tsx:3` — `import { NavLink, useLocation, useNavigate } from 'react-router-dom';`. `pathname` is already destructured at line 67: `const { pathname } = useLocation();`. T5 only adds a `useEffect` keyed on `pathname` that clears `renamingItem` → `null` + `renameValue` → `''` on change. No new import needed.

## Dependencies

- T1–T5 are independent of each other (disjoint files); order is the SDD per-task sequence, not a hard dependency.
- T6 is independent (admin JSON move); can run in parallel with T1–T5 if PM dispatches separately, but is small enough to be the last task in the SDD sequence.
- No upstream dependencies on P1 (P0 and P1 touch disjoint file trees).

## Risks

See compass `## Risk Register` rows 1–5.
