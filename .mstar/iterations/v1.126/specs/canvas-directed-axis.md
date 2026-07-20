# Spec — Canvas directed center axis (V1.126 P1)

**Status:** product-reviewed, architect-locked, writing-hygiene done (Phase 1 §1.6 seat 3 inline fallback — empty subagent response; PM applied flagged hygiene per V1.124 pattern)
**Document class:** Iteration package working spec (not `{SPECS_DIR}` Master)
**Compass:** [`../delivery-compass.md`](../delivery-compass.md) — AC-V1126-2
**Plan:** [`2026-07-20-v1.126-p1-canvas-directed-axis`](../../../plans/2026-07-20-v1.126-p1-canvas-directed-axis.md)
**Wire contracts:** `wire_contracts_changed: false` — visual decoration on existing `when-axis`; existing `canvas-layer-{brief,narrative,moment}-accent` tokens consume.

## Problem

V1.123 P1 / P2 / P4 shipped three Timeline layers (Brief / Narrative / Moment) all projecting onto the same flat `when-axis` (Y=0). The three layers differ in **what** they project (eras / events / scene-beats) but not in **how the axis itself looks** — they share a single undifferentiated horizontal line. V1.125 Non-Goal "Canvas Brief/Narrative directed center axis" is the deferred visual bet: each layer gets a layer-specific directed spine so a glance reveals the active layer's scale and arrow-of-time. The `canvas-layer-{brief,narrative,moment}-accent` tokens already exist (V1.123 P3 T2 + V1.124 P1 gallery) but have no live consumer.

## Normative decisions (PM initial — pending seat 1/2/3)

1. **Directed axis = visual decoration on existing geometry.** The `when-axis` (Y=0) and its layer-specific projection logic (V1.123 `timeline-canvas-adapter.tsx::projectBrief/projectNarrative` + `work-timeline-canvas-adapter.tsx::projectMoment`) stay unchanged. P1 **adds** a layer-differentiated spine **on top of** the axis, it does **not** re-project.
2. **Brief layer spine — directed era-spanning arrow.** A thick left-to-right arrow runs along Y=0 with gradient ticks at era `start_hint` / `end_hint`. Arrow head at the right end. Color: `--color-canvas-layer-brief-accent` (V1.124 P1 gallery token, V1.123 P3 T2 introduced). Tick labels (era names) sit above the arrow.
3. **Narrative layer spine — discrete event-pin axis.** A thin connecting line at Y=0 with discrete tick marks at each event `timestamp`. Ticks below the line for undated events (existing cluster pattern preserved). Color: `--color-canvas-layer-narrative-accent`.
4. **Moment layer spine — chapter/scene-scoped micro-axis.** Work Timeline only. Short directed segments per chapter; ticks per scene + beat. Color: `--color-canvas-layer-moment-accent`. Chapter labels sit above each segment. **Segment layout:** Architect seat 2 ratifies whether segment **length is proportional to scene count** (density-encoded — flagged for ratification: this conflicts with the chronological-axis convention used by Brief + Narrative, where segment length encodes time span. Alternative: uniform-length segments with density encoded by tick density, not segment width. Product has no preference as long as the three layers read at-a-glance as different visual languages).
5. **V1.123 P4 semantic zoom preserved.** The directed-axis is a **constant** decoration — it does **not** morph between zoom bands. The V1.123 P4 zoom thresholds (0.55–0.70) and layer-swap factory rebuild pattern stay; the directed spine re-renders on layer swap (same lifecycle as today).
6. **Studio-first policy.** Studio Canvas Surfaces fixtures (V1.124 P0) gain the directed-axis treatment per layer in light + dark. P1 ships Studio + App in the same iteration (V1.106 invariant).
7. **Visual differentiation threshold (product-locked at seat 1).** The three layer spines must be **perceptually distinct at a glance** in the Studio gallery side-by-side — not just token-color-different. "Thick era arrow" (Brief) vs "thin event-pin line" (Narrative) vs "chapter micro-segments" (Moment) gives three different visual rhythms; if Studio review shows two layers reading as the same rhythm with different color, that is a defect (re-cut before P1 ship).
8. **No new tokens.** P1 consumes the three `canvas-layer-*-accent` tokens already in `tokens.css`. NG-9 (compass): do not invent tokens to fill the gallery — the gallery is already complete per V1.124 P1.
9. **Extraction rule.** If T1+T2+T3 produce a single reusable `DirectedAxisSpine` component consumed by World Timeline Brief + World Timeline Narrative + Work Timeline Moment (≥ 3 consumers), promote to `@web-canvas/directed-axis-spine` (new alias root) per V1.106 rule. If extraction is single-consumer glue, keep app-local with header comment.

