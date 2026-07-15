# Canvas Outline Spatial Parity — Primary Spec (V1.108 P0)

**Status:** Draft — product-complete (§5.1 PM); architecture locked (§5.2 architect); writing-complete (§5.3)  
**Tier:** Must (P0)  
**Plan:** `2026-07-10-v1.108-canvas-outline-spatial`  
**Compass:** `../v1.108/delivery-compass.md`  
**Normative master:** `.mstar/specs/canvas-strategy-surface.md` (Outline+Timeline surface)

## Product outcome

Authors can shape a Work's outline and timeline **spatially** on a React Flow graph — the same interaction model Strategy and World KB already ship — while keeping existing patch semantics and inspectors.

**User-visible win:** Opening `/works/:id/outline` shows a canvas graph (not a panel-only grid); authors toggle to list views when needed; foreshadow links are visible and editable; outline chrome uses shipped `canvas-outline-*` DESIGN tokens.

## Problem

V1.72 shipped outline/timeline **patch routes** and normative copy claiming a React Flow spatial graph. Current UI (`outline-canvas.tsx`) is a **panel + inspector grid** with no `CanvasShell` / `@xyflow/react`. Strategy and World KB already ship spatial graphs with graph ↔ list toggles — Outline is the remaining α spatial surface and the largest canvas spec↔implementation drift.

## Goals

1. Spatial Outline+Timeline graph inside shared `CanvasShell` (Volume/Chapter nodes + timeline lane).
2. Graph ↔ inspector sync; existing OCC patch routes unchanged in semantics.
3. Non-spatial alt views (chapter list + timeline list) with toolbar toggle (Strategy/World KB pattern).
4. Consume shipped `canvas-outline-*` DESIGN tokens on all shipped node/edge chrome.
5. Minimum foreshadow authoring UI (create/link/unlink via existing `timeline.patch_event` / structure ops).

## Non-goals

- Scene/Beat as first-class graph nodes; Steer-from-chapter; Idea artifact from outline nodes.
- Full shared command palette; Strategy spatial `onConnect`; graph performance engines (dagre/elk polish).
- New wire DTOs or schema/codegen unless implement blocked (prefer `wire_contracts_changed: false`).
- Studio `/surfaces/canvas` fixtures — **P1** owns presentational preview; P0 owns App Outline behavior.

## Studio-first note

P0 lands App Outline spatial behavior and token consumption. P1 ships Studio canvas Surfaces fixtures for shared shell/context-menu chrome. Visual acceptance for **shared** chrome still follows studio-first when P1 fixtures exist; P0 graph behavior is App-first.

## Voice & Content (locked)

Follow DESIGN.md §Voice & Content: **Title Case** for headings, labels, and CTAs; **sentence case** for helper text and empty-state body copy; **Verb + Noun** for actions.

| Surface | Element | Copy (exact) |
|---------|---------|--------------|
| Outline canvas toolbar | Graph → list | **Show list view** |
| Outline canvas toolbar | List → graph | **Show graph** |
| Outline conflict modal | Title pattern | Reuse outline-flavored copy with `{node_label}` placeholder (V1.72) |
| Foreshadow controls | Link action | **Link Foreshadow** |
| Foreshadow controls | Unlink action | **Unlink Foreshadow** |

Alt-toggle copy **must** match Strategy/World KB (`strategy-canvas/canvas-layout.tsx`, `world-kb-canvas-header.tsx`) for cross-canvas consistency. See also `../guides/studio-first-invariant.md` § Voice & Content.

## Wire

**Default:** `wire_contracts_changed: false`.

| Operation | Route | Patch op | Notes |
|-----------|-------|----------|-------|
| Structure moves | `POST …/outline/patch/structure` | `move_chapter`, etc. | Existing V1.72 |
| Chapter edits | `POST …/outline/patch/chapter` | field patches | Existing V1.72 |
| Timeline CRUD | `POST …/works/{id}/timeline/patch` | `add_event`, `remove_event`, `attach_event_to_chapter` | Existing V1.72 |
| **Link Foreshadow** | same | `link_foreshadow` | `event_id` + `foreshadows_event_id` — ships without wire change |
| **Unlink Foreshadow** | same | `unlink_foreshadow` | **Not in daemon today** — escape hatch below |

**Escape hatch (unlink only):** If implement cannot satisfy FB-C1-005 unlink without raw-file edits, add **additive** `unlink_foreshadow` to `schemas/daemon-api/canvas/outline/timeline-patch-event-request.schema.json`, regenerate contracts, and implement handler in `crates/nexus-daemon-runtime/src/api/handlers/outline.rs` (mirror `timeline_link_foreshadow`). That slice alone sets `wire_contracts_changed: true`; all other C1 deliverables remain `false`.

---

## Architecture Locks (§5.2)

### Module table

