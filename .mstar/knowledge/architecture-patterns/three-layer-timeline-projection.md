---
module: canvas
date: 2026-07-19
problem_type: architecture-pattern
category: architecture-patterns
severity: low
tags: [canvas, timeline, three-layer, adapter, brief, narrative, moment, world, work, v1.123]
applies_when: Extending a Canvas surface with multiple zoom layers (Brief/Narrative/Moment) or projecting a domain timeline at multiple granularities
---

# Three-Layer Timeline Projection Pattern

## Context

- **World Timeline** (V1.122 peer surface, V1.123 deepened) — **Brief + Narrative + Moment** layers (V1.156 completed the matrix; V1.123 shipped Brief+Narrative only)
- **Work Timeline** (V1.123 NEW peer surface) — **Brief + Narrative + Moment** layers (V1.156 completed the matrix; V1.123 shipped Narrative+Moment only)

The carrier choices are architect-locked per iteration (architect-locked in the V1.123 iteration spec `three-layer-architecture.md`):

| Layer | Carrier (V1.123 LOCK) | Rationale |
|-------|----------------------|-----------|
| Brief | Brief-on-KnowledgeEntry via `BlockType = "era"` (additive wire enum) | Lowest wire-contract churn; reuses V1.73 `kb.patch_entity` + World KB conflict DTOs |
| Narrative | `block_type=event` KnowledgeEntries (World) + V1.72 outline `timeline_events` (Work) | V1.122 baseline preserved; shared between World and Work |
| Moment | Moment-on-Outline (frontend-only projection; V1.108 `OutlineSceneNodeData`/`OutlineBeatNodeData`) | Zero wire-contract churn for Moment; V1.72 WorkOutline wire has no scene/beat data today (DF-V1123-MOMENT-WIRE deferred to V1.124+) |

## Guidance

When extending a Canvas surface with multi-layer projection:

1. **One surface adapter, multiple layer projections.** Each Canvas surface has ONE adapter conforming to V1.114 `CanvasSurfaceAdapter`; the adapter exposes `projectGraphForLayer(graph, layer)` instead of (or in addition to) `projectGraph(graph)`.

2. **Stable-factory pattern for layer swap.** Adapter factory `create<Surface>CanvasAdapter(ctxRef, activeLayer)` rebuilds on layer change; the orchestrator (`<Surface>Canvas`) memoizes on `activeLayer` and feeds the new adapter to `useCanvasSurface()` for clean re-projection.

3. **Default-layer logic per surface.** Default layer is data-driven, not hard-coded:
   - World Timeline: `'brief'` if `block_type=era` data exists, else `'narrative'` fallback
   - Work Timeline: `'narrative'` unconditionally per UX-risk override (V1.72 wire has no scene/beat data today; Moment-default would surface persistent empty-state)

4. **URL `?layer=` persistence.** Layer state survives surface switches via URL query (`?layer=brief|narrative|moment`). **V1.156 lifted the V1.123 surface-layer restriction** — all three layers are now valid on both surfaces (`moment` on World and `brief` on Work, previously ignored, are now valid as of V1.156 P1/P2). Default layer drops the URL param so graph-driven defaults can track changes. **Lesson (V1.156 P1 QC):** when lifting a layer restriction, add a test that an invalid `?layer=` value still falls back to default (don't repurpose the old invalid-test slot for the now-valid value — re-pin the null branch).

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

The carrier choices (Brief-on-KnowledgeEntry, Moment-on-Outline) prove that **additive wire-contract changes** can unlock rich multi-layer UX: only one wire enum value (`era` in `BlockType`) was added in V1.123; P2/P3/P4 added zero wire diff.

## When to Apply

- Adding a new Canvas surface that should project at multiple zoom layers
- Promoting DF-V1123-WORLD-MOMENT or DF-V1123-WORK-BRIEF (**shipped V1.156** — both matrix cells complete)

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

- **V1.156 3×2 matrix completion**: P1 (World×Moment) + P2 (Work×Brief) shipped, making all three layers valid on both surfaces. Both are frontend-only (`wire_contracts_changed:false`): World-Moment = read/projection of bound Works' `OutlineSceneNodeData`/`OutlineBeatNodeData` (fixture-driven; DR-26 tracks the WorkOutline wire extension to real scene/beat data); Work-Brief = projection of bound World's `block_type=era` entities via V1.73 `kb/graph`. Read-only inspectors (PD-3/PD-2 — no `kb.patch_entity` write path from projected-layer nodes). See V1.156 compass + product-locks (PD-2/PD-3).
- **V1.156 QC carry-forward lesson**: P1's QC fix-wave (read-only inspector W-1, alt-view crash W-2, memo-deps F-3, invalid-layer test F-4) was baked into P2's brief proactively → P2 needed only one converged fix (graph-query status gate). See `knowledge/workflow-patterns/carry-qc-lessons-to-sibling-plan.md`.
