---
iteration_id: V1.136
start_date: 2026-07-23
status: locked
iteration_base_branch: main
target_branch: main
spec_integration_branch: iteration/v1.136
direction_lock_mode: interactive
scale: M
plans:
  - 2026-07-23-v1.136-p0-dock-icon-squircle-followup
  - 2026-07-23-v1.136-p1-sidebar-inline-create-tabs
  - 2026-07-23-v1.136-p2-light-mode-chrome-harmony
---

# V1.136 Delivery Compass

## Scope

Close three **author-visible dogfood** defects that V1.135 shipped code for but did **not** fully resolve for author eyeball or UX intent:

1. **【图1】Dock icon** — `nexus-desktop` still reads as a **sharp full square** in the macOS Dock. V1.135 H6 baked-squircle compose may be invisible (same `#0D2B3E` plate/margin) or wrong binary/cache (H4/H5). **Fix until author sees native squircle** — or keep residual open with next named candidate (no fake Done).
2. **【图2】/【图3】Sidebar create IA** — V1.135 correctly moved create into the **sidebar 功能区**, but the zone is still two dashed **Create cards** that open dialogs. Author intent: an **inline create functional zone** with **local World | Work tabs** + title field + submit — **not** card buttons. Content stays browse-only (tabs + cards/empty; empty copy points at sidebar).
3. **Light Mode interactive VI** — Neon cyan (`#25D1E0`) on light feels **突兀** (Dark-only for primary signal). Light primary `brand-deep-blue` (#0D2B3E) on fills feels too **办公**. Retune light **interactive** chrome to **`brand-cyan-1000` (`#117480`)** + white on fills. Ink `brand-deep-blue` stays for titlebar / logo / text links. `TransportErrorBlock` CTAs → **compact text links** (not filled buttons). **Studio Tokens / Brand / Components / Surfaces** are author-facing SSOT — change once, reuse everywhere; no parallel product chrome.

**Scale:** **M** — 3 Must business plans (P0 Dock, P1 sidebar IA, P2 light VI).

**User value:** Author dogfood matches intent — native Dock tile, fast inline create without card→dialog hop, harmonious Light Mode that reads Chronos teal instead of office ink or neon cyan.

### HARD — Studio as author SSOT; reuse over reimplementation

| Studio gallery | Backing SSOT |
|----------------|--------------|
| Tokens | `DESIGN.md` / `DESIGN.dark.md` → `tooling/design-tokens` |
| Brand | `@42ch/nexus-ui` brand primitives + `theme.css` |
| Components | Promoted `@42ch/nexus-ui` primitives |
| Surfaces | Presentational extracts / fixtures on same tokens + primitives |

Implementer rules: change once at SSOT; reuse before invent; drift is a defect; Studio proves it before App Done.

### Terminology lock (anti-misread)

| Term | Means | Does **not** mean |
|------|-------|-------------------|
| **Sidebar / 功能区 / sidebar menu slot** | `ShellSidebarChrome.panelContent` — persistent left shell column | Content dual-pane left column; Open Design product UI |
| **Inline create zone** | Local **World \| Work tabs** + title input + submit **inside** sidebar panel | Two dashed `CreateCardButton` cards; dialog as the only visible create affordance |
| **Content / 内容区** | Hub browse: tab bar + card grid or empty state | Create form column; create cards in content |
| **OD 【图3】** | IA reference only (tab + inline form layout inspiration) | Scope to implement Open Design product UI |
| **Light interactive** | `brand-cyan-1000` fills, selection borders, active chrome, spinners, light canvas accents | Neon cyan `#25D1E0`; office deep-blue as primary fill |
| **Ink / structure** | `brand-deep-blue` titlebar, logo, text links | Primary Button fill in Light Mode |
| **Dock Done** | Author eyeball on **live macOS Dock** after documented rebuild ritual | Studio VI preview, PNG opacity, or icns file inspection alone |

## Direction lock

| Field | Value |
|-------|-------|
| **Mode** | `interactive` (Cursor Plan feedback closed → Build) |
| **Chosen direction** | Dock follow-up + sidebar inline create tabs + light interactive cyan-1000 + TransportError link CTAs |
| **Rationale** | Author screenshots + Studio Components feedback; V1.135 left Dock/author residuals and create-card IA unresolved |
| **Rejected** | Neon cyan as light primary; office deep-blue as light interactive fill; content dual-pane create; Open Design product UI as scope; keeping CreateCardButton hub UX |

## V1.135 residuals carried into V1.136

| Residual | Plan | Close when |
|----------|------|------------|
| `R-V1135P1-001` | P0 | Author Dock squircle confirm (`P0G-1`) |
| `R-V1135P1-005` | P0 | H6 visual subtlety resolved or next candidate named with author eyeball |
| `R-V1135P0-001` | P1 | Author visual confirm inline-tab create IA (supersedes card-button pattern for `R-V1134P3-001` closure) |
| `R-V1134P3-001` | P1 | PAC-1–5 pass with **inline** sidebar create (not card buttons) |

**Deferred (not V1.136 scope):** `R-V1135P0-003` (below-lg create), `R-V1135P1-002`–`004` (token hierarchy / VI wording / ritual prose).

## Plans

| plan_id | Name | Priority | Wave | Status | Notes |
|---------|------|----------|------|--------|-------|
| `2026-07-23-v1.136-p0-dock-icon-squircle-followup` | Dock icon squircle — continued RCA/fix | **Must** | 1 | Done | 【图1】; carries `R-V1135P1-001` |
| `2026-07-23-v1.136-p1-sidebar-inline-create-tabs` | Sidebar inline create + World/Work tabs | **Must** | 1 | Done | Inline create; residuals `R-V1136P1-*` |
| `2026-07-23-v1.136-p2-light-mode-chrome-harmony` | Light interactive = cyan-1000 + Button SSOT | **Must** | 2 | Done | VI + TransportError links; residuals `R-V1136P2-*` |

Status values: `Todo` | `InProgress` | `InReview` | `Done` | `Blocked`

**Priority rationale:** P0 and P1 are independent author pain points (desktop packaging vs shell IA). P2 is Must but wave-2 so Button/token changes can land once and P1 fixtures consume the locked interactive token.

## Milestones

| Milestone | Target date | Status |
|-----------|-------------|--------|
| Spec freeze | 2026-07-23 | **done** (PM §5.1 → Architect §5.2 → writing-specialist §5.3 → PM §5.4 lock; compass `status: locked`) |
| Dev complete | 2026-07-24 | pending |
| QC complete | 2026-07-24 | pending |
| Iteration close | 2026-07-24 | pending |

## Acceptance Criteria

### Iteration-level (author observable)

- **AC-I1:** Author Dock tile shows **macOS squircle** (not sharp square) after documented rebuild ritual — or residual stays open with next candidate named (no fake Done).
- **AC-I2:** Creator hub sidebar 功能区 = **World/Work tabs + inline create form** (title + submit) — **not** two dashed create cards. Content = list/empty only; empty copy points at sidebar create.
- **AC-I3:** Light interactive VI is **`brand-cyan-1000`** for primary Button fill, selection borders, active chrome, spinners, and light canvas accents. Neon cyan is **Dark-only** for those roles. Ink `brand-deep-blue` remains titlebar/links. Proven on Studio Components Button matrix + shell/wizard/splash/Work Timeline Selected.
- **AC-I4:** Studio fixtures prove P1 IA + P2 chrome under the **active global theme only** (no side-by-side Light+Dark matrices); theme caption tracks toggle.
- **AC-I5:** Touched Surfaces dual Light/Dark frames converted to single-theme-follow-toggle.
- **AC-I6:** `<TransportErrorBlock>` primary + secondary CTAs are **compact text links** (ErrorState-aligned). Proven on Studio Components full matrix.
- **AC-I7:** Product selected/primary chrome matches Button/token SSOT — no one-off parallel VI.
- **AC-I8:** P1/P2 changes land in DESIGN/tokens/`@42ch/nexus-ui` (or shared extract) and are proven in Studio Tokens/Brand/Components/Surfaces.

### Plan mapping (iteration AC → spec gates)

| Iteration AC | P0 spec (P0G) | P1 spec (P1G) | P2 spec (P2G) | P0 plan AC | P1 plan AC | P2 plan AC |
|--------------|---------------|---------------|---------------|------------|------------|------------|
| AC-I1 | P0G-1, P0G-3, P0G-4 | — | — | AC-3 | — | — |
| AC-I2 | — | P1G-1, P1G-2, P1G-4 | — | — | AC-1, AC-2, AC-4 | — |
| AC-I3 | — | — | P2G-1, P2G-2, P2G-4, P2G-5 | — | — | AC-1, AC-2, AC-6 |
| AC-I4 | — | P1G-3 | P2G-5 | — | AC-3 | AC-5 |
| AC-I5 | — | P1G-3 | P2G-5 | — | AC-3 | AC-5 |
| AC-I6 | — | — | P2G-3 | — | — | AC-3 |
| AC-I7 | — | — | P2G-5 | — | — | AC-6 |
| AC-I8 | — | P1G-4 | P2G-1, P2G-5 | — | AC-4 | AC-1, AC-4 |

**Author gate:** PM/QC cannot close AC-I1, AC-I2, or light-VI “feels right” on agent assertion alone — author or Studio fixture per spec gate tables.

## Non-Goals

- Fixing **Open Design** product UI (IA reference only for P1)
- Orchestrator IA redesign / canvas features
- Dark-mode primary CTA redesign (neon cyan stays)
- Closing author residuals without author eyeball
- Studio dual Light\|Dark side-by-side matrices
- Replacing ink deep-blue for titlebar / logo structure
- One-off product restyles that diverge from Studio SSOT
- Below-lg sidebar create affordance (`R-V1135P0-003`) — deferred
- Modal-only create as **primary** hub path (inline sidebar create is primary; dialogs demoted to list pages only)

## Roadmap Position

- **Current iteration（V1.136）:** Dock squircle follow-up; sidebar inline create tabs; Light interactive cyan-1000 + TransportError link CTAs + Studio SSOT reuse
- **Next iteration:** Author closes any remaining Dock/sidebar visual residuals; resume Control Room polish / below-`lg` create affordance (`R-V1135P0-003`) as needed
- **最终目标:** Author-visible dogfood matches intent — Dock looks native; create is inline in sidebar; Light VI is harmonious Chronos teal without office ink or neon cyan

## Delivery Branch Policy

| Field | Value |
|-------|-------|
| `iteration_base_branch` | `main` |
| `spec_integration_branch` | `iteration/v1.136` |
| `target_branch` | `main` |

## Ownership matrix (product + technical — architect §5.2 locked)

| Layer | P0 owner | P1 owner | P2 owner |
|-------|----------|----------|----------|
| **Product AC** | P0G-1–4 in `specs/p0-*` | P1G-1–4 in `specs/p1-*` | P2G-1–5 in `specs/p2-*` |
| **Normative contract** | Pipeline RCA + H4/H5/H6 evidence template + author Dock ritual | `create-inline` mode, direct API submit, independent create tabs, testid map | Token alias map, TransportError link primitive, Tier 1–2 retarget grep list |
| **Code SSOT** | `compose-app-icon.mjs`, `icons:generate`, `icons/README.md` | `creator-shell-content.tsx`, extracted form fields, `sidebar.tsx` host | `tokens.css`, `DESIGN.md`, `@42ch/nexus-ui` Button + TransportErrorBlock |
| **Visual proof** | Author macOS Dock eyeball | Studio fixture (inline create + content browse) | Studio Tokens/Brand/Components/Surfaces |
| **Author residuals** | `R-V1135P1-001`, `R-V1135P1-005` | `R-V1135P0-001`, `R-V1134P3-001` | — |

### Architecture decisions (PM → Architect — locked)

| # | Decision |
|---|----------|
| **Q1** | World + Work: **direct inline API** in sidebar; dialogs **only** on list pages |
| **Q2** | **Evolve** `CreatorShellContent` → `create-inline`; keep `sidebar-create-panel` testid |
| **Q3** | Create-zone tabs **independent** from content hub tabs; optional post-success sync |
| **Q4** | P0: H4/H5/H6 evidence template per attempt in iteration RCA guide |
| **Q5** | P2 wave 2; P1 uses semantic Button — **not blocked** on token lock |
| **Q6** | P2 spec Tier 1–2 file retarget map + grep closure in T4 |

## Risk Register

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Dock still square after another compose tweak | High | High | Ordered RCA; no fake Done; keep residual open |
| Agents keep CreateCardButton as “sidebar create” | High | High | P1G-1 Pass/Fail table; anti-pattern list; Studio fixture before App claim |
| Implementers invent parallel button/selection chrome | High | High | HARD SSOT rule; QC rejects one-offs |
| Light cyan-1000 still feels wrong to author | Med | Med | Nudge hex at Studio review; token-only change |
| Sidebar inline create scope creep into dialogs rewrite | Med | Med | Reuse form primitives; tabs local to create zone only |
| Studio dual-theme matrices left in place | Med | Med | AC-I4/I5; convert on touch |
| OD 【图3】 mistaken for implementation scope | Med | High | Terminology lock + P1 non-goals |

## Iteration package

| Path | Purpose |
|------|---------|
| `specs/p0-dock-icon-squircle-followup.md` | P0 product gates + architect pipeline contract (§5.2) |
| `specs/p1-sidebar-inline-create-tabs.md` | P1 product gates + architect IA contract (§5.2) |
| `specs/p2-light-mode-interactive-vi.md` | P2 product gates + architect token/primitive contract (§5.2) |
| `guides/p0-dock-icon-rca.md` | P0 RCA log — extend V1.135 `guides/p1-dock-icon-rca.md` (implement Task 1) |
| `README.md` | Package index |

## Quality Gate Summary

> Filled at iteration-close.

| plan_id | QC decision | QA gate | Residuals | Durable summary |
|---------|-------------|---------|-----------|-----------------|
| `2026-07-23-v1.136-p0-dock-icon-squircle-followup` | pending | pending | — | — |
| `2026-07-23-v1.136-p1-sidebar-inline-create-tabs` | pending | pending | — | — |
| `2026-07-23-v1.136-p2-light-mode-chrome-harmony` | pending | pending | — | — |

## Compound Round Summary

> Filled at iteration-close.

## Iteration Retrospective (minimal)

> Filled at iteration-close.
