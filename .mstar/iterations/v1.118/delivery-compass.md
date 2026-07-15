---
iteration_id: V1.118
start_date: 2026-07-15
status: locked
iteration_base_branch: main
target_branch: main
spec_integration_branch: iteration/v1.118
plans:
  - 2026-07-15-v1.118-daemon-no-profile-boot
  - 2026-07-15-v1.118-creation-peer-groups
  - 2026-07-15-v1.118-canvas-work-shell
---

# V1.118 Delivery Compass — Daemon no-Profile + Creation Canvas shell

> **Phase 1:** product-manager §5.1 · architect §5.2 · writing-specialist §5.3 — all done (2026-07-15).
> **PM lock (§5.4):** `status: locked`. Prepare gates pass on all three plans (specify / clarify / plan). Spec freeze locked.

### PM lock notes (§5.4)

1. **P0 Must** — Daemon no-Profile boot is the clean-wipe launch gate; no silent Default Profile as sole fix.
2. **P1 Must before P2** — Creation peer groups merge before Canvas work shell (shared `sidebar.tsx`).
3. **HTTP 409 `uninitialized`** — Tier-2 Profile-gated routes use existing `NexusApiError::Uninitialized` (not 401).
4. **`wire_contracts_changed: false`** — all three plans (architect AD-P0).
5. **Residuals** — opportunistic overlapping V1.117 only; mediums deferred (`R-V192SEC-001`, `R-P1-001`, `R-V1116P0QA-001`).

## Product story

After V1.117, two coupling problems block the local-first author experience:

1. **Process × Profile coupling** — Wiping `~/.nexus42` still kills the desktop daemon with `No active creator`. V1.105 always-auto-starts the sidecar, so a clean home now fails *earlier and louder* than before. Profile creation belongs in setup, not in daemon process prerequisites.
2. **Navigation × Canvas coupling** — The Creation tab still uses a V1.117 "Creator" meta-group that mixes Outline, World KB, and Memory. Entering a work hides the two-tab shell and replaces it with a left-rail drill-in (`isDrillIn`), pushing Canvas to the margins instead of the center.

The coherent bet:

> **Daemon is a system process; Profile is a business gate. Creation navigation matches author mental models (Works / Worlds / Memories); entering a work makes Canvas the core product surface.**

| Who | Pain today | What they get when V1.118 is Done |
| --- | --- | --- |
| **Authors (clean install / wipe)** | Daemon gate fails; desktop stuck before setup | Daemon reaches **Running** on empty `~/.nexus42`; setup/Profiles create the Profile later |
| **Authors (browsing Creation)** | Mixed meta-group; Outline as a top-level peer | Creator tab: three **peer** groups — Works, Worlds, Memories — each wired to existing surfaces |
| **Authors (inside a work)** | Whole-left drill-in hides tabs; Canvas feels secondary | Canvas-first shell: **main** = Outline canvas (default); **right rail** = Works list + preview; Body read-only retained |

### Grill-me decisions (locked)

1. **Daemon × Profile:** Boots without active creator; Profile required only for business ops; init `~/.nexus42/` system dirs. **Not** silent auto-create-as-sole-fix.
2. **Creation list mode:** Peer hierarchy — Works / Worlds / Memories (no wrapping Creator canvas meta-group).
3. **Enter-work depth:** Ship peer groups **and** Canvas-first work shell this iteration.
4. **Worlds / Memories depth:** Nav + wire existing `/worlds` and `/memory` (SOUL etc.); no new editor/authoring flows.
5. **Residuals:** Opportunistic overlapping V1.117 items only; mediums deferred; no dedicated residual plan.
6. **Branch policy:** `main` → `iteration/v1.118` → `main`.

## Scope slices (non-overlapping)

| Slice | Plan | Route / surface boundary | Ships alone? |
| --- | --- | --- | --- |
| **P0** | daemon-no-profile-boot | Daemon boot, health, creators/setup APIs, Profile-gated business routes | Yes — unblocks clean desktop launch |
| **P1** | creation-peer-groups | Creator tab **list mode** only (`/works`, `/worlds`, `/memory`, no `:workId` drill-in UX change) | Yes — IA improvement without work shell |
| **P2** | canvas-work-shell | `/works/:workId/*` work context only | No — depends on P1 peer Works model |

**Overlap guard:** P1 must not implement right-rail work shell or retire `isDrillIn`. P2 must not redefine list-mode peer groups. P0 must not change sidebar IA.

## Plans

| plan_id | Name | Status | Tier | Notes |
|---------|------|--------|------|-------|
| 2026-07-15-v1.118-daemon-no-profile-boot | Daemon no-Profile boot | InProgress | Must / P0 | T1 committed (e864698e); reviewer found 2 Critical + 1 Important — fix wave pending |
| 2026-07-15-v1.118-creation-peer-groups | Creation peer groups | Todo | Must / P1 | List-mode IA; parallel-safe with P0 |
| 2026-07-15-v1.118-canvas-work-shell | Canvas-first work shell | Todo | Must / P2 | Depends on P1 |

