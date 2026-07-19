# Spec — Sidebar selection-mode submenu shell (V1.126 P0)

**Status:** product-reviewed, architect-locked, writing-hygiene done (Phase 1 §1.6 seat 3 inline fallback — empty subagent response; PM applied flagged hygiene per V1.124 pattern)
**Document class:** Iteration package working spec (not `{SPECS_DIR}` Master)
**Compass:** [`../delivery-compass.md`](../delivery-compass.md) — AC-V1126-1
**Plan:** [`2026-07-20-v1.126-p0-shell-selection-submenu`](../../../plans/2026-07-20-v1.126-p0-shell-selection-submenu.md)
**Wire contracts:** `wire_contracts_changed: false` (PM proposal) — frontend chrome only; agent assignment reuses `setAgentProfile` IPC; rename/delete reuse existing PATCH routes.

## Problem

Today the Creator-tab sidebar row only navigates — clicking a World row navigates to `/worlds/<id>/timeline`, clicking a Work row navigates to `/works/<id>/outline`. Authors have no in-place chooser for "what to do with this entity". V1.125 Non-Goal "Selection → submenu shell (World/Work selected mode + agent dialog)" is the deferred structural answer. Concrete dogfood friction: the agent assignment flow requires going to Settings, picking an agent, then coming back to the entity — the submenu puts assignment at the entity.

## Normative decisions (PM initial — pending seat 1/2/3)

1. **Trigger contract — non-navigating click opens submenu (product-locked at seat 1).** A plain click on the row body **navigates** (existing behavior preserved — least surprise for mouse users). The submenu opens via:
   - **`Enter` when the row is focused** → opens submenu (**product-locked recommendation**; overrides the current `Enter` = navigate default). Rationale: keyboard-only authors need direct access to per-entity actions without learning a chord; the submenu contains `Open Timeline` as item #1, so a keyboard user who meant to navigate just presses `Enter → Enter`.
   - **`⌘.` (macOS) / `Ctrl.` (Win/Linux) when the row is focused** → opens submenu (power-user chord). Architect seat 2 ratifies — flag: `⌘.` also opens the keyboard-shortcuts sheet in VS Code and friends; if dogfood shows conflict, fall back to `Shift+F10` (standard context-menu key).
   - **`•••` button at the right edge of the row** → opens submenu (mouse + touch accessible — discoverable entry point for non-keyboard users).
   - **`click` outside the row body but inside the row's submenu affordance zone** → no-op (popover only opens from the three triggers above).
2. **Submenu shape — popover anchored to row.** Anchored to the row's right edge; auto-flips if viewport edge is near. Width: 280px (matches dialog max-width token). Body is a vertical menu of `menuitem` rows.
3. **Submenu contents (locked list — conditional items are explicit so the menu shape stays predictable):**
   | Position | Label | Action | Visible on |
   |----------|-------|--------|-----------|
   | 1 | Open Timeline | Navigate to `/worlds/<id>/timeline` (World) or `/works/<id>/timeline` (Work) | World + Work |
   | 2 | Open KB / Open Outline | World: `/worlds/<id>/kb`; Work: `/works/<id>/outline` | World XOR Work (conditional copy + target — single menu slot) |
   | 3 | Agent: `<current agent or "Unassigned">` › | Opens `AgentPicker` in dialog mode; on apply → `setAgentProfile` IPC + invalidation (V1.125 P0 T2 contract) | World + Work |
   | 4 | Rename | Inline-edit the row label; persists via `PATCH /v1/daemon/works/{id}` (Work) or World KB patch (World) | World + Work |
   | 5 | Delete | Confirmation dialog (`Delete <World Name>` / `Delete <Work Name>` per DESIGN.md §Voice — name the changed object); existing delete flows | World + Work |
4. **Transient state.** Submenu open/close is **not** URL-persisted — it is transient UI state (popover open/close). The active World/Work selection itself is already URL-derived.
5. **Keyboard contract.** Row focused → `Enter` / `⌘.` opens; arrow keys move within submenu; `Esc` closes and returns focus to the row; `Tab` out closes. **Dismiss triggers:** outside-click, Esc, route change, blur to outside the submenu.
6. **i18n + Voice.** Verb-only action labels (`Open`, `Rename`, `Delete`) per DESIGN.md §Voice & Content; entity name in dialog title (`Delete <World Name>`, `Rename <World Name>`). All keys in the `shell` namespace; en + zh-CN.
7. **Studio-first policy.** Per root `AGENTS.md` UI Component Policy, a Studio fixture lands with all variants. Extract to `@web-shell/selection-submenu` (new alias root) **if** the implementation produces a pure presentational shape (Studio fixture is the second consumer justifying extraction per V1.106 rule). If implementation is glue-heavy (uses `useNavigate`, `useTranslation`, App routing), keep app-local with header comment per V1.115 pattern.
8. **Reuse, not reinvent.** The `AgentPicker` dialog component (V1.110 + V1.119 catalog) is reused — only its **invocation mode** changes (called from submenu instead of Settings). No new agent picker; no AgentPicker component edits.

