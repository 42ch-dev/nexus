---
iteration_id: V1.135
start_date: 2026-07-23
end_date: 2026-07-23
status: completed
iteration_base_branch: main
target_branch: main
spec_integration_branch: iteration/v1.135
direction_lock_mode: autonomous
scale: M
plans:
  - 2026-07-23-v1.135-p0-sidebar-menu-create-ia
  - 2026-07-23-v1.135-p1-dock-icon-squircle-rca
---

# V1.135 Delivery Compass

## Scope

Fix two **author-visible dogfood regressions** that V1.134 claimed to address but did **not** match author intent:

1. **【图1】Sidebar menu-area create** — The **entire left shell menu / sidebar functional zone** (above 创作|编排 + 工作区 footer) must host create UX. V1.134 P3 incorrectly put create in **content dual-pane left** and **hid** `CreatorCreatePanel` on hub routes (`isCreatorHubSurface` → empty `panelContent`). That is wrong.
2. **【图2】Dock icon still a flat square** — After ~6 iterations including V1.134 opaque full-bleed compose, Dock still shows a sharp square `nexus-desktop` tile. Opaque PNG alone is insufficient; this iteration must find and fix the **real** pipeline / bundle / cache cause until the Dock shows a macOS squircle.

**Scale:** User asked「无限大，直到问题解决」. Autonomous lock interprets that as **fix-until-done** on these two defects — **not** unbounded plan count. **Business plan budget = M (2 Must plans)** — one per defect. Plans stay open until author-observable AC pass (or Blocked with verified next candidate exhausted).

### Terminology lock (anti-misread)

| Term | Means | Does **not** mean |
|------|-------|-------------------|
| **Sidebar / 左边菜单 / 侧边栏功能区 / sidebar menu slot** | `Sidebar` → `ShellSidebarChrome` `panelContent` — persistent left shell column | Content dual-pane left column inside `CreatorHubDualPane` |
| **Content / 内容区** | Main hub browsing: tabs + cards / empty | Create form column |
| **Dock Done** | Author eyeball on **live macOS Dock** after cache-invalid ritual | Studio VI-004, PNG opacity, or preview PNG alone |

## Autonomous direction lock

| Field | Value |
|-------|-------|
| **Mode** | `autonomous` (`/iteration-loop`; no grill-me) |
| **User direction** | Prior iteration still wrong on (1) create placement = original menu/sidebar zone not content-left; (2) Dock icon still unresolved after 6 iterations |
| **Chosen direction** | Correct Creator Hub IA placement + deep Dock icon RCA/fix |
| **Rationale** | Code: `sidebar.tsx` L134–135 hides create on `/works`/`/worlds`; `CreatorHubPage` mounts content dual-pane create. Author screenshot: empty sidebar + create in content. Dock: V1.134 P1 `DONE_WITH_CONCERNS` — `R-V1134P1-001` never closed; author reconfirms still square |
| **Rejected alternatives** | Re-litigate V1.134 dual-pane-as-sidebar; cosmetic Studio-only icon preview without Dock path; adding unrelated dogfood nits; a third business plan |

## V1.134 residuals carried into V1.135

| Residual | Plan | Close when |
|----------|------|------------|
| `R-V1134P3-001` | V1.134 P3 | P0 author AC: sidebar create + **no** content dual-pane create |
| `R-V1134P1-001` | V1.134 P1 | P1 author Dock squircle confirm (`P1G-1`) |
| `R-V1134P1-002` | V1.134 P1 | Supporting only — does not replace Dock gate |

## Plans

| plan_id | Name | Status | Notes |
|---------|------|--------|-------|
| `2026-07-23-v1.135-p0-sidebar-menu-create-ia` | Sidebar menu-area create IA | Done | Must — corrects V1.134 P3 misplacement (`R-V1134P3-001`) |
| `2026-07-23-v1.135-p1-dock-icon-squircle-rca` | Dock icon macOS squircle RCA | Done | Must — deep pipeline beyond opaque PNG (`R-V1134P1-001`) |

Status values: `Todo` | `InProgress` | `InReview` | `Done` | `Blocked`

## Milestones

| Milestone | Target date | Status |
|-----------|-------------|--------|
| Spec freeze | 2026-07-23 | **done** (PM §5.1 + Architect §5.2 normative contracts) |
| Dev complete | 2026-07-24 | pending |
| QC complete | 2026-07-24 | pending |
| Iteration close | 2026-07-24 | pending |

## Acceptance Criteria

### Iteration-level (author observable)