### Plan dependencies (implement order)

| Plan | Depends on | Rationale |
| --- | --- | --- |
| P0 Daemon boot | — | Empty home must reach healthy daemon before any UI work matters |
| P1 Peer groups | — | May land in parallel with P0; no runtime dependency |
| P2 Canvas shell | P1 | Right-rail Works list reuses peer Works mental model and data paths |

## Milestones

| Milestone | Target date | Status |
|-----------|-------------|--------|
| Product specify + clarify (§5.1) | 2026-07-15 | **done** (`@product-manager`) |
| Architect plan lock (§5.2) | 2026-07-15 | **done** (`@architect`) |
| Writing Review & Edit (§5.3) | 2026-07-15 | **done** (`@writing-specialist`) |
| Spec freeze (iteration package) | 2026-07-15 | locked (§5.4 PM) |
| Dev complete | TBD | pending |
| QC complete | TBD | pending |
| Iteration close | TBD | pending |

## Acceptance Criteria (iteration-level)

Author-observable unless noted as operator/API. Detail IDs live in iteration `specs/`.

| ID | Criterion | Verification |
| --- | --- | --- |
| IC-1 | After wiping `~/.nexus42`, desktop/daemon reaches **Running** without fatal `No active creator in ~/.nexus42/config.toml` | Delete `~/.nexus42`; launch desktop or `nexus42 daemon start`; daemon status indicator / health probe succeeds |
| IC-2 | Health/ready and creators/setup APIs succeed with **no** `active_creator_id` | `GET /v1/daemon/runtime/health` OK; list/create/set-active creator paths work before Profile selected |
| IC-3 | Profile-scoped routes (works, memory, worlds, schedules) return **HTTP 409** `uninitialized` until Profile exists and `state.db` is attached | API calls without active creator → structured 409 body, not process crash |
| IC-4 | After Profile selected, existing happy-path flows unchanged | Smoke: create work, open outline, open memory |
| IC-5 | Creator tab (list mode): peer groups **Works**, **Worlds**, **Memories** | Visual + `shell.json` keys; no "Creator" meta-group mixing canvas surfaces |
| IC-6 | Worlds → existing `/worlds`; Memories → existing `/memory` / SOUL surfaces | Click peer group entries; routes match today’s pages |
| IC-7 | Entering a work → Canvas-first shell: main Outline canvas (default); right Works list + preview | Navigate to `/works/:id/outline`; tabs not replaced by drill-in skeleton |
| IC-8 | Body read-only reachable from work shell via chapters routes | `/works/:id/chapters` loads inside shell |
| IC-9 | V1.117 whole-left `isDrillIn` is **not** the primary enter-work UX | Inside work: Creator/Orchestrator tabs remain; no Back/Outline/Body-only left nav takeover |
| IC-10 | Overlapping V1.117 residuals closed when touched; `R-V192SEC-001`, `R-P1-001`, `R-V1116P0QA-001` stay deferred | QC ledger |

## Non-Goals

- Platform / remote auth redesign
- New Memories editor or world authoring flows
- Orchestration-tab IA redesign (Strategy stays under Orchestrator unless shell chrome forces layout touch)
- Dedicated residual slate-clear or medium closures (TOFU, i18n gaps, CodexNative HostManager)
- Changing wire contracts unless architect proves P0 needs an **additive** endpoint (prefer reuse existing creators APIs)
- Removing `ensureSetupBootstrap` IPC — it may still run on wizard Continue; P0 changes **daemon** requirements, not necessarily wizard step order

## Roadmap Position

- **Current iteration (V1.118):** Daemon no-Profile boot + Creation peer IA + Canvas-first work shell
- **Next iteration:** Deeper Memories IA / World authoring UX / medium residual paydown (TOFU, i18n gaps, CodexNative HostManager) — **trigger:** V1.118 shipped; **owner:** PM
- **North star:** Desktop first-run and Creation surfaces feel local-first and Canvas-core without Profile/process coupling

## Delivery Branch Policy

| Field | Value |
|-------|-------|
| `iteration_base_branch` | `main` |
| `spec_integration_branch` | `iteration/v1.118` |
| `target_branch` | `main` |

## Architect decisions (locked — §5.2)

Technical approach locked by `@architect` (2026-07-15). Implementers MUST follow iteration `specs/` + plan Prepare Package (Architecture); durable stubs in `.mstar/specs/{daemon-runtime,desktop-shell,web-ui}.md`.

### AD-P0 — Daemon lazy-open + error semantics

