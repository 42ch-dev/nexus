# Canvas Scene/Beat Expansion (C2) — Primary Spec (V1.109 P0)

**Status:** Draft — product-complete (§5.1 product-manager)  
**Tier:** Must (P0)  
**Plan:** `2026-07-11-v1.109-canvas-scene-beat`  
**Compass:** `../v1.109-delivery-compass.md`  
**Normative master:** `.mstar/specs/canvas-strategy-surface.md` (§1 product thesis, §3.3 surface 2, §3.5 write boundary, §4 UX, §4.5 user stories)

## Product outcome

Authors can see and steer **Scene/Beat** structure spatially on the Outline canvas — not only Volume/Chapter. Opening Outline reveals scene-level beats nested inside chapter parents, so the structural map matches how authors think about a chapter (beats inside scenes inside chapters).

**User-visible win:** Outline graph depth matches the master projection claim (Work → Volume → Chapter → Scene/Beat). Authors click a Scene or Beat to inspect it; the list alt view nests Scene/Beat rows under their parent Chapter; chrome uses new `canvas-outline-scene-*` / `canvas-outline-beat-*` DESIGN tokens (light + dark).

## Problem

V1.108 shipped Outline **spatial C1** (Volume/Chapter + timeline lane + foreshadow min). The master spec (`canvas-strategy-surface.md` §1.2 / §3.3) already names Scene/Beat as outline projection nodes, but the App graph **stops at Chapter**. Authors steering long Works cannot see scene-level pacing or beat density on the canvas — the largest remaining outline spec↔implementation gap after C1.

## Product story

**Who:** Authors steering Works via the Outline canvas (structural map for the Work).

**Why spatial Scene/Beat matters:**
- A chapter is not a single blob — authors plan **scenes** (setting/moment blocks) and **beats** (turn points inside a scene).
- Linear lists hide density and nesting; spatial nesting under Chapter parents makes “where am I in this chapter?” visible at a glance.
- Steering Nexus (“revise this beat”, “expand this scene”) requires the author to **point at** the right structural unit — C1 only points at chapters.

**Narrative:** C2 deepens the Outline graph without inventing a fifth canvas surface. Scene/Beat nodes are **children of Chapter** parents (`parentId` + `extent: "parent"`), projected from existing outline/chapter data when available. Write ops for scene/beat stay deferred unless architect proves read-projection cannot satisfy inspector display.

## Goals

1. Scene and Beat nodes render **inside Chapter parent nodes** on the Outline spatial graph.
2. Scene/Beat chrome paints title + status from DESIGN tokens (`canvas-outline-scene-*` / `canvas-outline-beat-*`).
3. Selecting a Scene/Beat drives the side inspector (read-only if no write wire).
4. Alt list view includes Scene/Beat rows nested under parent Chapter (keyboard / SR path).
5. Light + dark token consumption verified; studio-first for any new presentational chrome.

## Non-goals

- Scene/Beat **write** operations (structured create/move/status patch) if read-projection satisfies inspector — deferred to next iteration unless §5.2 architect determines write is required for parity.
- Graph layout engine (dagre/elk) — deterministic parent-child grid remains.
- Steer-from-scene / Idea artifact from Scene/Beat nodes.
- Full shared command palette; Sidebar canvas IA.
- Strategy spatial `onConnect` (P1 owns); graph viewport/scale reliability (P2 owns).
- New fifth canvas domain surface; Manuscript/Findings/Memory as graphs.
- `preset` → `strategy` breaking rename; platform / cloud.

## Studio-first note

P0 lands App Outline Scene/Beat projection, inspector, alt rows, and token consumption. If Design Studio has outline surface fixtures, add scene/beat examples. Visual token accept: light + dark contrast on Scene/Beat fills/borders.

## Wire

**Locked (`wire_contracts_changed: false`):** Scene/Beat ships as a **fixture-driven read-projection**.

**Architect finding (§5.2 Q1):** The outline model carries **no scene/beat data**. Verified against:
- `work-outline.schema.json` — `volumes[]`, `timeline_events[]`, `foreshadows[]`, `chapter_titles{}`; no scenes/beats
- `chapter-summary.schema.json` — title/slug/wc/status/outline_path/body_path; no scenes/beats
- `chapter-outline.schema.json` — opaque `content: string` markdown prose
- `WorkChapterRecord` (nexus-local-db `work_chapters.rs`) — same field set as ChapterSummary

