---
module: canvas
date: 2026-07-19
problem_type: design-pattern
category: conventions
severity: low
plan_id: 2026-07-18-v1.123-three-layer-zoom-experience
tags: [canvas, timeline, three-layer, feel, layout, visual-language, semantic-zoom, v1.123]
applies_when: Designing per-layer visual/layout/zoom feel differentiation for a multi-layer Canvas surface
---

# Three-Layer Timeline Feel Differentiation

## Context

V1.123 caller mandate: "三层不一样的感受" — Brief/Narrative/Moment must each have a perceptibly distinct feel (not just different data). V1.123 P4 (Three-Layer Zoom Experience Differentiation) implemented this on top of P1/P2 layer abstractions.

## Guidance

### Per-layer feel contract

| Layer | Layout | Density | Visual language | Default zoom | Accent token |
|-------|--------|---------|-----------------|--------------|--------------|
| **Brief** | Horizontal era sweep (LR); wide `rankSep` (~240); small `nodeSep` (~40) | Minimal (era markers only); no Context clusters | Era-icon (Hourglass) + time-span label + era-id pill + world-summary line | Far (whole-world view) | `--color-canvas-layer-brief-accent` (amber/gold — age/era) |
| **Narrative** | Balanced event timeline (LR); V1.122 baseline `rankSep` ~80; `nodeSep` ~24 | Balanced (event + context + edges) | Event-icon + occurred-at + relationship edges (V1.122 visual) | Medium (event-level view) | `--color-canvas-layer-narrative-accent` (neutral — may stay dormant) |
| **Moment** | Vertical scene-stack (TB); tight `rankSep` (~60); `nodeSep` (~30) | Dense (scene/beat cards per chapter); manuscript-anchored | Scene-icon + beat-pin + manuscript-link badge | Close (scene-level view) | `--color-canvas-layer-moment-accent` (ink-on-paper) |

### Semantic zoom (NOT viewport zoom)

- Brief↔Narrative zoom on World Timeline: discrete layer swap at viewport thresholds (0.55–0.70 band)
- Narrative↔Moment zoom on Work Timeline: same pattern
- Layer swap is a discrete transition (CSS keyframe 240ms opacity+scale OR Framer Motion `AnimatePresence`); NOT continuous viewport zoom

### First-observation skip

`useSemanticZoom` must ignore the first viewport observation to prevent mount-time layer bounce. This is essential — without it, the layer swap fires on mount and the canvas flickers.

### Layer transition animation

- CSS keyframe fallback (V1.123 P4 chose this per `layer-feel-differentiation.md` §4): 240ms opacity+scale; no new runtime dependency
- Framer Motion `AnimatePresence` is the spec-preferred option but adds a runtime dep; defer to P5+ if richer animation needed

### Layer-state persistence

- URL `?layer=brief|narrative|moment` is the SSOT
- Invalid values for the surface are ignored (`moment` on World; `brief` on Work)
- Default layer drops the URL param so graph-driven defaults can track changes
- `handleLayerChange` is the single callback consumed by layer tabs + breadcrumb + semantic zoom bridge

### Layer breadcrumbs

- Show layer hierarchy in canvas header: `Brief > Narrative` (World Timeline) or `Narrative > Moment` (Work Timeline)
- Current layer highlighted with `aria-current="page"`
- Parent segment is a clickable zoom-out button

### Per-layer honest empty-state

- Brief empty: "No era markers yet — switch to Narrative to see events." (en) / "尚无纪元标记" (zh-CN)
- Narrative empty: V1.122 baseline copy
- Moment empty: "No scene or beat data yet — switch to Narrative to see events." (en) / "尚无场景或节拍数据" (zh-CN)
- All copy in en + zh-CN; tested via `per-layer-empty-state.test.tsx`

## Why This Matters

Per-layer feel differentiation is what makes a multi-layer Timeline feel like **three instruments at three scales**, not three filters over the same data. The caller's "三层不一样的感受" mandate is honored when:

- An author opens a World and **feels** the Brief layer's wide, sparse, gold sweep
- Switches to Narrative and **feels** the balanced, neutral, event-rich timeline
- Opens a Work Timeline and switches to Moment and **feels** the close, dense, ink-on-paper scene stack

Without differentiated feel, the three layers collapse into "the same timeline with different filters" — which is the failure mode the caller explicitly named.

## When to Apply

- Designing a new multi-layer Canvas surface
- Tuning existing layer feel (e.g., promoting `--color-canvas-layer-narrative-accent` from dormant to active)
- Migrating from CSS keyframe to Framer Motion animation (P5+)
- Adding a 4th layer (e.g., World Moment or Work Brief in V1.124+) — extend the breadcrumb + feel contract

## Examples

### V1.123 P4 — Tokens

`tooling/design-tokens/src/tokens.css` (light + dark variants):
```css
--color-canvas-layer-brief-accent: <amber>; /* era/age */
--color-canvas-layer-narrative-accent: <neutral>; /* events; may stay dormant */
--color-canvas-layer-moment-accent: <ink>; /* scenes */
```

### V1.123 P4 — Semantic zoom hook

`apps/web/src/components/canvas/use-semantic-zoom.ts`:
```ts
useSemanticZoom({
  thresholds: { 'brief-narrative': [0.55, 0.70] },
  onLayerChange: (newLayer) => handleLayerChange(newLayer),
});
```

### V1.123 P4 — Layer-state URL persistence

`apps/web/src/components/canvas/timeline-canvas/timeline-canvas.tsx`:
```ts
const [searchParams, setSearchParams] = useSearchParams();
const activeLayer = validLayerOrDefault(searchParams.get('layer'), defaultLayer);
const handleLayerChange = (layer) => {
  if (layer === defaultLayer) searchParams.delete('layer');
  else searchParams.set('layer', layer);
  setSearchParams(searchParams, { replace: true });
};
```

## References

- V1.123 layer feel spec: `iterations/v1.123/specs/layer-feel-differentiation.md` (this doc promoted from there)
- V1.123 P4 implementer reports: `sdd/2026-07-18-v1.123-three-layer-zoom-experience/batch-a-report.md` + `batch-b-report.md`
- Three-layer timeline projection pattern: `knowledge/architecture-patterns/three-layer-timeline-projection.md`
- DESIGN.md + DESIGN.dark.md: V0.5 layer-feel section