## Concerns for architect seat 2

- **Moment spine layout convention.** Brief + Narrative both encode **time span** by segment length (chronological axis convention). Moment "length proportional to scene count" (ND-4) encodes **density** instead — a deliberate rhythm break that signals "you are now at scene precision, not event precision". Architect ratifies: keep the rhythm break (product recommendation — supports the layer differentiation threshold ND-7) OR fall back to uniform-length segments with tick-density encoding.
- **Spine rendering host.** React Flow custom node vs background renderer (architecture notes row 2). Architect decides based on existing V1.123 P4 layer-swap factory; product has no preference as long as layer swap rebuild lifecycle is preserved.
- **`projectMoment` source data.** Today the Work Outline wire does not expose scenes/beats (V1.108 fixture-only — `DF-V1123-MOMENT-WIRE`). The Moment directed spine reads the same fixture/UI-projection data as the existing Moment layer nodes — no wire change in P1, but the spine visually demonstrates the empty-state gap in real Works. This is honest (V1.123 spec §3 "honest empty-state") and aligns with the V1.126 Non-Goal NG-8.

## Architecture locks (architect seat 2)

> Ratified 2026-07-20. All AQ verdicts are final — implementers treat these as non-negotiable architecture contracts.

### ND-A1 — Moment spine layout convention (AQ-4)

- **Density-encoding (length proportional to scene count) is LOCKED.** This is a deliberate rhythm break from Brief + Narrative's time-span-by-segment-length convention.
- **Rationale:** (a) The three spines MUST be perceptually distinct per ND-7 — encoding density on Moment while Brief+Narrative encode time-span achieves this. (b) Density signals "you are at scene precision, not event/era precision" — the visual language itself communicates the layer's granularity. (c) Uniform-length segments would risk conflating with the Narrative discrete-pin rhythm, violating the differentiation threshold.
- **Documented as intentional divergence** — not a defect, a feature. The implementer adds a comment in the Moment spine renderer explaining the convention break.
- **Chapter segments** are ordered by the chapter's position in the outline (ascending); within a chapter, ticks are positioned proportionally by scene/beat index. Empty chapters with no scenes show as a short stub segment (different from the full-length segment of a populated chapter).

### ND-A2 — Spine rendering host (AQ-5)

- **React Flow custom node is LOCKED.** The `DirectedAxisSpine` is registered as a React Flow custom node type (e.g., `directedAxisSpine`).
- **Rationale:** The spine shares the React Flow canvas lifecycle — it has access to the viewport transform via `useReactFlow()`, participates in the same re-render cycle as existing layer nodes (`TimelineEventNode`, `TimelineKeyBlockNode`, `TimelineBriefEraNode`), and benefits from React Flow's render batching. The spine is a non-interactive decoration (no handles, no selection, no drag).
- **Rejected alternative:** Background SVG overlay would require separate zoom-sync wiring (sync `useViewport()` to an independent SVG transform), would sit outside React Flow's render cycle, and would be a novel rendering path that complicates the V1.123 P4 factory-rebuild pattern (layer swap must destroy+recreate both the React Flow nodes AND the background SVG layer).
- **Custom node contract:**
  ```ts
  // Registered as nodeTypes.directedAxisSpine in the ReactFlow component
  interface DirectedAxisSpineNodeData {
    layer: 'brief' | 'narrative' | 'moment';
    spineConfig: BriefSpineConfig | NarrativeSpineConfig | MomentSpineConfig;
    accentColor: string; // CSS variable reference, e.g. 'var(--color-canvas-layer-brief-accent)'
  }
  ```