The "scene/beat" mentions elsewhere are different concepts: World KB `block_type` (entity taxonomy); narrative crate `Beats/beat-sheet.md` (screenwriting Script Beat Sheets). Neither is the outline Scene/Beat from master §3.3/§3.4.

| Concern | Mechanism | Wire risk |
|---------|-----------|-----------|
| Scene/Beat graph projection | **Fixture-driven**: Design Studio / test fixtures inject scene/beat payloads at the UI projection layer. On real Works (no scene/beat data today), chapters render with zero scene/beat children + honest empty chrome. | None — no wire needed; projection emits empty when data absent |
| Inspector display | Read injected scene/beat node data from RF selection | None for read-only |
| Scene/Beat write | **Deferred** | Future iteration when daemon models scenes/beats |

**Hierarchy model (§5.2 Q2):** Scene is a child of Chapter (`parentId = chapter:<n>`, `extent: "parent"`); Beat is a child of Scene (`parentId = scene:<id>`, `extent: "parent"`). Scene→Beat nesting matches master §3.4 `WorkNodeData.nodeKind: "scene" | "beat"`. Flat-under-chapter is NOT the model.

**Why not additive read DTOs now?** Adding `scenes[]`/`beats[]` to `WorkOutline` requires daemon-side modeling (where do scenes come from? chapter prose parsing? a new DB table? an author-facing create op?). That is a larger design that does not fit V1.109's Must scope alongside P1 wire work. The fixture path ships the full UI (nodes, inspector, alt rows, tokens) honestly now; the wire change is deferred with tracking.

## Continuity from V1.108

| Carry-forward | Source |
|---------------|--------|
| Spatial Outline shell + Volume/Chapter + timeline | V1.108 FB-C1-000..006 |
| Alt toggle copy **Show list view** / **Show graph** | V1.108 Voice lock |
| OCC conflict modal pattern | V1.72 / V1.108 |
| Studio-first invariant | V1.106+ |
| Residual R-V1108P0QC1-S001 (extract `useOutlineCanvasGraph`) | **Lands in P0** (§5.2 Q5 architect-locked — C2 complexity justifies the extract now; also unblocks R-V1108P1QC1-S002 studio import) |

---

## User stories (steering-loop style)

Aligned with `canvas-strategy-surface.md` §4.5 — author directs an autonomous executor; AI owns prose.

- **See scene-level structure on the map** — *As an author*, I open Outline and see Scene and Beat nodes nested inside each Chapter parent, so chapter pacing and beat density are visible without opening every inspector.
- **Inspect a beat without writing body** — *As an author*, I click a Scene or Beat node and the inspector shows title, status, and structural metadata for that unit, so I can decide where to steer Nexus next without typing chapter body prose.
- **Browse scenes from the list path** — *As an author* (keyboard-first or reduced-motion), I switch to **Show list view** and expand a Chapter row to see nested Scene/Beat rows, so accessibility and list productivity match the spatial graph.
- **Steer by pointing at structure** — *As an author*, after I identify the Scene/Beat I care about on the canvas, I use existing steering verbs (**Steer / Run / Resume / Ask Nexus to revise**) at Work or chapter scope (scene-scoped Idea deferred), so spatial visibility still feeds the autonomous loop.

---

## Voice & Content (locked)

Follow DESIGN.md §Voice & Content: **Title Case** for headings, labels, and CTAs; **sentence case** for helper text and empty-state body; **Verb + Noun** for actions. No protocol jargon (`parentId`, `extent`, DTO names) in author-facing copy.

| Surface | Element | Copy (exact) |
|---------|---------|--------------|
| Scene node | Default label fallback | **Untitled Scene** |
| Beat node | Default label fallback | **Untitled Beat** |
| Scene inspector | Panel heading | **Scene** |
| Beat inspector | Panel heading | **Beat** |
| Scene inspector | Status field label | **Status** |
| Beat inspector | Status field label | **Status** |
| Scene inspector | Parent chapter helper | *Part of {chapter_title}.* |
| Beat inspector | Parent scene helper | *Part of {scene_title}.* |
| Scene inspector | Read-only banner (if no write) | *Scene details are view-only for now.* |
| Beat inspector | Read-only banner (if no write) | *Beat details are view-only for now.* |
| Alt list view | Scene row type badge | **Scene** |
| Alt list view | Beat row type badge | **Beat** |
| Alt list view | Empty scenes under chapter | *No scenes in this chapter yet.* |
| Graph empty nested | Chapter with zero scenes | *(no extra chrome — chapter node alone is fine)* |
| Outline toolbar | Graph → list | **Show list view** (unchanged) |
| Outline toolbar | List → graph | **Show graph** (unchanged) |

