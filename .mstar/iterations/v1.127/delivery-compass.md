---
iteration_id: V1.127
start_date: 2026-07-20
status: locked
iteration_base_branch: main
target_branch: main
spec_integration_branch: iteration/v1.127
plans:
  - 2026-07-20-v1.127-p0-control-room-author-loop-fixes
  - 2026-07-20-v1.127-p1-native-agent-provider-registration
---

# V1.127 Delivery Compass — Dogfood-readiness sweep

> **Direction lock mode: autonomous** (`/iteration-loop`, scale **M** — 2 business plans).
> Caller direction: "上一轮类似的方向，修 bug 或者优化，或者 UI 的提升都可以。这个轮做完之后，我会开始测试做一些使用体验上的 review。你要做一些代码的调研，先尽可能地对代码进行一次扫描和预判。目标是确保不会产生更多影响人工测试体验的点。"
>
> **Phase 1 Review & Edit chain: COMPLETE — compass LOCKED.**
> - Seat 1 (product-manager): AC-V1127-7 discovery-flow semantics tightened; tester-visible notes sharpened on AC-1/AC-3; P0/P1 spec User value rewritten; NG-17 added (no new full-screen loading/error boundaries).
> - Seat 2 (architect): All 10 AQ-* resolved with file:line evidence; predictive scan claims verified; `ClaudeNativeProvider` → `ClaudeCliProvider` corrected everywhere; `client.createWorld` confirmed ABSENT (T1 ships disabled-with-tooltip only); agent endpoint corrected to `GET /v1/daemon/agent-host/providers`; `useTimelineOverview` already accepts `cursor?: string` (T2 UI-only); `ErrorState` component confirmed for T3 reuse; sidebar scroll container identified for T4; `useLocation` already imported for T5; P1 example code corrected to real `default_config()` + `.await` API; architecture locks section ratified in compass; dependency graph disjoint confirmed; `wire_contracts_changed: false` CONFIRMED for both plans.
> - Seat 3 (writing-specialist): DF tracker hygiene complete — added `DF-V1127-COMPOSITE-PERF` and `DF-V1127-NIT-CLOSEOUT` rows; notable in-flight residual note for `R-V1116P0QA-001`; quick status + Quick index synced to V1.127 active / V1.126 shipped; package README created.
>
> Direction is **locked** — do not re-question the scan-driven P0/P1 friction surface.

## Autonomous direction lock record

**Scale budget:** M = 2 business plans (harness process not counted). Caller gave "M ~ L"; PM locked at M (2 plans) to maximize fix quality per plan and minimize new test surface — the iteration's explicit goal is "no more manual-test friction points", so breadth would defeat the purpose.

**Caller direction mapping:**

| Caller phrase | Candidate coverage |
|---------------|--------------------|
| 上一轮类似的方向，修 bug 或者优化，或者 UI 的提升 | Same profile as V1.126 (bug-fix + optimization + UI cluster); P0/P1 candidates surfaced by predictive scan map directly to "bug + UI" |
| 你要做一些代码的调研，先尽可能地对代码进行一次扫描和预判 | Predictive scan completed via `explore` subagent — 3 P0 + 4 P1 + 3 P2 candidates ranked; top 6 selected for the 2 plans |
| 目标是确保不会产生更多影响人工测试体验的点 | Only items whose user-visible symptom a manual tester would actually hit are in scope; pure-scale perf and 24 V1.126 nit polish stay roadmap |
| 这个轮做完之后，我会开始测试 | Tester's first flow: app open → Setup → Worlds list → Create World → enter World → Timeline → Work → Outline → agent test; P0/P1 fixes map to this flow |

**Branch policy (autonomous resolve per `references/autonomous-direction-lock.md`):**