- The spine node occupies a single React Flow node slot positioned at the axis origin (Y=0). Its renderer draws the full horizontal spine as internal SVG within the node's DOM element. The node's width is set to the viewport width; the viewport transform handles zoom/pan automatically.

### ND-A3 — Decoration-only invariant (LOCKED)

- The directed-axis spine is rendered as an **overlay/decoration** on the existing `when-axis` (Y=0). The V1.123 `projectBrief` / `projectNarrative` / `projectMoment` projection logic is **not** altered.
- Each projection function **adds** a new `directedAxisSpine` field to its projection result:
  ```ts
  // Timeline projection result (additive field — architect-locked)
  interface TimelineProjectionResult {
    nodes: Node<TimelineNodeData>[];
    edges: Edge<TimelineEdgeData>[];
    directedAxisSpine: DirectedAxisSpineNodeData | null; // NEW — null when layer has no spine data
  }
  ```
- The existing node/edge arrays are unchanged. The adapter's `projectGraphForLayer` returns the existing nodes + edges + the spine node (if non-null). No re-projection, no coordinate translation, no node relocation.

### ND-A4 — V1.123 P4 semantic-zoom + factory-rebuild preservation (LOCKED)

- **Semantic zoom:** The directed-axis spine is a **constant** decoration — it does **not** change appearance, density, or visibility between zoom bands. The V1.123 P4 zoom thresholds (brief↔narrative 0.70, narrative↔brief 0.55 — per `canvas-strategy-surface.md` §3.3.3) and the layer-switcher segmented control are preserved. The spine is always visible when its layer is active, regardless of zoom level.
- **Factory rebuild:** The `createTimelineCanvasAdapter(ctxRef)` + `createWorkTimelineCanvasAdapter(ctxRef)` stable-factory pattern (V1.114 §3.3.1 + V1.123 §7.1) is preserved. The `DirectedAxisSpine` custom node type is registered in the adapter's `nodeTypes` registry. On layer swap, the adapter destroys the current projection (including the spine node) and rebuilds from the new layer's projection data — same lifecycle as V1.123 P4.
- **Verified against:** `canvas-strategy-surface.md` §3.3.3 layer-swap contract; `three-layer-architecture.md` §7 adapter contract; `apps/web/src/components/canvas/timeline-canvas/timeline-canvas-adapter.tsx` line 1–50 header doc.

### ND-A5 — Extraction rule

- **Extract to `@web-canvas/directed-axis-spine`** (new alias root) when the spine component has **≥ 3 consumers** across World Timeline Brief + Narrative + Work Timeline Narrative + Moment.
- If only 1–2 consumers (e.g., the Moment micro-axis is structurally different from the Brief arrow and Narrative pin axis), keep the spine renderer(s) app-local in `apps/web/src/components/canvas/timeline-canvas/directed-axis-spine.tsx` and `work-timeline-canvas/directed-axis-spine.tsx` with header comments explaining the visual contract.
- **Extraction scope:** the visual spine renderer only (custom node component). Layer-specific projection data (`BriefSpineConfig`, `NarrativeSpineConfig`, `MomentSpineConfig`) and the adapter-level node registration stay in `apps/web`.

### ND-A6 — Studio fixture boundary

- Extend V1.124 P0 Studio Timeline fixtures (`apps/design-studio/src/fixtures/canvas-surfaces-fixtures.tsx`): each layer shows its directed spine. Light + dark. All three layer spines visible side-by-side (per ND-7 visual differentiation threshold).
- **Studio review is the gate:** if Brief + Narrative + Moment read as the same rhythm with different color, that is a defect — re-cut before P1 ship.

### ND-A7 — Wire contracts verdict

- **`wire_contracts_changed: false` — CONFIRMED.** Visual decoration on existing `when-axis` geometry; existing `canvas-layer-{brief,narrative,moment}-accent` tokens consumed from `tokens.css` (V1.124 P1). No new DTOs, no daemon changes, no codegen.