| Concern | Path | Notes |
|---------|------|-------|
| **Orchestrator** | `apps/web/src/components/canvas/outline-canvas.tsx` | Thin facade; owns selection, patch dispatch, `showAlt` toggle — mirror `strategy-canvas.tsx` |
| **RF projection** | `outline-canvas/rf-projection.ts` (**new**) | Pure `WorkOutline` + `ChapterSummary[]` → RF `Node`/`Edge` + layout coords |
| **Conflict/helpers** | `outline-canvas/graph-projection.ts` (**existing**) | Keep `ConflictState`, `changedFieldsOf`, `unassignedChaptersOf` — **no** RF types here |
| **nodeTypes** | `outline-canvas/outline-nodes.tsx` (**new**) | Export `outlineNodeTypes` (Volume, Chapter, TimelineEvent lane nodes) — mirror `strategy-nodes.tsx` |
| **CanvasShell** | `apps/web/src/components/canvas/canvas-shell.tsx` | **Consume only** — `CanvasShell` + `useNodeChangeHandler`; P0 does not own shell chrome edits |
| **Toolbar / toggle** | `outline-canvas/canvas-layout.tsx` | Extend with `CanvasHeader` + alt toggle — copy **Show list view** / **Show graph** from Strategy/World KB |
| **Alt view** | `outline-canvas/outline-alt-view.tsx` (**new**) | Non-spatial chapter list + timeline list; P0 owns component (not P1 Studio fixtures) |
| **Inspectors** | `outline-canvas/inspectors/*` | Graph click → same inspector selection as panel path; patch hooks unchanged |
| **Conflict modal** | `outline-canvas/conflict-modal.tsx` → `outline-conflict-modal.tsx` | Reuse shared modal pattern — **no fifth modal** |
| **Data hooks** | `apps/web/src/lib/canvas/use-outline-data.ts` | `useWorkOutline`, `usePatchOutlineStructure`, `usePatchOutlineChapter`, `usePatchTimelineEvent` |
| **Route** | `apps/web/src/pages/outline-page.tsx` → `outline-canvas.tsx` | `/works/:workId/outline` (already route-split in `App.tsx`) |

### Layout (V1.108)

Pragmatic **deterministic lane/layered** layout in `rf-projection.ts` — same class as World KB grid (`LANE_X` / `ROW_Y` constants). Suggested lanes: volumes (left) → chapters (center) → timeline events (bottom lane). **Defer** dagre/elk/auto-layout engines to a future iteration.

### Foreshadow entry points

| UI surface | Link | Unlink |
|------------|------|--------|
| **Event inspector** (`event-inspector.tsx`) | **Link Foreshadow** control → `link_foreshadow` patch | **Unlink Foreshadow** → `unlink_foreshadow` (escape hatch) or residual |
| **Graph** (optional) | `CanvasShell` `onConnect` between eligible event nodes | Edge context action or inspector-only minimum |

Linked edges render from `outline.foreshadows[]` using token `canvas-outline-foreshadow-edge`.

### Alt view vs Strategy pattern

| Layer | Strategy reference | Outline lock |
|-------|-------------------|--------------|
| Toggle state | `strategy-canvas.tsx` `showAlt` | `outline-canvas.tsx` `showAlt` |
| Toolbar | `strategy-canvas/canvas-layout.tsx` | `outline-canvas/canvas-layout.tsx` |
| Alt component | `strategy-alt-view.tsx` | `outline-alt-view.tsx` (P0-owned) |
| Default view | Graph for pointer users | Graph for pointer users |

P1 Studio `/surfaces/canvas` fixtures are **presentational preview only** — they do not implement alt-view behavior.

### Blast radius

**P0 owns (write):**

- `apps/web/src/components/canvas/outline-canvas/**` (new + modified modules)
- `apps/web/src/components/canvas/outline-canvas.tsx`
- `apps/web/src/pages/outline-page.tsx` (if layout wrapper changes)
- `apps/web/src/components/canvas/outline-canvas/__tests__/**`

**P0 consumes (read / no ownership):**

- `canvas-shell.tsx`, `outline-conflict-modal.tsx`, `use-outline-data.ts`, root `DESIGN.md` / `DESIGN.dark.md`

**P1 owns (parallel — do not edit in P0 branch):**

- `apps/design-studio/**`, `layout/presentational/**`, `setup/agent-picker.tsx`, `work-detail-page.tsx`, `world-kb-canvas-header.tsx`

**Coordination:** No shared hot files if branches stay file-disjoint. If P0 must touch `canvas-shell.tsx`, coordinate with PM — default is **no edit**.

---

## FB-C1-000 — Outline Opens as Spatial Canvas

**Problem:** Outline route renders a panel grid without `CanvasShell` or React Flow — authors cannot spatially navigate structure.

**User-visible outcome:** Authors opening Outline see a pannable/zoomable graph as the primary view.

### Acceptance

- [ ] `/works/:workId/outline` mounts `CanvasShell` with `@xyflow/react` as the main content area (not panel-only layout).
- [ ] Graph shows at least one Volume or Chapter node when outline data exists.
- [ ] Empty outline shows honest empty chrome inside the canvas shell (no fake graph nodes).
- [ ] Light and dark themes render graph chrome without invisible controls.