- **AC-I1:** On 创作 hub (`/works`, `/worlds`), create UX lives in the **left sidebar menu slot** (`ShellSidebarChrome` `panelContent` / `CreatorCreatePanel`). Content area is **list + empty state only** — **no** content-left create column.
- **AC-I2:** Sidebar is **not** a large empty white region on hub while create sits in content (inverse of 【图1】).
- **AC-I3:** Dock tile for the running desktop app shows **macOS squircle rounding** (not a sharp square), after documented rebuild + cache invalidation — **author visual confirm** required (`R-V1134P1-001` closes here).
- **AC-I4:** Iteration specs (`specs/p0-*`, `specs/p1-*`) + knowledge correction (P0 Task 4) state clearly: sidebar owns hub create; content dual-pane left-create was V1.134 misread; Dock Done ≠ Studio/PNG alone.

### Plan mapping (iteration AC → spec gates)

| Iteration AC | P0 spec (PAC) | P1 spec (P1G) | P0 plan AC | P1 plan AC |
|--------------|---------------|---------------|------------|------------|
| AC-I1 | PAC-1 | — | AC-1 | — |
| AC-I2 | PAC-2, PAC-3 | — | AC-2, AC-3 | — |
| AC-I3 | — | P1G-1, P1G-5 | — | AC-4 |
| AC-I4 | Anti-patterns + residual table | Anti-patterns + verify gate | AC-7 (knowledge) | AC-5, AC-6 |

## Non-Goals

- New daemon create APIs / wire contract bumps
- Orchestrator IA redesign; canvas feature work
- AgentPicker / desktop-startup-500 follow-ups (shipped V1.134)
- Treating Studio preview or PNG opacity alone as Dock-done
- Infinite additional business plans beyond the two Must defects
- Preserving V1.134 content dual-pane left-create as a compatibility branch

## Roadmap Position

- **Current iteration（V1.135）— delivered:** Sidebar menu-area create IA + Dock icon baked-squircle compose / RCA (author Dock confirm residual open)
- **Next iteration:** Author closes `R-V1135P0-001` / `R-V1135P1-001` visual gates; then resume Control Room polish / below-`lg` create affordance (`R-V1135P0-003`) / VI-004 wording (`R-V1135P1-003`)
- **最终目标：** Author-visible dogfood matches intent — create in shell sidebar; Dock looks like a normal macOS app

## Delivery Branch Policy

| Field | Value |
|-------|-------|
| `iteration_base_branch` | `main` |
| `spec_integration_branch` | `iteration/v1.135` |
| `target_branch` | `main` |

**Branch resolve:** `status.json` metadata already had `iteration_base_branch` + `target_branch` = `main` from V1.134 closeout; autonomous resolve adopts that (no silent invent).

## Ownership matrix (architect-locked)

| Layer | P0 owner | P1 owner |
|-------|----------|----------|
| **Product AC** | PM PAC-1–5 in `specs/p0-sidebar-menu-create-ia.md` | PM P1G-1–5 in `specs/p1-dock-icon-pipeline.md` |
| **Normative contract** | Architect spec § component/file/testid contracts | Architect spec § pipeline H1–H7 + stage map |
| **App implementation** | `sidebar.tsx`, `creator-hub-*`, i18n, tests | `compose-app-icon.mjs`, `icons:generate`, `tauri.conf.json`, README |
| **Visual proof** | Studio fixture (sidebar menu slot create + content browse) before app claim | Author macOS Dock squircle confirm in `guides/p1-dock-icon-rca.md` |
| **Durable docs** | P0 Task 4 → `workspace-parent-shell-ia.md` | P1 Task 3 → `icons/README.md`, `apps/desktop/AGENTS.md` |
| **Knowledge forbidden this turn** | — | Architect must not edit `{KNOWLEDGE_DIR}` |

## Key interfaces (cross-plan)

### P0 — Shell IA

| Interface | Contract |
|-----------|----------|
| `ShellSidebarChrome.panelContent` | `CreatorCreatePanel` when `activeTab === 'creator'` on **all** creator routes including `/works`, `/worlds` |
| `CreatorCreatePanel` | Reuse `CreatorShellContent` `mode="create"` + dialogs; `data-testid="sidebar-create-panel"` |
| Hub content | `HubTabBar` + `HubCardListPane` only — **no** `HubWorkspacePane` on hub |
| Delete | `isCreatorHubSurface` panel suppression; content-left inline create (`forceExpandedCreate`, `onCreateSubmit`) |
| Routes | Canvas (`/works/:id/*`, `/worlds/:id/*`) orthogonal; `wire_contracts_changed: false` |

### P1 — Icon pipeline

