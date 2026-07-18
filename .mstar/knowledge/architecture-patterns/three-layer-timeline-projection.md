---
module: canvas
date: 2026-07-19
problem_type: architecture-pattern
category: architecture-patterns
severity: low
plan_id: 2026-07-18-v1.123-three-layer-timeline-spec
tags: [canvas, timeline, three-layer, adapter, brief, narrative, moment, world, work, v1.123]
applies_when: Extending a Canvas surface with multiple zoom layers (Brief/Narrative/Moment) or projecting a domain timeline at multiple granularities
---

# Three-Layer Timeline Projection Pattern

## Context

V1.123 (Three-Layer Timeline iteration) canonized Brief / Narrative / Moment as Timeline's three zoom layers and projected them onto **two distinct Canvas surfaces** with **domain-differentiated layer use**:

- **World Timeline** (V1.122 peer surface, V1.123 deepened) — leads with **Brief + Narrative** layers
- **Work Timeline** (V1.123 NEW peer surface) — leads with **Narrative + Moment** layers

The carrier choices are architect-locked per iteration (`iterations/v1.123/specs/three-layer-architecture.md`):

| Layer | Carrier (V1.123 LOCK) | Rationale |
|-------|----------------------|-----------|
| Brief | Brief-on-KeyBlock via `BlockType = "era"` (additive wire enum) | Lowest wire-contract churn; reuses V1.73 `kb.patch_entity` + World KB conflict DTOs |
| Narrative | `block_type=event` KeyBlocks (World) + V1.72 outline `timeline_events` (Work) | V1.122 baseline preserved; shared between World and Work |
| Moment | Moment-on-Outline (frontend-only projection; V1.108 `OutlineSceneNodeData`/`OutlineBeatNodeData`) | Zero wire-contract churn for Moment; V1.72 WorkOutline wire has no scene/beat data today (DF-V1123-MOMENT-WIRE deferred to V1.124+) |

## Guidance

When extending a Canvas surface with multi-layer projection:

1. **One surface adapter, multiple layer projections.** Each Canvas surface has ONE adapter conforming to V1.114 `CanvasSurfaceAdapter`; the adapter exposes `projectGraphForLayer(graph, layer)` instead of (or in addition to) `projectGraph(graph)`.

2. **Stable-factory pattern for layer swap.** Adapter factory `create<Surface>CanvasAdapter(ctxRef, activeLayer)` rebuilds on layer change; the orchestrator (`<Surface>Canvas`) memoizes on `activeLayer` and feeds the new adapter to `useCanvasSurface()` for clean re-projection.

3. **Default-layer logic per surface.** Default layer is data-driven, not hard-coded:
   - World Timeline: `'brief'` if `block_type=era` data exists, else `'narrative'` fallback
   - Work Timeline: `'narrative'` unconditionally per UX-risk override (V1.72 wire has no scene/beat data today; Moment-default would surface persistent empty-state)

4. **URL `?layer=` persistence.** Layer state survives surface switches via URL query (`?layer=brief|narrative|moment`). Invalid values for the surface are ignored (`moment` on World; `brief` on Work). Default layer drops the URL param so graph-driven defaults can track changes.

5. **Per-layer layout options.** Each layer has its own dagre layout direction + spacing:
   - Brief: LR sweep (wide `rankSep` ~240, small `nodeSep` ~40) — world-shape-at-a-glance feel
   - Narrative: LR balanced (V1.122 baseline ~80/24) — events-in-order feel
   - Moment: TB stack (tight `rankSep` ~60, `nodeSep` ~30) — scene-precision feel

