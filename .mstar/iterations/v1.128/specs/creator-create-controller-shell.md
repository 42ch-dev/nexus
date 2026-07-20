# Spec — Creator Create vs Controller shell (V1.128 P2)

**Status:** product-reviewed, architect-locked, writing-hygiene done  
**Document class:** Iteration package working spec (not `{SPECS_DIR}` Master)  
**Compass:** [`../delivery-compass.md`](../delivery-compass.md) — AC-V1128-3  
**Plan:** [`2026-07-20-v1.128-p2-creator-create-controller-shell`](../../../plans/2026-07-20-v1.128-p2-creator-create-controller-shell.md)  
**Wire contracts:** `wire_contracts_changed: false`  
**Related specs:** [P3 — `@web-*` / `@42ch/nexus-ui` clarity](web-alias-clarity.md) (P2 owns `@web-layout/creator-shell-content`; P3 labels Shell surfaces); V1.125 [creation-world-first-ia](../../v1.125/specs/creation-world-first-ia.md) (`createWorld` honesty path)

## Problem

App shell chrome left/main Creator region does not match author mental model: with no Work/World selected it should be a **Create** function page; with a selection it should be a **Controller Panel** (business widgets TBD — this iteration ships stub only). Studio Shell fixtures still lag Worlds-first IA and do not show these two modes side by side.

## User value

**Before:** A maintainer cannot preview the Create → select → control shell flow in Studio; App Creator region does not clearly separate “start something new” from “control what is selected.”

**After:** The maintainer toggles Studio Shell fixtures between empty Create (honest World/Work CTAs) and selected Controller stub (placeholder + Back), then confirms the same two modes in App — establishing IA before Controller business content lands.

## Normative decisions

### Mode A — Empty selection (Create page)

When Creator tab is active and **no Work or World entity is selected** in the shell selection model, the **primary content region** (not the Worlds/Works sidebar list) shows a **Create page**:

| CTA | Author action | Behavior (reuse V1.125) |
|-----|---------------|-------------------------|
| **Create World** (when `createWorld` present) | Clicks card-sized CTA | Opens Create World flow |
| **Create World** (when `createWorld` absent) | Sees honest fallback copy | Single card CTA opens **Create Work** with copy such as “Create a Work to get started — Worlds are created from your Works.” No disabled button, no silent no-op |
| **Create Work** | Clicks card-sized CTA | Opens existing `CreateWorkDialog` flow |

Create page is **not** the Worlds/Works list — it is the content pane authors see when nothing is selected. Sidebar nav remains Worlds → Works list chrome.

### Mode B — Selected Work or World (Controller Panel stub)

When a **Work or World is selected**, the same content region shows a **Controller Panel stub**:

- Placeholder copy indicating Controller content is **TBD** (e.g. “Controller Panel — coming soon”)
- Single primary **Back** control
- **No** business widgets (timeline controls, agent assignment, delete, etc.) in V1.128

### Back behavior (architect-locked — Seat 2)

**LOCKED:** **Back** on the Controller stub:

1. Clears `selectedEntity` in `CreatorEntitySelectionContext` (sets to `null`)
2. Returns the author to **Mode A (Create page)** in the content region
3. Does **not** navigate away from Creator tab or collapse the sidebar
4. Does **not** scroll-highlight a Worlds/Works list row while keeping selection — the list-highlight alternate is **rejected** (ambiguous selected-but-Create state)

### Selection model (architect-locked — Seat 2)

**SSOT:** `CreatorEntitySelectionContext` — explicit React context in the App layout layer (`apps/web/src/components/layout/`), **not** route-derived.

```typescript
type CreatorEntityRef =
  | { kind: 'work'; id: string; label: string }
  | { kind: 'world'; id: string; label: string };

// Context value: selectedEntity: CreatorEntityRef | null
```

| State source | Role in V1.128 | Evidence |
|--------------|----------------|----------|
| **`CreatorEntitySelectionContext.selectedEntity`** | **SSOT** for Create vs Controller content region | New in P2 — no prior equivalent |
| `submenuItem` in `shell-sidebar-chrome.tsx:107` | Ephemeral Selection submenu popup anchor only | **Not** entity selection |
| Route params `/works/:workId/*`, `/worlds/:worldId/*` | Work-shell / canvas surfaces (`isWorkShellRoute` in `work-shell-routes.ts:21`) | **Orthogonal** — P2 does not repurpose canvas routes as selection |
| React Flow node selection | Canvas inspector wiring (`timeline-canvas.tsx:454`) | **Out of scope** |

**Writers:** Sidebar entity row activation (Work row in `sidebar.tsx:143–147`; World pick from Worlds list or future per-world sidebar row) calls `setSelectedEntity`. Controller **Back** calls `setSelectedEntity(null)`.

**Readers:** `CreatorShellContent` presentational extract (`@web-layout/creator-shell-content`, owned by P2) receives `selectedEntity` + mode props; renders Create page vs Controller stub.

**Route boundary:** Creator hub routes (`/works`, `/worlds` — list/hub surfaces) host the content region swap. Deep canvas routes (`/works/:workId/outline`, `/worlds/:worldId/timeline`, etc.) remain work-shell / canvas-first and do **not** inherit P2 Controller stub — selection context may persist in background but content region is owned by canvas routes on those paths.

**Studio fixtures:** Prop-driven toggle (`selectedEntity` fixture prop) — no App context in Design Studio; mirrors conflict-modal / shell fixture pattern.

**Wire contracts:** `wire_contracts_changed: false` (confirmed; `createWorld` honesty path only — `worlds-page.tsx:37–43`, `lib/nexus/create-world.ts:6–7`).

### Studio-first

Shell fixtures must show **both modes** (empty Create + selected Controller stub) with Worlds-first nav data **before** App wiring.

### Nav boundary

Nav sidebar remains Worlds → Works list chrome; this spec owns the **content region modes**, not a rewrite of selection-submenu.

## Acceptance (author-observable)

| ID | Author sees / does |
|----|-------------------|
| AC-V1128-3a | In Studio `/surfaces/shell`, toggles fixture: **empty** → Create page with card CTAs (World path honors `createWorld` detect; Works path → Create Work); **selected** → Controller stub with placeholder + **Back** |
| AC-V1128-3b | In App Creator: with no selection, sees Create page CTAs per table above; selects Work or World → Controller stub; clicks **Back** → selection cleared, Create page returns |
| AC-V1128-3c | Controller stub shows **no** business widgets beyond placeholder + Back — author understands content is TBD |

## Out of scope

Full Controller Panel business content; Delete routes; agent assignment redesign; `POST /v1/daemon/narrative/worlds` or new Create World wire contract; wiring `createWorld` desktop bridge (V1.127 confirmed absent — honesty path only).