## Architecture notes (implementer)

| Component | Change |
|-----------|--------|
| `apps/web/src/components/canvas/timeline-canvas/timeline-canvas-adapter.tsx` | `projectBrief` + `projectNarrative` emit a new `directedAxisSpine` field on the projection result (ND-A3). `projectGraphForLayer` returns the spine node alongside existing nodes/edges. Brief spine config: `{ type: 'brief', eras: EraSpineTick[] }`. Narrative spine config: `{ type: 'narrative', events: EventSpineTick[] }`. Existing node/edge arrays unchanged. |
| New `apps/web/src/components/canvas/timeline-canvas/directed-axis-spine.tsx` (or `@web-canvas/directed-axis-spine` per ND-A5) | React Flow custom node (`type: 'directedAxisSpine'`) that renders the directed spine. Consumes `--color-canvas-layer-*-accent` tokens via Tailwind. **Renderer host: React Flow custom node (ND-A2).** Three spine render paths: Brief horizontal arrow with era gradient ticks; Narrative discrete event-pin axis; Moment chapter-scoped micro-segments with density-encoding (ND-A1). |
| `apps/web/src/components/canvas/timeline-canvas/timeline-canvas.tsx` | Register `directedAxisSpine` in `nodeTypes`; render based on active layer (ND-A4 — factory rebuild on layer swap) |
| `apps/web/src/components/canvas/work-timeline-canvas/work-timeline-canvas-adapter.tsx` | Same: project directed-spine data for Narrative + Moment (ND-A3). Narrative spine: discrete pin axis. Moment spine: chapter micro-segments with density-encoding (ND-A1). |
| `apps/web/src/components/canvas/work-timeline-canvas/work-timeline-canvas.tsx` | Register `directedAxisSpine` in `nodeTypes`; render based on active layer |
| `apps/design-studio/src/fixtures/canvas-surfaces-fixtures.tsx` | Extend V1.124 P0 Timeline fixtures: each layer shows its directed spine. Light + dark. All three spines visible side-by-side (ND-A6 — Studio review is the visual differentiation gate). |
| `apps/design-studio/src/pages/surfaces.tsx` | Legend explaining the three layer spines + Moment density-encoding convention (ND-A1) |

### Architecture locks

- **T1 (Brief spine):** React Flow custom node (ND-A2). Brief arrow spine consumes `--color-canvas-layer-brief-accent`. Era nodes attach on top. Context cluster nodes stay above the axis. No semantic zoom regression (ND-A4).
- **T2 (Narrative spine):** Same custom node component, narrative variant. Discrete pin axis with tick marks at event timestamps. Undated events cluster below with off-axis tick row (existing pattern preserved).
- **T3 (Moment + Work-Narrative):** Moment spine uses density-encoding (ND-A1 — deliberate rhythm break). Work-Narrative spine matches World-Narrative (same discrete pin axis). T3 shares the spine component with T1+T2; component reuse may trigger extraction per ND-A5.
- **T4 (Studio):** Extend V1.124 P0 fixtures (ND-A6). Studio review is the visual-differentiation gate. `wire_contracts_changed: false` (ND-A7).

## Acceptance (author-observable)

| ID | Author sees / does |
|----|-------------------|
| AC-V1126-2 | Each Timeline layer has a differentiated directed center axis; Studio fixtures show all three; V1.123 P4 semantic zoom + layer-swap factory rebuild preserved |

## Out of scope

Continuous-zoom axis morphing (V1.123 P4 residual "adapter factory rebuild pattern does not scale to continuous zoom" stays roadmap); new design tokens; Moment-on-wire migration (`DF-V1123-MOMENT-WIRE`); World Timeline Moment layer (`DF-V1123-WORLD-MOMENT`); Work Timeline Brief layer (`DF-V1123-WORK-BRIEF`); World-scoped `TimelineEvent` route promotion (`DF-V1122-DEEPER-WB` remainder — full per-World row access stays open separately from P2 overview slice); Work-scoped `GET /v1/daemon/works/{work_id}/timeline` route (NG-13 — rejected V1.123 alternative).