| Stage | Entry → output |
|-------|----------------|
| Compose | `compose-app-icon.mjs` → `source-1024.png` (opaque full-bleed) |
| Generate | `icons:generate` → `icon.icns` + `bundle.icon` PNGs |
| Bundle | `Nexus.app` → `CFBundleIconFile: Nexus.icns` |
| Verify gate | Author Dock squircle after quit → rebuild → `killall Dock` → relaunch |

## Technical risk register

| Risk | Likelihood | Impact | Mitigation | Owner |
|------|------------|--------|------------|-------|
| Agents re-interpret “功能区” as content-left again | Med | High | Normative spec terminology + testid contract (`sidebar-create-panel` present; `*-workspace-pane-inline-form` absent); PAC anti-patterns | P0 implementer |
| Partial P0 fix (sidebar restored but dual-pane create kept) | Med | High | Architect forbids dual implementation; delete content-left create branch | P0 QC |
| `HubDualPaneChrome` refactor scope creep | Med | Med | Allow browse-only path or `HubBrowseChrome` extract; do not redesign orchestrator shell | P0 implementer |
| Studio fixtures still prove V1.134 dual-pane create | High | High | Replace `creator-hub-dual-pane-ia-fixtures` matrix; align `gongnengqu-ia` creator-hub frame | P0 Task 2 |
| Dock still square after another opaque-only “fix” | High | High | Ordered H1–H7 RCA; P1G-4 written primary cause; no Done without P1G-1 | P1 implementer |
| Author tests wrong `.app` (stale duplicate bundle) | Med | High | P1G-2 identity checks; document exact build command in RCA | P1 + `@author` |
| `tauri dev` vs release bundle icon mismatch | Med | Med | H7 in hypothesis list; compare `target/debug` vs `target/release` Resources | P1 implementer |
| Author unavailable for Dock eyeball | Med | High | Document ritual; keep `R-V1134P1-001` open — do not fake Done | PM / `@author` |
| QC approves V1.134 P3 dual-pane as “correct IA” | Med | High | Explicit supersede of `R-V1134P3-001`; reject content-left create in AC | QC |
| Knowledge drift (workspace-parent-shell-ia still says hide sidebar create) | High | Med | P0 Task 4 mandatory before iteration close | P0 Task 4 |

## Iteration package

| Path | Purpose |
|------|---------|
| `specs/p0-sidebar-menu-create-ia.md` | P0 normative IA contract (PAC-1–5 + architect sign-off) |
| `specs/p1-dock-icon-pipeline.md` | P1 normative pipeline + verify gate (P1G-1–5 + H1–H7) |
| `guides/p1-dock-icon-rca.md` | P1 implementer RCA + author confirm block (Task 1 deliverable) |
| `README.md` | Package index |

## Quality Gate Summary

| plan_id | QC decision | QA gate | Residuals | Durable summary |
|---------|-------------|---------|-----------|-----------------|
| `2026-07-23-v1.135-p0-sidebar-menu-create-ia` | Approve with residuals | Pass with residuals | R-V1135P0-001..005 | plan `## Review Gate Summary` / `## QA Gate Summary` |
| `2026-07-23-v1.135-p1-dock-icon-squircle-rca` | Approve with residuals | Pass with residuals | R-V1135P1-001..005 | plan `## Review Gate Summary` / `## QA Gate Summary` |

**Iteration AC notes:** AC-I1/I2/I4 met in code. AC-I3 (author Dock squircle) **explicitly deferred** to `R-V1135P1-001` / `@author` — not forged Done.

## Compound Round Summary

| Candidate | Action | Notes |
|-----------|--------|-------|
| `workspace-parent-shell-ia.md` | **Updated in P0 Task 4** | Sidebar `panelContent` create; V1.134 dual-pane create superseded |
| `nexus-brand-token-hierarchy.md` | **Updated at close** | V1.135 H1+H6+H7 Dock compose rules; closes knowledge contradiction vs baked squircle |
| `guides/p1-dock-icon-rca.md` | **Retain in iteration package** | Author ritual + H1–H7 evidence; not promoted wholesale (pipeline SSOT = iteration spec + AGENTS/README) |
| `specs/p0-*`, `specs/p1-*` | **Retain in iteration package** | Normative for this ship; durable patterns already in knowledge |

No new knowledge slug created. README index rows refreshed for both updated patterns.

## Iteration Retrospective (minimal)

- **What worked:** Parallel lease-gated P0/P1; clear terminology lock stopped content-left misread; deep RCA beyond opacity.
- **What failed before:** V1.134 treated dual-pane content create + opaque PNG as Done without matching author intent / Dock proof.
- **Carry forward:** Author visual gates stay open residuals; do not close dogfood UI on Studio/metadata alone.
