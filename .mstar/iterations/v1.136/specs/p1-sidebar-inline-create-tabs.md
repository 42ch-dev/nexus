# P1 — Sidebar inline create + World/Work tabs (V1.136)

**Status:** draft (Phase 1 §1.6 — **Architect §5.2 technical contract locked**; writing-specialist §5.3 complete — PM lock next)  
**Plan:** `2026-07-23-v1.136-p1-sidebar-inline-create-tabs`

## Author problem (plain language)

V1.135 fixed **where** create lives — the left sidebar 功能区, not the content column. Good. But the sidebar still shows two big dashed **“创建世界 / 创建工作” cards** that only open dialogs. I want to **type a title and submit right there**, switching between World and Work with **tabs in the create zone itself** — like the layout spirit of 【图3】, not card buttons. The main content area should stay **browse-only** (lists or empty state telling me to create from the sidebar).

**【图2】 evidence:** Dashed create cards in sidebar.  
**【图3】 / Open Design:** IA reference only — tab + inline form layout inspiration; **do not** implement OD product UI.

## User value

- **Speed:** One glance, one tab, one title field, one submit — no card → dialog detour for the common path.
- **Clarity:** Sidebar = create; content = browse. No mixed signals or duplicate create surfaces.
- **Continuity:** Builds on V1.135 placement win without reverting to content dual-pane create.

## Intent

Shell sidebar 功能区 hosts an **inline create zone**: local **World | Work** tabs, title input, submit. **Not** two dashed `CreateCardButton` cards. Content = browse/empty only.

## Terminology

| Term | Means | Does **not** mean |
|------|-------|-------------------|
| Sidebar 功能区 | `ShellSidebarChrome.panelContent` | Content dual-pane left; OD product shell |
| **Inline create zone** | Tabs + title field + submit inside sidebar panel | `CreateCardButton` ×2; dialog as the only visible affordance |
| Content | Hub list / empty — no create cards | Create form column in content |
| OD 【图3】 | IA reference — tabbed inline form pattern | Scope to ship Open Design UI |

## Product acceptance (P1G — PM locked)

Author-observable gates. Plan AC-1–AC-5 map 1:1 below.

| ID | Author can observe… | Pass | Fail |
|----|---------------------|------|------|
| **P1G-1** | Sidebar create pattern | **World \| Work tabs** + title field + submit inline in sidebar | Two dashed create cards; cards that only open dialogs as primary UX |
| **P1G-2** | Content on hub | Tabs + card grid or empty only; empty copy points to **sidebar** create (en + zh-CN) | Create cards or inline form in content column |
| **P1G-3** | Studio proof | Fixture shows sidebar inline create + content browse under **active theme only** (caption follows toggle) | App-only wiring; Light\|Dark side-by-side matrix |
| **P1G-4** | SSOT reuse | Uses existing Input / Button / tab patterns from Components; no third create surface | New one-off create card system; parallel chrome in `apps/web` |

**Compass mapping:** AC-I2, AC-I4, AC-I8 ↔ P1G-1–P1G-4.

**Author gate:** `R-V1135P0-001` closes when P1G-1–P1G-3 pass with **inline** pattern (`@author`). `R-V1134P3-001` closes when sidebar inline create supersedes card-button hub UX.

## Target experience (product — architect details wiring)

```
Sidebar panelContent (creator hub)
├── [ World | Work ]   ← local tabs (create zone only)
├── Title input
└── Submit control

Content (CreatorHubPage)
├── Hub tab bar (World | Work — browse SSOT)
└── Card list OR empty (“从侧边栏创建”)
```

**Primary path:** inline title + submit in sidebar. **No** `CreateCardButton` gate.

## Architect technical contract (§5.2 — normative)

### Architecture decisions (PM open questions)

| # | Question | Decision |
|---|----------|----------|
| **Q1** | Submit path: direct API vs dialog-wrapped | **World tab:** direct inline submit via `useCreateWorld` — **no dialog** on happy path. **Work tab:** full inline form (title + long-term goal + initial idea + optional profile) via `useCreateWork` — **no dialog** on happy path. Dialogs **remain** on `worlds-page` / `works-page` only; **removed from sidebar host** (`CreatorCreatePanel`). |
| **Q2** | Component split vs `CreatorShellContent` | **Evolve** `CreatorShellContent`: replace `mode: 'create'` card layout with `mode: 'create-inline'`. Extract shared field blocks from dialogs into presentational subcomponents (see File ownership). **Delete** `CreateCardButton` usage from hub path. |
| **Q3** | Dual tab bars: shared vs independent state | **Independent.** Create-zone tabs = local `useState<HubTab>` inside inline panel. Content hub tabs = existing `useHubTabState` in `CreatorHubDualPane`. **Optional post-success sync only:** after successful create on tab `X`, navigate content hub to tab `X` + refresh lists — **not** coupled during browsing. |
| — | `CreateWorldDialog` / `CreateWorkDialog` fate | **Keep** dialog components for non-hub routes. Sidebar uses extracted **form field** primitives; dialogs may thin-wrap same fields later (out of scope unless refactor is trivial). |

### File ownership matrix