## Concerns for architect seat 2

- **`⌘.` chord conflict.** VS Code (and most Electron-based editors) bind `⌘.` to the keyboard-shortcuts sheet. Nexus authors often run inside or alongside an editor. Product recommendation: keep `Enter` as the primary keyboard trigger; treat `⌘.`/`Ctrl+.` as additive power-user chord. If ratification rejects `⌘.`, fall back to `Shift+F10` (standard Windows/web context-menu key) — no product copy change needed.
- **`Enter` override risk.** Today `Enter` on a focused sidebar row navigates. Existing tests asserting "Enter navigates" must be retargeted to assert "Enter opens submenu; submenu Enter on item #1 navigates". Implementer must update `sidebar.test.tsx` matrix in the same task as the override (do not split).
- **`@web-shell/selection-submenu` alias root vs app-local.** Architect decides: extract-as-alias (Studio fixture as 2nd consumer justifies extraction per V1.106) OR keep app-local with header comment (V1.115 pattern). Product has no preference as long as Studio fixture lands in-iteration (V1.106 invariant).

## Architecture locks (architect seat 2)

> Ratified 2026-07-20. All AQ verdicts are final — implementers treat these as non-negotiable architecture contracts.

### ND-A1 — Trigger contract (AQ-1 + AQ-2)

- **`Enter` is the primary keyboard trigger.** When a sidebar row is focused, `Enter` opens the contextual submenu. This overrides the current `Enter` = navigate behavior. The submenu's item #1 is "Open Timeline" — a keyboard user who meant to navigate presses `Enter → Enter`.
- **`⌘.` (macOS) / `Ctrl+.` (Win/Linux) is the additive power-user chord.** Registered as secondary keyboard trigger. `Shift+F10` is the explicit documented fallback if dogfood reports persistent chord conflict with VS Code / Electron editors. The submenu opens on either chord; no product copy change needed if the fallback is activated.
- **`•••` button at row-end** is the mouse + touch entry point. Visible on row hover/focus.
- **Click on row body navigates** (existing behavior preserved — least surprise for mouse users).
- **AQ-2 locked:** The `sidebar.test.tsx` matrix update (existing "Enter navigates" assertions → "Enter opens submenu; submenu Enter on item #1 navigates") **MUST** be in the same task (T1) as the Enter handler override. Do not split.

### ND-A2 — Submenu alias extraction (AQ-3)

- **EXTRACT to `@web-shell/selection-submenu`** (new alias root under `packages/nexus-ui` or `@web-shell/` per extraction pattern).
- **Extraction scope:** the presentational `SelectionSubmenu` component (menu items list, keyboard navigation, focus trap, ARIA roles, popover anchor logic). This is pure presentational — it receives menu items, callbacks, and labels via props.
- **App-owned scope (stays in `apps/web`):** row data mapping (`ShellNavItem` → `SelectionMenuItem`), routing callbacks (`useNavigate`), i18n injection (`t()`), entity-specific handlers (rename → PATCH, delete → confirm + PATCH, agent assignment → IPC).
- **Studio fixture** is the second consumer (app → Studio), satisfying the V1.106 ≥ 2 consumers rule.
- **`SelectionMenuItem` type contract:**
  ```ts
  interface SelectionMenuItem {
    id: string;
    label: string;
    icon?: LucideIcon;
    disabled?: boolean;
    variant?: 'default' | 'danger';
    onSelect: () => void;
  }
  ```
- **`SelectionSubmenu` props contract:**
  ```ts
  interface SelectionSubmenuProps {
    items: SelectionMenuItem[];
    open: boolean;
    onClose: () => void;
    anchorEl?: HTMLElement | null;
    width?: number; // default 280
    ariaLabel: string;
  }
  ```

### ND-A3 — Sidebar integration contract

- **`ShellSidebarChromeProps` gains one new optional prop:**
  ```ts
  renderSubmenu?: (item: ShellNavItem) => ReactNode;
  ```
- When `renderSubmenu` is provided, the chrome renders the submenu popover anchored to the active row's right edge. The chrome owns the popover portal container and the `isOpen` per-row state (tracked by `item.to`).
- **Focus-trap lifecycle:** the submenu component (`SelectionSubmenu`) owns the focus trap (on mount → trap inside submenu; on Esc → release + return focus to the row). The chrome does **not** own focus management.
- **Dismiss triggers:** outside-click, Esc, route change, `Tab` out of the submenu. The chrome listens to `useLocation()` for route-change dismiss.

### ND-A4 — AgentPicker reuse contract (NG-14 locked)

- The existing `AgentPicker` component (`apps/web/src/components/agents/`) is **reused as-is** — no component file edits.
- Invocation mode: the submenu's "Agent: <name>" item opens `AgentPicker` in dialog mode via a callback passed from the app layer. The `AgentPicker` component itself is **not** modified.
- On apply, persists via existing `setAgentProfile` IPC + invalidation keys (V1.125 P0 T2 contract).