**Forbidden in author-facing Scene/Beat UI:** `parentId`, `extent`, `OutlineSceneNodeData`, wire op names, “DTO”, “projection”.

---

## FB-C2-000 — Scene/Beat Nodes Inside Chapter Parents

**Problem:** Outline spatial graph stops at Chapter; authors cannot see scene/beat nesting on the map.

**User-visible outcome:** When chapter data includes scenes/beats (or projection fixtures provide them), Scene and Beat nodes appear **inside** their parent Chapter node bounds on the Outline canvas.

### What “render inside Chapter parent” means (product)

1. **Visual nesting:** Scene/Beat cards are drawn within the Chapter parent group/bounds — not as free-floating top-level nodes in the volume lane.
2. **Hierarchy semantics:** Collapsing or focusing a Chapter keeps Scene/Beat as children of that chapter (React Flow `parentId` + `extent: "parent"` is the implementation mechanism; authors experience “inside the chapter card/group”).
3. **Ordering:** Scenes stack in document order under the chapter; beats stack under their scene (or under chapter if model is flat beats — architect confirms).
4. **Zero children:** A chapter with no scenes/beats still renders as a Chapter node only — no fake empty Scene chrome.
5. **Zoom/pan:** Nested nodes remain selectable and readable at default zoom; no requirement for auto-layout engines.

### Acceptance

- [ ] Scene nodes project as children of the owning Chapter parent when scene data exists.
- [ ] Beat nodes project as children of their owning Scene (preferred) or Chapter (if model is flat) when beat data exists.
- [ ] Nested nodes are visually contained within the parent Chapter group/bounds on the graph.
- [ ] Chapters with zero scenes/beats do not invent placeholder Scene/Beat nodes.
- [ ] Graph remains usable when only Volume/Chapter data exists (C1 regression: Volume/Chapter/timeline still render).

**SSOT:** `outline-canvas/rf-projection.ts`, `outline-nodes.tsx` / `scene-beat-nodes.tsx`, `outline-canvas.tsx`

---

## FB-C2-001 — Scene/Beat Title + Status Token Paint

**Problem:** Nested structure without status paint does not help authors scan progress.

**User-visible outcome:** Scene and Beat nodes show **title** and **status** using DESIGN tokens `canvas-outline-scene-*` / `canvas-outline-beat-*` (not hard-coded one-off colors).

### Acceptance

- [ ] Scene node displays title (or **Untitled Scene** fallback) and status chip/paint when status exists in data.
- [ ] Beat node displays title (or **Untitled Beat** fallback) and status when status exists.
- [ ] Fill/border/status colors consume `canvas-outline-scene-fill`, `canvas-outline-scene-border`, `canvas-outline-scene-status-*`, `canvas-outline-beat-fill`, `canvas-outline-beat-border` (exact key set finalized with DESIGN.md in implement).
- [ ] Labels remain readable (WCAG 2.1 AA contrast floor on fills in light + dark).

**SSOT:** Scene/Beat node components; root `DESIGN.md` / `DESIGN.dark.md`; `@nexus/design-tokens`

---

## FB-C2-002 — Selection Drives Scene/Beat Inspector

**Problem:** Spatial nodes without inspector binding leave authors unable to read structural metadata after click.

**User-visible outcome:** Clicking a Scene or Beat node focuses the side inspector on that entity with locked headings and fields.

### What the inspector shows (product)

| Field / region | Scene inspector | Beat inspector |
|----------------|-----------------|----------------|
| Heading | **Scene** | **Beat** |
| Title | Scene title (read) | Beat title (read) |
| Status | Status value (read) | Status value (read) |
| Parent context | *Part of {chapter_title}.* | *Part of {scene_title}.* (or chapter if flat) |
| Write controls | Hidden if no write wire; show read-only banner | Same |
| Body / prose | **Not** full manuscript body — structural metadata only | Same |