**SSOT:** `apps/web/src/components/canvas/outline-canvas.tsx` (+ modules under `outline-canvas/`)

---

## FB-C1-001 — Volume/Chapter Nodes With Status Paint

**Problem:** Without spatial nodes, chapter status (draft/revised/locked/etc.) is not visible at a glance on the graph.

**User-visible outcome:** Authors see Volume/Chapter cards on the graph with status-appropriate paint from DESIGN tokens.

### Acceptance

- [ ] Volume and Chapter entities project to distinct node types on the graph.
- [ ] Chapter status maps to the four shipped chapter-card token variants (`canvas-outline-chapter-*`).
- [ ] Volume fill uses `canvas-outline-volume-fill` (light+dark).
- [ ] Node labels remain readable (WCAG 2.1 AA contrast floor on fills).

**SSOT:** Outline node components; `DESIGN.md` / `DESIGN.dark.md` `canvas-outline-*` chapter + volume keys

---

## FB-C1-002 — Timeline Lane and Foreshadow Edges

**Problem:** Timeline events and foreshadow relationships are list-only — authors cannot see temporal structure spatially.

**User-visible outcome:** Timeline events appear on a lane; linked foreshadow relationships render as edges when data exists.

### Acceptance

- [ ] Timeline events project to lane/event nodes (or equivalent lane visualization).
- [ ] Foreshadow links render as edges using `canvas-outline-foreshadow-edge` when relationship data exists.
- [ ] Timeline markers/pins use `canvas-outline-timeline-event-pin` and `canvas-outline-timeline-marker` where applicable.
- [ ] Graph remains usable with zero foreshadow links (no orphan edge chrome).

**SSOT:** Outline graph projection + edge components

---

## FB-C1-003 — Selection Syncs Inspectors; OCC Conflict Modal Works

**Problem:** Spatial selection must drive the existing chapter/timeline inspectors without breaking patch semantics.

**User-visible outcome:** Clicking a graph node focuses the correct inspector; saving still uses OCC-safe patches; stale revisions show the outline conflict modal.

### Acceptance

- [ ] Node click updates inspector selection for the corresponding chapter/timeline entity.
- [ ] Inspector edits still call existing `outline.patch_structure` / `outline.patch_chapter` / `timeline.patch_event` routes.
- [ ] Stale revision returns 409 `OutlineConflictError`; outline-flavored conflict modal appears with retry/merge path.
- [ ] No regression in patch semantics vs pre-spatial outline UI.

**SSOT:** `outline-canvas.tsx`, inspectors, conflict modal wiring

---

## FB-C1-004 — Graph ↔ List Alt Toggle

**Problem:** Keyboard-first and accessibility paths need non-spatial lists; Strategy/World KB already ship this pattern.

**User-visible outcome:** Authors toggle between spatial graph and non-spatial chapter + timeline lists from the canvas toolbar.

### Acceptance

- [ ] Toolbar toggle switches graph ↔ alt view (chapter list + timeline list reachable in alt mode).
- [ ] Toggle labels: **Show list view** (from graph) and **Show graph** (from list) — match Strategy/World KB.
- [ ] Default view remains graph for pointer users.
- [ ] Alt view is keyboard-reachable (`aria-pressed` on toggle; no mouse-only trap).
- [ ] Alt lists remain sortable/equivalent to V1.72 non-spatial alternate views.

**SSOT:** Outline canvas toolbar / alt view components

---

## FB-C1-005 — Foreshadow Link/Unlink From UI

**Problem:** Foreshadow relationships exist in the data model but lack minimum authoring controls in the spatial UI.

**User-visible outcome:** Authors can create and remove foreshadow links between timeline events from the graph or inspector without raw JSON edits.

### Acceptance

- [ ] UI control to **Link Foreshadow** between eligible timeline events (minimum: inspector or graph connect gesture).
- [ ] UI control to **Unlink Foreshadow** for an existing link.
- [ ] Operations use existing `timeline.patch_event` (or structure ops) — no new wire unless blocked.
- [ ] Linked state visible on graph (FB-C1-002) after save.

**SSOT:** Event inspector / graph connect path; `timeline.patch_event` client

---

## FB-C1-006 — `canvas-outline-*` Tokens Applied

**Problem:** V1.72 shipped eight outline/timeline DESIGN tokens that the current UI does not consume.

**User-visible outcome:** Shipped outline node/edge chrome reads from DESIGN tokens — no unused-token gap for nodes/edges actually rendered.

### Acceptance

- [ ] All shipped outline node types consume their mapped `canvas-outline-*` token keys (light+dark).
- [ ] Foreshadow edges consume `canvas-outline-foreshadow-edge`.
- [ ] Conflict/highlight states use `canvas-outline-conflict-*` if conflict chrome ships in C1.
- [ ] No hard-coded one-off colors for elements covered by shipped tokens.
- [ ] Vitest and light/dark smoke notes in implement evidence.

**SSOT:** Outline node/edge components; root `DESIGN.md` / `DESIGN.dark.md`