### ND-A5 — Wire contracts verdict

- **`wire_contracts_changed: false` — CONFIRMED.** Submenu is frontend chrome only. Agent assignment reuses existing `setAgentProfile` IPC. Rename/delete reuse existing `PATCH /v1/daemon/works/{id}` + World KB patch routes. No new schemas, no codegen, no daemon Rust changes. No PATCH route additions.

## Architecture notes (implementer)

| Component | Change |
|-----------|--------|
| `apps/web/src/components/layout/sidebar.tsx` | Add submenu trigger handler per row (Enter key handler, `⌘.`/`Ctrl+.` chord handler, `•••` button click handler); pass `renderSubmenu` render-prop to `ShellSidebarChrome`; Enter override + test matrix update in same task (ND-A1) |
| `apps/web/src/components/layout/presentational/shell-sidebar-chrome.tsx` | New optional `renderSubmenu?: (item: ShellNavItem) => ReactNode` prop; render popover portal when active row has a submenu; track `isOpen` per `item.to`; dismiss on route change (ND-A3) |
| New `packages/nexus-ui/src/selection-submenu/selection-submenu.tsx` (extracted per ND-A2) | Presentational `SelectionSubmenu` component: menu items list, keyboard nav (arrow keys), focus trap, ARIA `menu`/`menuitem` roles, popover anchor logic. Props: `items: SelectionMenuItem[]`, `open`, `onClose`, `anchorEl`, `width`, `ariaLabel` (ND-A2) |
| New `apps/web/src/components/layout/selection-submenu.tsx` (app wrapper) | Thin app wrapper over extracted `SelectionSubmenu`: maps `ShellNavItem` → `SelectionMenuItem[]`, injects `useNavigate` callbacks, `t()` labels, entity-specific handlers (ND-A2) |
| New `apps/web/src/components/layout/selection-submenu.test.tsx` | Keyboard interaction + a11y + content tests; `sidebar.test.tsx` matrix updated in same task (T1 — ND-A1) |
| `apps/design-studio/src/fixtures/selection-submenu-fixtures.tsx` | All-variants fixture (light + dark; World/Work; with/without agent; rename in progress; delete confirm); consumes the extracted `@web-shell/selection-submenu` (ND-A2) |
| `apps/design-studio/src/pages/surfaces.tsx` | New "Selection Submenu" section |
| `apps/web/src/locales/{en,zh-CN}/shell.json` | New keys: `submenu.openTimeline`, `submenu.openKb`, `submenu.openOutline`, `submenu.agent`, `submenu.unassigned`, `submenu.rename`, `submenu.delete`, `submenu.deleteConfirm*` |

### Architecture locks

- **T1 (trigger + dismiss):** `Enter` primary trigger (ND-A1); test matrix update in same task. Files: `sidebar.tsx` (Enter handler + chord handler), `shell-sidebar-chrome.tsx` (`renderSubmenu` prop), new `selection-submenu.tsx` (app wrapper), new `selection-submenu.test.tsx`, `sidebar.test.tsx` (updated matrix).
- **T2 (contents):** `AgentPicker` reused as-is — no component edits (ND-A4). Agent assignment persists via `setAgentProfile` IPC. Rename inline edit → `PATCH /v1/daemon/works/{id}` (Work) / World KB patch (World). Delete confirm dialog titled `Delete <World Name>` / `Delete <Work Name>`.
- **T3 (a11y + i18n):** ARIA `menu`/`menuitem` roles on the extracted `SelectionSubmenu`; focus trap within submenu. All visible strings via `t()` in `shell` namespace.
- **T4 (Studio fixture):** Consume the extracted `@web-shell/selection-submenu` presentational component (ND-A2). Light + dark, all variants. `wire_contracts_changed: false` (ND-A5).

## Acceptance (author-observable)

| ID | Author sees / does |
|----|-------------------|
| AC-V1126-1 | Sidebar row opens submenu via `Enter` (primary keyboard path), `⌘.`/`Ctrl+.` (power-user chord — pending architect ratification per Concerns), or `•••` button (mouse + touch); submenu has Timeline/KB (World) / Outline (Work) entry, agent assignment, rename, delete; keyboard complete (arrow keys + Esc + Tab-out dismiss); i18n complete (en + zh-CN); a11y complete (ARIA menu roles + focus trap); Studio fixture shipped (all variants, light + dark) |

## Out of scope

Agent dialog inside the submenu itself (submenu only triggers the existing `AgentPicker` dialog — the dialog component is reused unchanged); `AgentPicker` component refactor or visual redesign (NG-14); Orchestrator-tab row submenus (those routes already have list/detail pages); multi-select batch actions in the sidebar (NG-16); mobile / sidebar-below-`lg` UX (NG-15); agent dialog inside the submenu itself; new wire contracts (`wire_contracts_changed: false` — agent assignment reuses `setAgentProfile` IPC, rename/delete reuse existing PATCH routes).