6. **Per-layer accent tokens.** Register `--color-canvas-layer-{brief,narrative,moment}-accent` in tokens.css + Tailwind preset + DESIGN.md (light + dark). Reuse existing palette (don't invent new colors): Brief = amber/gold (era/age), Narrative = neutral (events), Moment = ink-on-paper (scenes).

7. **Semantic zoom ≠ viewport zoom.** Use `useSemanticZoom` hook to observe React Flow viewport (`useViewport()` / `onViewportChange`) and emit discrete layer-swap events at thresholds (e.g., 0.55–0.70 band). Layer swap is a discrete transition (CSS keyframe or Framer Motion), not continuous viewport zoom.

8. **First-observation skip.** `useSemanticZoom` must ignore the first viewport observation to prevent mount-time layer bounce.

9. **Honest empty-state per layer.** Each layer has its own empty-state copy in i18n (en + zh-CN):
   - Brief empty: "No era markers yet — switch to Narrative to see events."
   - Narrative empty: V1.122 baseline
   - Moment empty: "No scene or beat data yet — switch to Narrative to see events."

10. **Cross-surface navigation hooks.** Declare adapter-context hooks for cross-surface navigation (`onViewOnWorldTimeline`, `onViewInWorkTimeline`) even if unwired in the surface's own iteration. P3 (IA) wires them later via bidirectional CTAs.

## Why This Matters

The three-layer projection pattern lets an author see the **same Timeline at three scales** (world shape / events / scenes) without inventing three separate Canvas surfaces. It also encodes the **domain-differentiated layer use** (World leads from Brief; Work leads from Moment) as a product thesis, not just a UX accident.

The carrier choices (Brief-on-KeyBlock, Moment-on-Outline) prove that **additive wire-contract changes** can unlock rich multi-layer UX: only one wire enum value (`era` in `BlockType`) was added in V1.123; P2/P3/P4 added zero wire diff.

## When to Apply

- Adding a new Canvas surface that should project at multiple zoom layers
- Extending an existing Canvas surface with a new layer (e.g., promoting DF-V1123-WORLD-MOMENT or DF-V1123-WORK-BRIEF in V1.124+)
- Designing cross-surface navigation between layers (e.g., a Future surface that should link to a specific layer on another surface)
- Migrating a wire-only data carrier to a wire+DTO carrier (e.g., DF-V1123-MOMENT-WIRE when V1.124+ adds scene/beat wire data)

## Examples

### V1.123 P1 — World Timeline Brief+Narrative

`apps/web/src/components/canvas/timeline-canvas/timeline-canvas-adapter.tsx`:
- `TimelineLayer = 'brief' | 'narrative'`
- `projectBriefLayer` filters `block_type === 'era'`; positions LR by `body.attributes.start_hint`
- `projectNarrativeLayer` is V1.122 baseline (events on when-axis)
- `createTimelineCanvasAdapter(ctxRef, activeLayer)` factory; default `'brief'` if era data exists, else `'narrative'`

### V1.123 P2 — Work Timeline Narrative+Moment

`apps/web/src/components/canvas/work-timeline-canvas/work-timeline-canvas-adapter.tsx`:
- `WorkTimelineLayer = 'narrative' | 'moment'`
- `projectNarrativeLayer` reads V1.72 outline `timeline_events[]`
- `projectMomentLayer` reads V1.108 `OutlineSceneNodeData`/`OutlineBeatNodeData` fixture
- `createWorkTimelineCanvasAdapter(ctxRef, activeLayer)` factory; default `'narrative'` (UX-risk override)

### V1.123 P4 — Semantic zoom + layer-state persistence

`apps/web/src/components/canvas/use-semantic-zoom.ts`:
- `useSemanticZoom({ thresholds, onLayerChange })` observes React Flow viewport
- `SemanticZoomBridge` host component (orchestrator runs outside RF provider)
- First-observation skip prevents mount-time layer bounce

`apps/web/src/components/canvas/timeline-canvas/timeline-canvas.tsx` + `work-timeline-canvas.tsx`:
- Layer state via `useSearchParams` (`?layer=` URL query)
- `handleLayerChange` single callback consumed by layer tabs + breadcrumb + semantic zoom bridge

## References

- V1.123 architecture LOCK: `iterations/v1.123/specs/three-layer-architecture.md`
- V1.123 layer feel contract: `iterations/v1.123/specs/layer-feel-differentiation.md` (also promoted to `knowledge/conventions/three-layer-timeline-feel.md`)
- V1.122 surface extraction pattern: `knowledge/architecture-patterns/canvas-surface-extraction-pattern.md` (V1.123 extends with multi-layer)
- V1.114 Canvas adapter recipe: `specs/canvas-strategy-surface.md` §3.3.1
- Wire-contracts frozen verification: `knowledge/conventions/wire-contracts-frozen-verification.md`