### Acceptance

- [ ] Clicking a Scene node opens/updates inspector with heading **Scene** and title/status from node data.
- [ ] Clicking a Beat node opens/updates inspector with heading **Beat** and title/status from node data.
- [ ] Parent helper line uses locked copy with real parent title when available.
- [ ] If write is not shipped, inspector is read-only with locked banner copy (no broken Save control).
- [ ] Selecting a Chapter/Volume/Timeline node still drives existing inspectors (no regression).

**SSOT:** `outline-canvas/inspectors/scene-inspector.tsx`, `beat-inspector.tsx`, selection wiring in orchestrator

---

## FB-C2-003 — Alt List View Scene/Beat Rows

**Problem:** Keyboard-first and list-preferring authors need non-spatial access to the same hierarchy (§4.4).

**User-visible outcome:** **Show list view** includes Scene/Beat rows nested under their parent Chapter.

### What alt view rows look like (product)

1. **Hierarchy:** Chapter row remains primary; Scene rows indent under chapter; Beat rows indent under scene (or under chapter if flat).
2. **Columns (minimum):** Type badge (**Scene** / **Beat**), Title, Status — reuse chapter list column rhythm where possible (title / status / updated if available).
3. **Empty:** Chapter with no scenes shows helper *No scenes in this chapter yet.* under that chapter — not a global empty state if other chapters have scenes.
4. **Selection:** Activating a Scene/Beat row drives the same inspector as graph selection (or focuses equivalent detail).
5. **Toggle copy:** Unchanged **Show list view** / **Show graph**.

### Acceptance

- [ ] Alt chapter list nests Scene rows under parent Chapter when data exists.
- [ ] Beat rows nest under Scene (preferred) or Chapter (if flat model).
- [ ] Type badges use locked **Scene** / **Beat** copy.
- [ ] Empty-under-chapter helper uses locked copy when a chapter has zero scenes.
- [ ] Keyboard can reach nested rows (no mouse-only expand trap); focus-visible rings present.
- [ ] Timeline event list in alt view remains available (C1 regression).

**SSOT:** `outline-canvas/outline-alt-view.tsx`

---

## FB-C2-004 — Light + Dark Token Consumption

**Problem:** New node types must not introduce hard-coded palette drift.

**User-visible outcome:** Scene/Beat chrome consumes DESIGN tokens in both themes.

### Acceptance

- [ ] Scene/Beat node fills, borders, and status paints use DESIGN tokens (no one-off hex for covered elements).
- [ ] Light and dark themes both render readable Scene/Beat chrome.
- [ ] Tokens added to `DESIGN.md` + `DESIGN.dark.md` + `tokens.css` before App consumption.
- [ ] Implement evidence notes light+dark smoke (Studio or App).

**SSOT:** DESIGN tokens + Scene/Beat node components

---

## Definition of Done (product)

- All FB-C2-000..004 acceptance checkboxes satisfy App Outline on a Work that has (or fixtures that provide) multi-level outline data.
- C1 regressions: Volume/Chapter/timeline, alt toggle, foreshadow min, OCC conflict path still work.
- Prefer `wire_contracts_changed: false`; any additive wire documented with escape-hatch evidence.
- Non-goals remain unshipped (no scene write unless architect lock requires it).

## Roadmap / deferred (tracked)

| Deferred item | Trigger | Owner |
|---------------|---------|-------|
| Additive `scenes[]`/`beats[]` read DTOs on `WorkOutline` (wire change) | Daemon models scenes/beats (chapter-prose parse, new DB table, or author create op) | `@architect` + `@product-manager` |
| Scene/Beat structured write (create/move/status) | Additive read DTOs shipped + author demand / parity need | `@product-manager` + `@architect` |
| Scene-scoped Idea / Steer affordance | After write or stable scene ids | next iteration |
| Layout engine for deep nesting | Real Works with large scene counts + P2 scale learnings | post-V1.109 |

## Effort (agent-oriented)

Medium–Large plan (4 SDD tasks): tokens+nodes → RF projection → inspector+alt → integration/studio. Depends on architect data-source answer; blocked only if no projection path and no residual strategy.