- `iteration_base_branch: main` — resolved from `status.json` root `metadata.iteration_base_branch` (consistent across V1.122 → V1.126).
- `target_branch: main` — resolved from `status.json` root `metadata.target_branch` (matches V1.122 PR #156 / V1.123 PR #157 / V1.125 PR #159 / V1.126 PR #160 documented project policy).
- `spec_integration_branch: iteration/v1.127` — new branch cut from `main`.

This is the documented project policy; not a silent `main` default.

### Predictive scan summary

PM dispatched `explore` (read-only) against V1.126 surfaces + cross-cutting Control Room flow + native agent host wiring. Top candidates by severity:

| # | Severity | Title | Type | Plan |
|---|----------|-------|------|------|
| 1 | P0 | Create World button is a no-op (`apps/web/src/pages/worlds-page.tsx:36`) | Bug | P0 |
| 2 | P0 | Cursor pagination UI missing — first 20 worlds only (`global-timeline-view.tsx:40`, `worlds-page.tsx:24`) | Missing feature | P0 |
| 3 | P0 | CodexNativeProvider / ClaudeCliProvider never registered in daemon HostManager (`crates/nexus-daemon-runtime/src/boot.rs:721`) | Bug | P1 |
| 4 | P1 | WorldsPage overview fetch has no error state — silent 5xx (`worlds-page.tsx:24-34`) | Missing error handling | P0 |
| 5 | P1 | Selection submenu: no resize listener — popover drifts on window resize/scroll (`selection-submenu.tsx:93-102`) | UX regression (R-V1126P0-QC-S-002) | P0 |
| 6 | P1 | Stale `renamingItem` on pathname change — rename input persists across navigation (`sidebar.tsx:75-77`) | State bug (R-V1126P0-QC-S-003) | P0 |
| 7 | P1 | `total_worlds` full table scan every request (`timeline.rs:118`) | Perf/overhead | **Roadmap** (not a manual-test friction) |
| 8 | P2 | Dynamic SQL prevents prepared statement caching (`timeline.rs:71`) | Perf | **Roadmap** |
| 9 | P2 | `handleOpenSubmenu` two-source identity (qc1 S-1) | Code smell (R-V1126P0-QC-S-001) | **Roadmap** |
| 10 | P2 | `NavItemLi` not memoized (`shell-sidebar-chrome.tsx:330`) | Perf (R-V1126P0-QC-S-006) | **Roadmap** |

**Out-of-scope (roadmap V1.128+):** items 7–10 + the 22 remaining V1.126 nit residuals (qc1 S-4 outside-click vs dialog portal, qc1 S-5 URL regex over typed workId, qc3 S-8 unused anchorEl dep, qc1 P1 S-1 extraction alias, qc1 P1 S-2 layer extensibility, qc2 P1 S-1 runtime shape validation, qc2 P1 S-2 Studio fixture divergence, qc3 P1 S-1 scale-stress, 9 P2 composite-endpoint nits, 3 P3 status-compaction nits). These are polish, not test-blockers.

**Verified shipped (residual to close):** `R-HOTFIX-SETUP-CONTINUE-404` — fix `e320e62d` ("fix: Setup Continue 404 on creator PATCH (#153)") is on `main` HEAD (verified via `git merge-base --is-ancestor e320e62d main`). The HIGH residual row is observably stale and is closed as a low-cost process task inside P0 (T6).

### Candidates evaluated

Research base: V1.126 ship artifacts (`delivery-compass.md` Roadmap Position + Retrospective "下迭代建议" explicitly recommends "investigate subagent dispatch reliability root cause" OR "pick from roadmap"), V1.126 open residuals (32 total: 1 high + 3 medium + 4 low + 24 nit), `explore` predictive scan results (10 candidates ranked P0→P2), `STRATEGY.md` Three Pillars alignment.

| # | Candidate | Trade-off | Verdict |
|---|-----------|-----------|---------|
| A | **Dogfood-readiness sweep — predictive-scan P0/P1 fixes** — directly answers caller's "scan + predict + ensure no more test-friction points". 2 plans: frontend Control Room author-loop fixes (P0) + native agent provider registration (P1). | Tightest fit to caller direction; smallest test surface; highest fix-quality per plan. | **LOCKED** — every clause of caller direction; every plan fixes an observable bug. |
| B | Subagent dispatch reliability investigation (V1.126 retrospective "下迭代建议") | Real V1.126 pain (5/22 empty-response fallbacks); but root cause is likely OpenCode host / context-length — not a code bug in this repo. Investigation would consume the whole iteration without shipping user-visible value. | Rejected (this iteration) — needs upstream investigation; not a Nexus code change. Stays as V1.127+ process note. |
| C | Fork UI (`DF-V1122-FORK-UI`) — V1.124 roadmap next | Highly ranked feature; but adds new test surface (the opposite of caller's goal). | Rejected (this iteration) — wrong direction fit ("ensure no MORE friction"). |
| D | P0 Delete follow-up (daemon DELETE routes for works/worlds) — V1.126 roadmap (j) | Closes R-V1126P0-T2-001; but introduces wire contract change (`wire_contracts_changed: true`) and new routes — adds test surface, not removes it. | Rejected (this iteration) — pick when Delete becomes top dogfood ask. |
| E | Composite endpoint perf + total_worlds cleanup (scan items 7-8) | Real perf wins; but manual tester with <100 worlds never sees the symptom. | Rejected (this iteration) — folded into roadmap V1.128+; not a manual-test friction. |
| F | V1.126 nit residual close-out (22 nits) | Real tech-debt pressure; but nits are polish, not test-blockers. Closing them en masse is process work, not new value. | Rejected (as standalone) — P0 absorbs the 2 nits that overlap with friction fixes (R-V1126P0-QC-S-002, R-V1126P0-QC-S-003); the rest stay roadmap. |
| G | i18n migration completion (R-P1-001 — ~25 hardcoded strings) | Real medium residual; but the strings are in secondary pages (works/schedule/sessions/strategies/capabilities) not on the primary tester flow. | Rejected (this iteration) — stays roadmap; pick when i18n becomes top dogfood ask or pre-localization-release gate. |

### Evidence base for A

- **Caller direction is scan-first ("先尽可能地对代码进行一次扫描和预判").** The `explore` predictive scan produced 10 ranked candidates with file:line evidence — 6 of them are observable bugs a manual tester hits in the first 30 minutes. This is the highest evidence rank per autonomous ranking heuristics (caller-constrained + product completeness + risk-avoidance).
- **P0-1 Create World no-op** (`apps/web/src/pages/worlds-page.tsx:36`): `handleCreateWorldClick` is an empty function. V1.125 shipped this stub; V1.126 P3 residual cleanup did not pick it up. A tester clicking "Create World" today gets nothing. **Highest dogfood friction.**
- **P0-2 Cursor pagination missing** (`global-timeline-view.tsx:40`, `worlds-page.tsx:24`): V1.126 P2 shipped the composite endpoint with cursor + `has_more` + `total_worlds`, but the web consumer never passes a cursor. A tester with >20 worlds (the maintainer almost certainly has this) sees only the first 20 with no "Load More". V1.126 P2 residual `R-V1126P2-QC-S-006` flagged this.
- **P0-3 Native providers not registered** (`crates/nexus-daemon-runtime/src/boot.rs:721`): `HostManager::new()` creates an empty manager; `register_provider` is never called. `CodexNativeProvider` + `ClaudeCliProvider` exist in the codebase and are path-scanned, but never wired into the daemon's HostManager. If the tester has `codex` or `claude` CLI installed locally (very likely — they're the maintainer), the AgentPicker will show no agents. V1.116 residual `R-V1116P0QA-001` is exactly this bug and has rolled forward 11 iterations.
- **P1-1 WorldsPage overview silent 5xx** (`worlds-page.tsx:24-34`): `overview.isError` never checked; on backend error the UI silently shows "No recent activity" for every world. V1.126 P2 residual `R-V1126P2-QC-S-008` flagged this.
- **P1-2 Selection submenu resize listener missing** (`selection-submenu.tsx:93-102`): popover position computed once at render; no `resize`/`scroll` listener. V1.126 P0 residual `R-V1126P0-QC-S-002` flagged this. A tester resizing the window mid-submenu sees popover drift.
- **P1-3 Stale renamingItem on pathname change** (`sidebar.tsx:75-77`): `renamingItem` state in `Sidebar` is independent of chrome's `submenuItem` and is never cleared on navigation. V1.126 P0 residual `R-V1126P0-QC-S-003` flagged this. A tester clicking Rename then navigating sees a stale rename input.
- **Setup-Continue hotfix verified shipped**: `e320e62d fix: Setup Continue 404 on creator PATCH (#153)` is on `main` HEAD (confirmed via `git merge-base --is-ancestor e320e62d main`). `R-HOTFIX-SETUP-CONTINUE-404` is observably stale and is closed inside P0 T6 as a 5-minute admin task.
- **STRATEGY alignment — Three Pillars:** Harness pillar (native agent provider registration in P1) — direct hit. Canvas pillar (Control Room fixes in P0 keep the Timeline-centric World entry surface usable). No new pillar invented; no scope drift into Computable.

### Locked direction (single sentence)

Pre-empt the manual-test friction points surfaced by the V1.127 predictive code scan: fix the **frontend Control Room author-loop bugs** a tester hits first — Create World no-op, cursor pagination for >20 worlds, silent overview error state, selection-submenu resize listener, stale renamingItem on navigation — in **P0**, and **wire the native agent providers (Codex + Claude) into the daemon HostManager** so agent discovery works during agent-flow testing in **P1**; plus close the observably-shipped Setup-Continue hotfix residual as a low-cost process task inside P0.

### Dependency graph (locked)

```
P0 (Control Room author-loop fixes)        ← Must; no upstream; frontend-only (apps/web)
   └── P1 (Native agent provider reg)      ← Must; independent (crates/nexus-daemon-runtime + nexus-agent-host)
```

P0 and P1 touch disjoint files (frontend pages/components vs Rust daemon boot + agent host). P0 → P1 serial order per `mstar-iteration` §2.6 per-plan loop; both Prepare in parallel.

## Scope

本迭代锁定的 spec 点（**dogfood-readiness sweep — predictive-scan-driven**）：

- **S1 (P0 — Must)**: Control Room author-loop dogfood fixes — wire the empty Create World handler (or surface graceful "browser-only" if `client.createWorld` is absent), add cursor pagination UI for >20 worlds on Global Timeline + Worlds page overview, surface an inline error state when `useTimelineOverview` fails on the Worlds page, add a resize/scroll listener that closes or repositions the selection submenu popover, and clear `renamingItem` on pathname change. **Pain today:** a manual tester clicking Create World gets nothing; a tester with >20 worlds sees only the first 20 with no Load More; a tester hitting a 5xx on overview sees silent fallback text; a tester resizing the window with a submenu open sees popover drift; a tester clicking Rename then navigating sees a stale rename input. Closes V1.126 P0/P2 residuals R-V1126P0-QC-S-002, R-V1126P0-QC-S-003, R-V1126P2-QC-S-006, R-V1126P2-QC-S-008 and the HIGH Setup-Continue hotfix residual R-HOTFIX-SETUP-CONTINUE-404 (verified shipped on main).
- **S2 (P1 — Must)**: Native agent provider registration in daemon HostManager — call `manager.register_provider(...)` for `CodexNativeProvider` and `ClaudeCliProvider` in `crates/nexus-daemon-runtime/src/boot.rs` before wiring the agent host subsystem facade; verify the path-scan → register → AgentPicker list → session-creation invocation loop works end-to-end (the **discovery flow** is the V1.127 user-visible fix; provider-internal session-create bugs are NG-13, not a P1 blocker). **Pain today:** the daemon's HostManager is constructed empty in `boot.rs`; `register_provider` is never called. A tester with `codex` or `claude` CLI installed locally sees an empty or stale agent list in the AgentPicker. **Relief after V1.127:** the AgentPicker shows discovered Codex / Claude agents, and creating a session invokes the provider's handshake — the V1.116 roll-forward is closed. Closes V1.116 residual `R-V1116P0QA-001` (rolled forward 11 iterations).

## Plans

| plan_id | Name | Status | Notes |
|---------|------|--------|-------|
| `2026-07-20-v1.127-p0-control-room-author-loop-fixes` | P0 — Control Room author-loop dogfood fixes | Todo | **Must** (plan). Tasks: T1 Create World wiring Must · T2 cursor pagination UI Must · T3 overview error state Must · T4 selection-submenu resize listener Must · T5 stale renamingItem clear Must · T6 (process) close R-HOTFIX-SETUP-CONTINUE-404 residual Must. Frontend-only. Closes 5 V1.126 residuals. |
| `2026-07-20-v1.127-p1-native-agent-provider-registration` | P1 — Native agent provider registration in daemon HostManager | Todo | **Must** (plan). Tasks: T1 register providers Must · T2 end-to-end verification Must. Backend Rust. Closes R-V1116P0QA-001 (11-iteration roll-forward). |

Status values: `Todo` | `InProgress` | `InReview` | `Done` | `Blocked`

## Milestones

| Milestone | Target date | Status |
|-----------|-------------|--------|
| Phase 1 compass locked | 2026-07-20 | in-progress (PM seat 1 + architect seat 2 + writing-specialist seat 3; PM lock pending) |
| P0 Control Room author-loop fixes Done | 2026-07-20 | pending |
| P1 Native agent provider registration Done | 2026-07-20 | pending |
| Iteration close + PR | 2026-07-21 | pending |

## Acceptance Criteria

Observable product criteria (each AC maps to exactly one plan or process gate; no orphans):

- **AC-V1127-1** (P0 → S1): On the Worlds page (`/worlds`), the empty-state "Create World" / "Start a new World" card is no longer a no-op. If the desktop bridge exposes `client.createWorld`, clicking the card creates a World (with confirmation) and navigates to the new World's Timeline. If the bridge does not expose `client.createWorld` (browser-only context), the card shows a graceful "Use the desktop app to create a World" tooltip or disabled-with-reason state instead of silently doing nothing. **Tester-visible:** clicking the "Create World" card on desktop navigates to the new World's Timeline; clicking it in a browser-only context shows a tooltip explaining why (e.g. "Open in the desktop app to create a World"); the silent no-op is gone.
- **AC-V1127-2** (P0 → S1): On the Global Timeline page (`/timeline`) and the Worlds page (`/worlds`) overview, when the backend returns `has_more: true` for the composite endpoint, a "Load More" control (or infinite scroll) fetches the next page using the cursor. A tester with >20 worlds sees all of them via repeated Load More. No regression on the ≤20 case. **Tester-visible:** all worlds are reachable from the Global Timeline + Worlds page.
- **AC-V1127-3** (P0 → S1): On the Worlds page, if `GET /v1/daemon/timeline/overview` fails (5xx or network error), an inline error banner shows "Couldn't load recent activity — retry" instead of silently rendering "No recent activity" for every world. The world list itself still renders from `useNarrativeWorlds`. **Tester-visible:** when the overview endpoint fails, the tester sees an inline "Couldn't load recent activity — Retry" banner above the world list (not the silent "No recent activity" text); clicking Retry re-fetches; the world list itself still renders from its separate query.
- **AC-V1127-4** (P0 → S1): With a selection submenu open on a sidebar row, resizing the window or scrolling the sidebar dismisses (or repositions) the popover so it does not drift away from the trigger row. **Tester-visible:** no orphaned floating popover after layout change.
- **AC-V1127-5** (P0 → S1): After clicking Rename on a sidebar row and then navigating via a different NavLink (without committing the rename), the stale rename input does not appear in the new view. **Tester-visible:** rename state is scoped to its row + route; navigating away clears it.
- **AC-V1127-6** (P0 → S1 process): `R-HOTFIX-SETUP-CONTINUE-404` (HIGH severity, `decision: fix_now`) is moved from `.mstar/status.json::residual_findings["2026-07-16-hotfix-setup-continue-404"]` to `.mstar/archived/residuals/2026-07-16-hotfix-setup-continue-404.json` with `closure_note: "Shipped — fix e320e62d on main HEAD (#153)"`. No code change. **PM-visible:** HIGH open residual count drops by 1.
- **AC-V1127-7** (P1 → S2): With `codex` or `claude` CLI installed locally, the daemon boot wires `CodexNativeProvider` and `ClaudeCliProvider` into the `HostManager` via `register_provider(...)` before the agent host subsystem facade is constructed. The AgentPicker dialog in the sidebar shows the discovered Codex / Claude agent, and attempting to create a session against a discovered agent invokes the provider's session-create flow. **Tester-visible:** with `codex` (or `claude`) installed locally, opening the AgentPicker shows the Codex (or Claude) agent in the list; selecting it and creating a session reaches the provider. **Scope note:** if the provider's *internal* session-create handshake has a latent bug, that becomes a V1.128+ plan candidate per NG-13 — the V1.127 user-visible fix is the **discovery flow**, not a guarantee that every provider-internal handshake is bug-free.
- **AC-V1127-8** (process): No new `{KNOWLEDGE_DIR}/` documents from Phase 1 Review chain. Knowledge crystallization deferred to Phase 3 `mstar-compound`.
- **AC-V1127-9** (process): Compass `status: locked` after PM lock; both plans registered in `status.json` with `spec_integration_branch: iteration/v1.127`.

**AC → plan map (no orphans):** AC-1..AC-6 → P0 · AC-7 → P1 · AC-8/9 → process.

## Non-Goals

Concrete exclusions (if a PR does any of these, it is out of V1.127 scope):

- **NG-1**: No Fork creation/merge UI (`DF-V1122-FORK-UI`). Stays roadmap; adds test surface (opposite of caller's goal).
- **NG-2**: No Computable pillar UI / compute-on-timeline (`DF-V1122-COMPUTABLE-UI`, `DF-V1122-COMPUTE-ON-TIMELINE`). Not a dogfood-friction fix.
- **NG-3**: No daemon `DELETE /v1/daemon/works/{id}` / `DELETE /v1/daemon/worlds/{id}` routes (V1.126 R-V1126P0-T2-001 follow-up). Wire contract change; adds test surface. Stays roadmap V1.128+ candidate.
- **NG-4**: No `total_worlds` query removal / caching (scan item 7). Pure-scale perf; manual tester with <100 worlds never sees the symptom. Stays roadmap.
- **NG-5**: No dynamic-SQL → static-query refactor in `timeline.rs` (scan item 8). Pure-scale perf. Stays roadmap.
- **NG-6**: No V1.126 nit residual close-out beyond the 2 absorbed by P0 (R-V1126P0-QC-S-002 resize, R-V1126P0-QC-S-003 stale state). Remaining 22 nits are polish, not test-blockers; stay roadmap.
- **NG-7**: No subagent dispatch reliability investigation (V1.126 retrospective recommendation). Root cause likely upstream (OpenCode host / context-length); not a Nexus code change. Stays as V1.127+ process note.
- **NG-8**: No i18n migration completion (R-P1-001 — ~25 hardcoded strings in secondary pages). Not on the primary tester flow. Stays roadmap.
- **NG-9**: No new World-scoped `GET /v1/daemon/worlds/{world_id}/timeline` route promotion (`DF-V1122-DEEPER-WB` remainder slice). V1.126 NG-6 stays in force.
- **NG-10**: No Moment-on-wire migration (`DF-V1123-MOMENT-WIRE`), no Work Brief layer (`DF-V1123-WORK-BRIEF`), no World Moment layer (`DF-V1123-WORLD-MOMENT`). Out of scope for a dogfood-readiness sweep.
- **NG-11**: No selection-submenu Delete action (depends on NG-3 routes). Stays deferred with R-V1126P0-T2-001.
- **NG-12**: No `AgentPicker` component refactor. P0 V1.126 NG-14 stays in force; P1 only registers providers, does not touch `AgentPicker` chrome.
- **NG-13**: No CodexNativeProvider / ClaudeCliProvider internal logic change (path-scan, session creation, etc.). P1 only wires them into the HostManager via `register_provider`; if the providers themselves have bugs, those become separate V1.128+ plans.
- **NG-14**: No `opencode.json` / `secrets.env` / global config changes.
- **NG-15**: No mobile / sidebar-below-`lg` UX work. V1.126 NG-15 stays in force.
- **NG-16**: No new schemas or wire contract changes. Both P0 and P1 are `wire_contracts_changed: false`.
- **NG-17**: No new full-screen loading skeletons and no new top-level React error boundaries. Loading states introduced by T2/T3 are local to controls (Load More button spinner while fetching the next page, Retry button disabled-while-refetching); error states are scoped to the overview sub-section only (per AC-V1127-3). The Worlds page root loading + error behavior is unchanged.

## Roadmap Position

- **Current iteration (V1.127)**: in-flight — Dogfood-readiness sweep. Pre-emptive P0/P1 fixes from predictive scan + Setup-Continue residual close. 2 business plans (M budget).
- **Next iteration (V1.128+) candidates** (pick after V1.127 dogfood): (a) subagent dispatch reliability investigation (V1.126 retrospective); (b) daemon DELETE routes for works/worlds (V1.126 R-V1126P0-T2-001 follow-up — flips wire_contracts_changed); (c) Fork UI (`DF-V1122-FORK-UI`); (d) Computable pillar UI (`DF-V1122-COMPUTABLE-UI`); (e) composite-endpoint perf round (`total_worlds` cleanup + dynamic-SQL refactor + N+1 assertion + sqlx prepared-statement caching — scan items 7–8 + V1.126 P2 residual cluster); (f) V1.126 nit polish close-out (22 nits); (g) i18n migration completion (R-P1-001); (h) full per-World `GET /v1/daemon/worlds/{world_id}/timeline` route (`DF-V1122-DEEPER-WB` remainder slice). **Trigger:** V1.127 shipped + user's manual testing review feedback.
- **最终目标**: Every Nexus surface expresses one coherent literary-computational design language and the basic author loop (Setup → Create → World → Timeline → Work → Outline → Agent) is bug-free for manual testing. V1.127 closes the highest-friction cluster surfaced by predictive scan and resets the test surface to "no known P0 bugs".

## Delivery Branch Policy

> Mirror of frontmatter; keep in sync with `.mstar/status.json` `metadata`.

| Field | Value |
|-------|-------|
| `iteration_base_branch` | `main` |
| `spec_integration_branch` | `iteration/v1.127` |
| `target_branch` | `main` |

## Risk Register

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| `client.createWorld` does not exist on any bridge (browser + desktop) | Medium | Medium | P0 T1 spec: if method absent everywhere, surface graceful "use desktop app" disabled state with tooltip — do NOT mock a no-op. T1 verifies bridge capability duck-typing before wiring. |
| Cursor pagination UI conflicts with V1.126 P2 has_more semantics | Low | Low | P0 T2 spec: reuses existing `has_more` + `cursor` fields from V1.126 P2 response. No schema change. |
| WorldsPage overview error banner collides with existing world-list error state | Low | Low | P0 T3 spec: banner is scoped to the overview sub-section only; world list keeps its own error handling. |
| Selection submenu resize-listener causes submenu to dismiss mid-drag (tester mid-click) | Medium | Low | P0 T4 spec: listener dismisses only on `scroll` and `resize` (not on mousemove). Submenu can be re-opened with one click. Acceptable UX. |
| Stale renamingItem fix breaks existing rename persistence flow | Low | Medium | P0 T5 spec: clear on pathname change ONLY — existing commit-on-Enter / commit-on-blur paths preserved. Test asserts rename still commits when user presses Enter before navigating. |
| Native provider registration breaks existing agent host boot order | Low | High | P1 T1 spec: register_provider call is inserted in boot.rs AFTER path scan, BEFORE subsystem facade construction. Existing `AgentHostSubsystem` invariant ("with providers already registered") is finally met. |
| CodexNativeProvider or ClaudeCliProvider has latent bugs that surface after registration | Medium | Medium | P1 T2 spec: end-to-end verification includes session creation against each registered provider. If session creation fails, the failure is recorded as a V1.128+ plan candidate (NG-13); registration itself still ships (the discovery flow is the user-visible fix). |
| Phase 1 Review chain diverges on scope | Low | Medium | Direction locked autonomous; specialists edit within § Scope + Plans. Out-of-scope ideas → roadmap only. |
| Subagent dispatch reliability (V1.124/V1.126 retrospective) — empty-response fallbacks | Medium | Medium | PM commits on behalf of implementers only when subagent returns empty; SDD per-task loop preserves reviewer isolation. |

## Architecture locks (architect seat 2)

Per-plan architecture decisions ratified at seat 2. Each verdict is a **locked handoff** — implementers treat these as non-negotiable architecture contracts. Where seat 1 flagged open questions (AQ-1–AQ-5 per spec), the architect verdict is final.

### P0 — Control Room author-loop dogfood fixes

| Decision | Verdict | Rationale |
|----------|---------|-----------|
| **`wire_contracts_changed`** | `false` — **CONFIRMED** | All six tasks are frontend-only (`apps/web/src/{pages,components}/`) or admin-only (T6 JSON move). No daemon changes, no schema changes, no IPC additions. |
| **AQ-1: `client.createWorld` presence** (T1) | **ABSENT** — ship disabled-with-tooltip only | `apps/desktop/src-tauri/src/lib.rs:983-1003` (no `create_world` command), `apps/web/src/lib/nexus/types.ts:141-454` (no `createWorld` on `NexusClient`), `apps/web/src/lib/nexus/create-world.ts:4-7` (`hasCreateWorldClient` always returns false). No desktop IPC wiring in this iteration (NG-16). Log V1.128+ roadmap entry. |
| **AQ-2: `useTimelineOverview` hook signature** (T2) | Already accepts `cursor?: string` | `apps/web/src/api/queries.ts:211` — signature is `export function useTimelineOverview(cursor?: string)`. T2 adds pagination accumulator + Load More UI only; no hook refactor needed. |
| **AQ-3: Error banner component** (T3) | Reuse `ErrorState` from `states.tsx:112` | Already imported at `worlds-page.tsx:9` and used for world-list errors at line 65. T3 scopes a second `ErrorState` to the overview sub-section. Not the `GlobalTimelineListChrome` pattern (list-chrome-specific). |
| **AQ-4: Sidebar scroll container** (T4) | Chrome `ul` with `overflow-auto` at `shell-sidebar-chrome.tsx:181-182` | `resize` listener on `window`; `scroll` listener on the `ul` element (NOT `window` — chrome is independently scrollable). Pass scroll container via `scrollContainerRef` prop to `SelectionSubmenu`. |
| **AQ-5: `useLocation` import** (T5) | Already imported at `sidebar.tsx:3` | `pathname` already extracted at line 67. T5 adds a `useEffect` clearing `renamingItem` + `renameValue` on `pathname` change. No new imports. |
| **T5 clear trigger** (T5) | Pathname change ONLY | Do NOT clear on route-parameter change (`:workId` swap). Existing commit-on-Enter/blur preserved. |
| **T6 `closure_note`** (T6) | `"Shipped — e320e62d on main (PR #153)"` | Fifth closure_note value extends V1.126 P3's four-value enum. Admin JSON move only. |

### P1 — Native agent provider registration

| Decision | Verdict | Rationale |
|----------|---------|-----------|
| **`wire_contracts_changed`** | `false` — **CONFIRMED** | Providers already exist; only boot wiring changes. No new IPC, schemas, or daemon routes. |
| **AQ-1: Provider constructors** (T1) | Both have `default_config()` | `CodexNativeProvider::default_config()` at `codex.rs:122`; `ClaudeCliProvider::default_config()` at `claude.rs:197`. Both return `Self` implementing `ProviderAdapter` (codex.rs:530, claude.rs:432). |
| **Correct type name** (T1) | `ClaudeCliProvider`, NOT `ClaudeNativeProvider` | Struct at `claude.rs:118`. All plan, spec, and compass references corrected. |
| **AQ-2: `register_provider` signature** (T1) | `Arc<dyn ProviderAdapter>` | `manager.rs:116` — `pub async fn register_provider(&self, adapter: Arc<dyn ProviderAdapter>)`. Both providers wrap with `Arc::new(...).await`. |
| **AQ-3: Duplicate registration** (T1) | **REPLACE** — `HashMap::insert` | `manager.rs:120` — silently replaces. Safe on re-boot; no panic, no skip. |
| **Missing-CLI resilience** (T1) | **Straight-line registration** — no wrapper needed | `default_config()` does NOT panic (codex.rs:544-579, claude.rs:446-481). Uses `which` crate; returns `HostError::provider_unavailable` at probe time. |
| **Registration order** (T1) | AFTER `HostManager::new()` (line 721), BEFORE `state.set_agent_host(...)` (line 724) | The facade is passed to `create_subsystems()` at line 731. `AgentHostSubsystem::new(host, ...)` docstring already states "with providers already registered" (agent_host.rs:44-45). |
| **`register_provider` async** (T1) | Must `.await` | `manager.rs:116` — `pub async fn`. Both calls must be awaited. |
| **AQ-5: Agent-list endpoint** (T2) | `GET /v1/daemon/agent-host/providers` | `api/mod.rs:45-47` — NOT `/v1/daemon/agents`. All plan references corrected. |
| **AQ-4: Test pattern** (T2) | No existing "boot daemon with stubbed CLI" integration test | `path_scan.rs:177-219` has `scan_custom_path()` for unit tests. T2 establishes boot-test pattern in `crates/nexus-daemon-runtime/tests/`. |
| **Session-create fallback** (T2) | Manual QA if stubbing infeasible | T2 ships path-scan + agent-list automated tests; session-create is the manual QA step. |

## Iteration package

> Sibling paths under `.mstar/iterations/v1.127/` — not in `specs/` or `knowledge/`. Promoted to knowledge at iteration-close via `mstar-compound`.

| Path | Kind | Status |
|------|------|--------|
| `README.md` | index | active |
| `specs/control-room-author-loop-fixes.md` | spec (P0) | PM draft — pending product-manager seat 1 + architect seat 2 + writing-specialist seat 3 |
| `specs/native-agent-provider-registration.md` | spec (P1) | PM draft — pending product-manager seat 1 + architect seat 2 + writing-specialist seat 3 |

Plans: `.mstar/plans/2026-07-20-v1.127-p0-control-room-author-loop-fixes.md` · `.mstar/plans/2026-07-20-v1.127-p1-native-agent-provider-registration.md`.

## Quality Gate Summary

> Filled at iteration-close. Human summary only; per-plan gate details stay in each main plan, and open residual SSOT stays in `.mstar/status.json`.

| plan_id | QC decision | QA gate | Residuals | Durable summary |
|---------|-------------|---------|-----------|-----------------|
| `2026-07-20-v1.127-p0-control-room-author-loop-fixes` | TBD | TBD | TBD | `{PLAN_DIR}/2026-07-20-v1.127-p0-control-room-author-loop-fixes.md` + SDD review bundle |
| `2026-07-20-v1.127-p1-native-agent-provider-registration` | TBD | TBD | TBD | `{PLAN_DIR}/2026-07-20-v1.127-p1-native-agent-provider-registration.md` + SDD review bundle |

## Compound Round Summary

> Filled at iteration-close.

## Iteration Retrospective (minimal)

> Filled at iteration-close.