| Layer | Path | Owner | Notes |
|-------|------|-------|-------|
| **Presentational SSOT** | `apps/web/src/components/layout/presentational/creator-shell-content.tsx` | P1 | Add `create-inline` mode; remove card mode from hub |
| **Form fields (extract)** | `apps/web/src/components/worlds/create-world-form-fields.tsx` (new) | P1 | Title input + validation props; shared by inline + dialog |
| **Form fields (extract)** | `apps/web/src/pages/dialogs/create-work-form-fields.tsx` (new) | P1 | Title + goal + idea + profile; shared by inline + dialog |
| **Dialog wrappers** | `create-world-dialog.tsx`, `create-work-dialog.tsx` | P1 touch | Thin shell over extracted fields — **not** used by sidebar |
| **Sidebar host** | `apps/web/src/components/layout/sidebar.tsx` `CreatorCreatePanel` | P1 | Mount inline panel; drop dialog `open` state |
| **Content browse** | `creator-hub-dual-pane.tsx`, `hub-tab-bar.tsx` | P1 read-only | Browse tabs unchanged |
| **Tab primitive reuse** | `hub-tab-bar.tsx` | P1 | Reuse `HubTabBar` in create zone with distinct `data-testid` prefix |
| **Studio fixture** | `apps/design-studio/src/fixtures/creator-orch-gongnengqu-ia-fixtures.tsx` (primary) | P1 | Update PAC matrix for inline create; retire card-button assertions |
| **i18n** | `src/locales/{en,zh-CN}/worlds.json`, `shell.json` | P1 | Empty copy → sidebar create; inline labels |

### `data-testid` contract (normative)

| testid | Element | Required |
|--------|---------|----------|
| `sidebar-create-panel` | Outer create zone wrapper | **Keep** — stable across V1.135→V1.136 |
| `sidebar-create-tab-bar` | Create-zone `HubTabBar` root | **New** |
| `sidebar-create-tab-world` | World tab button | **New** |
| `sidebar-create-tab-work` | Work tab button | **New** |
| `sidebar-create-form-world` | World inline form container | **New** |
| `sidebar-create-form-work` | Work inline form container | **New** |
| `sidebar-create-submit-world` | World submit control | **New** |
| `sidebar-create-submit-work` | Work submit control | **New** |
| `creator-create-world` / `creator-create-work` | Card buttons | **Remove** from hub path |
| `creator-hub-dual-pane-tab-bar-*` | Content hub tabs | **Unchanged** |

### Interface sketch (`create-inline` mode)

```ts
type CreatorShellContentProps =
  | { mode: 'create-inline'; canCreateWorld: boolean; onWorldCreated?: (id: string) => void; onWorkCreated?: (id: string) => void; labels: CreatorShellInlineCreateLabels; 'data-testid'?: string }
  | { mode: 'controller'; /* unchanged */ };
```

Host (`CreatorCreatePanel`) owns: client capability (`hasCreateWorldClient`), mutations, navigation after success, i18n `labels`. Presentational layer owns: tab UI, form layout, disabled states, testids.

### Anti-patterns (architect — do not ship)

1. Sidebar opening `CreateWorldDialog` / `CreateWorkDialog` as primary affordance.
2. Shared React state between `sidebar-create-tab-*` and `creator-hub-dual-pane-tab-bar-*`.
3. New parallel create component outside `CreatorShellContent` + extracted form fields.
4. Hard-coded `bg-brand-cyan` / `text-blue-700` on create chrome — use `Button` / `Input` SSOT (P2 token wave propagates automatically).

### P1 ↔ P2 token ordering

P1 **does not block** on P2. Inline create uses `@/components/ui` `Button` + `Input` + `HubTabBar` semantic classes — **no hard-coded hex**. P1 fixture may ship in wave 1 with current light primary (`brand-deep-blue`); P2 retargets Button SSOT and fixtures pick up `brand-cyan-1000` without P1 rework.

## Rejected (PM §5.1 clarify)

1. **Card-button hub UX** — `CreateCardButton` ×2 as primary sidebar create (current V1.135 regression of author intent).
2. **Dialog-as-only-create** — sidebar shows only buttons that open modals with no inline title field.
3. **Content dual-pane create** — any create form column in `CreatorHubPage` content (V1.134 misread — still forbidden).
4. **Open Design product UI** — implementing OD shell beyond IA reference.
5. **Third create surface** — new hub-only create component parallel to sidebar + dialogs.
6. **Re-hide sidebar create on hub** — `panelContent` must stay populated on `/works`, `/worlds`.

## Non-goals

- Open Design product UI implementation
- Orchestrator create redesign
- Dual Light\|Dark Studio matrices
- Below-lg sidebar hidden create (`R-V1135P0-003`) — deferred
- Removing dialog submit paths on list pages (`worlds-page` / `works-page`)

## PM sign-off (§5.1)

| Field | Value |
|-------|-------|
| **Product intent** | Ready for Architect §5.2 |
| **Date** | 2026-07-23 |
| **Blocked** | None — clarify closed via author direction + 【图2】/【图3】 |

## Architect sign-off (§5.2)

| Field | Value |
|-------|-------|
| **Technical contract** | Locked — submit path, component split, independent tab state, testid map |
| **Date** | 2026-07-23 |
| **PM Q1–Q3** | Answered in Architecture decisions table above |