| Decision | Choice | Rationale |
| --- | --- | --- |
| Boot without Profile | `WorkspaceState::new` initializes `~/.nexus42` system dirs + config skeleton; **does not** open creator `state.db` when `active_creator_id` is absent | Today `resolve_state_db_path` fatals at boot — root cause of IC-1 |
| Lazy-open trigger | First successful `set_active_creator` **or** first Profile-scoped handler after config already has `active_creator_id` | Single attach point; idempotent pool open |
| Pool holder | `Option<DbPool>` on `WorkspaceState` + `ensure_creator_pool()` helper (name implementer-stable) | Avoid fake DB path; tests use `new_for_testing` unchanged |
| Route tiers | **T0** health/status (no auth): always OK. **T1** creators/setup (config + FS): no `state.db`. **T2** business data: require active creator + open pool | Matches product Daemon vs Profile split |
| Uninitialized signal | **Reuse** `NexusApiError::Uninitialized` → HTTP **409**, wire `error.code: "uninitialized"` | Already emitted across handlers; no schema change |
| `wire_contracts_changed` | **`false`** for P0 | Behavior-only; existing `ErrorResponse` sufficient |

`GET /v1/daemon/creators/active` with no selection: **404 `not_found`** (not 409) — distinct from “business DB not ready”; clients treat as “no Profile selected.”

### AD-P1 — Creation peer groups (list mode)

| Decision | Choice | Rationale |
| --- | --- | --- |
| Works group shape | **“All Works”** link (`/works`) then **flat** work rows (same `useWorks` query, `limit: 12`) linking to `/works/:id` (P1) → `/works/:id/outline` (P2 default) | Product AC-P1-5; avoids nested collapse chrome |
| Worlds / Memories | Single nav link each under peer group heading — no canvas resolver items in Creation tab | Grill-me #4 |
| Highlight on `/works/:workId/*` before P2 | Keep V1.117 drill-in highlight rules **until P2 merges** | Overlap guard; P1 must not retire `isDrillIn` |
| `sidebar.tsx` ownership | **P1 merges first**; P1 T1 owns `creatorGroups` only | Deconflict with P2 |

### AD-P2 — Canvas-first work shell

| Decision | Choice | Rationale |
| --- | --- | --- |
| Layout module | New `apps/web/src/components/layout/work-shell-layout.tsx` + `work-rail.tsx` (presentational split optional) | Keeps `RootLayout` scroll SSOT; work chrome nested in `<main>` |
| Route wiring | Nested `<Route element={<WorkShellLayout />}>` under `works/:workId` for `outline`, `chapters`, `chapters/:chapter`; default `/works/:workId` → redirect `outline` | AC-P2-1/2 |
| Main column | Outline canvas outlet; `max-w` constraint **lifted** inside work shell (canvas needs width) | AD-P2-2 vs RootLayout 1200px cap |
| Right rail (lg+) | Fixed **280px** rail: scrollable Works list + **metadata preview** (title, status, `work_profile`, `updated_at`) — **no** manuscript snippet | MVP scope lock |
| Responsive (`<lg`) | Rail → **end-sheet drawer** toggled from work-shell header; main full width | Graceful degrade without drill-in fallback |
| `isDrillIn` / `drillInItems` | **Remove primary usage** in P2 T3; delete `drillInItems` construction + prop pass; keep `ShellSidebarChrome` prop **one release** with `@deprecated` JSDoc if removal is noisy | AC-P2-6; not “dead code until later” |
| Sidebar inside work | Creator \| Orchestrator tabs **stay visible**; P1 peer groups shown on Creator tab | AC-P2-5 |

### Cross-plan merge order

`P0 ‖ P1` → **P1 merge before P2 branch rebase**. P2 T3 touches same `sidebar.tsx` lines as P1 — rebase P2 on integration after P1 lands.

## Risk Register

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| Deferring `state.db` open breaks subsystems that assume DB at boot | Med | High | Architect: explicit lazy-open contract + regression tests for health-without-creator |
| V1.100/V1.105 bootstrap docs conflict with no-Profile model | Med | Med | Iteration spec + §13.11 desktop-shell stub; fold into Master at P5 |
| Canvas-first shell XL scope slips | Med | Med | P1 shippable alone; P2 AC scoped to Outline default + right rail + drill-in retirement |
| Dual layout (list vs work shell) regresses Orchestration | Low | Med | Orchestration tab unchanged; work shell scoped to `/works/:workId/*` |
| P1/P2 sidebar edits collide on same file | Med | Low | P1 lands first; P2 task T1 owns work-route layout only |

## Iteration package

| Path | Purpose |
|------|---------|
| `guides/` | Process / explore notes |
| `specs/` | Iteration-scoped normative drafts (P0–P2) |
| `README.md` | Package index |

## Quality Gate Summary

> Filled at iteration-close.

| plan_id | QC decision | QA gate | Residuals | Durable summary |
|---------|-------------|---------|-----------|-----------------|
| 2026-07-15-v1.118-daemon-no-profile-boot | InProgress | mandatory | — | T1 SDD review: Needs fixes — 2 Critical (boot.rs pool_or_uninit crash on no-creator; set_active_creator doesn't call ensure_creator_pool) + 1 Important (list creators returns 409 instead of empty). Fix wave pending. |
| 2026-07-15-v1.118-creation-peer-groups | N/A | mandatory | — | — |
| 2026-07-15-v1.118-canvas-work-shell | N/A | mandatory | — | — |

## Compound Round Summary

> Filled at iteration-close.

## Iteration Retrospective (minimal)

> Filled at iteration-close.
