/**
 * useSemanticZoom — V1.123 P4 Task 3.
 *
 * Locks the semantic-zoom contract (layer-feel-differentiation.md §3 +
 * Plan `2026-07-18-v1.123-three-layer-zoom-experience.md` Task 3):
 *
 *   - World Timeline Brief↔Narrative: zoom-IN past 0.70 → Brief→Narrative;
 *     zoom-OUT past 0.55 → Narrative→Brief.
 *   - Work Timeline Narrative↔Moment: zoom-IN past 0.70 → Narrative→Moment;
 *     zoom-OUT past 0.55 → Moment→Narrative.
 *   - Hysteresis band 0.55–0.70 prevents flicker at the threshold.
 *
 * Strategy:
 *   - The pure threshold logic (`computeSemanticZoomTransition`) is tested
 *     directly — no React tree needed.
 *   - The hook (`useSemanticZoom`) is exercised via `renderHook` with a
 *     `getZoom` test override so the hook can run outside a real React
 *     Flow provider. The default `useViewport()` path is verified via a
 *     smoke test that wraps the hook in a `ReactFlowProvider` and asserts
 *     no exceptions + correct wiring.
 */
import { describe, expect, it, vi } from 'vitest';
import { renderHook } from '@testing-library/react';
import { ReactFlowProvider } from '@xyflow/react';
import type { ReactNode } from 'react';

import {
  computeSemanticZoomTransition,
  DEFAULT_SEMANTIC_ZOOM_THRESHOLDS,
  useSemanticZoom,
} from '../use-semantic-zoom';

// ─── Pure threshold logic ──────────────────────────────────────────────────

describe('computeSemanticZoomTransition — pure threshold logic (P4 Task 3)', () => {
  const worldChain = { coarseLayer: 'brief', fineLayer: 'narrative' } as const;
  const workChain = { coarseLayer: 'narrative', fineLayer: 'moment' } as const;

  it('World Timeline: zoom-IN past 0.70 while on Brief → swap to Narrative', () => {
    // Architect §3.2 + plan Global Constraints: zooming IN past the upper
    // threshold swaps the World Timeline from Brief (coarse) to Narrative
    // (fine) — closer reading distance.
    const result = computeSemanticZoomTransition({
      currentLayer: 'brief',
      currentZoom: 0.75,
      chain: worldChain,
    });
    expect(result).toBe('narrative');
  });

  it('World Timeline: zoom-OUT past 0.55 while on Narrative → swap to Brief', () => {
    // Architect §3.2: zooming OUT past the lower threshold swaps the World
    // Timeline from Narrative (fine) back to Brief (coarse) — whole-world
    // reading distance.
    const result = computeSemanticZoomTransition({
      currentLayer: 'narrative',
      currentZoom: 0.5,
      chain: worldChain,
    });
    expect(result).toBe('brief');
  });

  it('Work Timeline: zoom-IN past 0.70 while on Narrative → swap to Moment', () => {
    // Architect §3.3 + plan Global Constraints: zooming IN past the upper
    // threshold swaps the Work Timeline from Narrative (coarse) to Moment
    // (fine) — scene-level reading distance.
    const result = computeSemanticZoomTransition({
      currentLayer: 'narrative',
      currentZoom: 0.71,
      chain: workChain,
    });
    expect(result).toBe('moment');
  });

  it('Work Timeline: zoom-OUT past 0.55 while on Moment → swap to Narrative', () => {
    const result = computeSemanticZoomTransition({
      currentLayer: 'moment',
      currentZoom: 0.54,
      chain: workChain,
    });
    expect(result).toBe('narrative');
  });

  it('Hysteresis band: zoom values BETWEEN 0.55 and 0.70 do NOT trigger a swap (no flicker)', () => {
    // layer-feel §3.2 + plan Global Constraints: "Hysteresis — Require
    // clear overshoot both ways so layers do not flicker at the boundary."
    // The 0.15-unit band (0.55–0.70) is the no-swap zone — both layers
    // stay put when zoom dwells between the thresholds.
    // On Brief, zoom = 0.65 (between thresholds) — no swap.
    expect(
      computeSemanticZoomTransition({
        currentLayer: 'brief',
        currentZoom: 0.65,
        chain: worldChain,
      }),
    ).toBeNull();

    // On Narrative, zoom = 0.65 (between thresholds) — no swap.
    expect(
      computeSemanticZoomTransition({
        currentLayer: 'narrative',
        currentZoom: 0.65,
        chain: worldChain,
      }),
    ).toBeNull();

    // On Narrative (Work), zoom = 0.60 — no swap.
    expect(
      computeSemanticZoomTransition({
        currentLayer: 'narrative',
        currentZoom: 0.6,
        chain: workChain,
      }),
    ).toBeNull();
  });

  it('Default thresholds: exact threshold values do NOT trigger a swap (strict inequality)', () => {
    // Strict inequality: zoom = 0.70 exactly is NOT > 0.70 → no swap on
    // coarse layer. zoom = 0.55 exactly is NOT < 0.55 → no swap on fine
    // layer. This avoids floating-point edge cases at the boundary.
    expect(
      computeSemanticZoomTransition({
        currentLayer: 'brief',
        currentZoom: 0.7,
        chain: worldChain,
      }),
    ).toBeNull();

    expect(
      computeSemanticZoomTransition({
        currentLayer: 'narrative',
        currentZoom: 0.55,
        chain: worldChain,
      }),
    ).toBeNull();
  });

  it('Layer outside the chain returns null (defensive — future-proof for a fourth layer)', () => {
    // If a future iteration adds a fourth layer and the orchestrator
    // passes it without updating the chain, the hook MUST no-op rather
    // than silently swap to an arbitrary layer.
    expect(
      computeSemanticZoomTransition({
        currentLayer: 'future-layer' as never,
        currentZoom: 0.9,
        chain: worldChain,
      }),
    ).toBeNull();
  });

  it('Non-finite zoom (NaN / Infinity) returns null (defensive — RF transient states)', () => {
    expect(
      computeSemanticZoomTransition({
        currentLayer: 'brief',
        currentZoom: Number.NaN,
        chain: worldChain,
      }),
    ).toBeNull();
    expect(
      computeSemanticZoomTransition({
        currentLayer: 'brief',
        currentZoom: Number.POSITIVE_INFINITY,
        chain: worldChain,
      }),
    ).toBeNull();
  });

  it('Custom thresholds override the defaults (per-surface tuning)', () => {
    // A surface MAY pass tighter / looser thresholds. The hook MUST honor
    // the override verbatim (e.g. a future accessibility mode widens the
    // hysteresis band to 0.40–0.85 for users with fine-motor variance).
    const result = computeSemanticZoomTransition({
      currentLayer: 'brief',
      currentZoom: 0.6, // would NOT swap under defaults
      chain: worldChain,
      thresholds: { zoomInThreshold: 0.5, zoomOutThreshold: 0.3 },
    });
    expect(result).toBe('narrative');
  });

  it('Defaults match the architect-locked 0.55–0.70 band', () => {
    expect(DEFAULT_SEMANTIC_ZOOM_THRESHOLDS.zoomInThreshold).toBe(0.7);
    expect(DEFAULT_SEMANTIC_ZOOM_THRESHOLDS.zoomOutThreshold).toBe(0.55);
  });
});

// ─── React hook composition ────────────────────────────────────────────────

describe('useSemanticZoom — React hook (P4 Task 3)', () => {
  // Always wrap with ReactFlowProvider because `useViewport()` is always
  // called (Rules of Hooks) — even when `getZoom` overrides the value, the
  // hook still reads from RF's store. The provider's store is enough for
  // the hook to mount cleanly in jsdom; no real `<ReactFlow>` graph needed.
  const wrapper = ({ children }: { children: ReactNode }) => (
    <ReactFlowProvider>{children}</ReactFlowProvider>
  );

  it('skips the FIRST observation so initial-mount zoom does not fire a swap (regression: Brief at zoom 1.0 must NOT immediately swap to Narrative)', () => {
    // Architect contract: semantic zoom fires on USER-initiated zoom
    // changes, not on initial mount. RF's default viewport zoom is 1.0;
    // on a Brief (coarse) surface entry, 1.0 > 0.70 would immediately
    // swap to Narrative if the hook fired on mount. The hook MUST skip
    // the first observation and treat it as the baseline.
    let zoom = 0.5; // below threshold on mount
    const onLayerChange = vi.fn();
    const { rerender } = renderHook(
      () =>
        useSemanticZoom({
          activeLayer: 'brief',
          onLayerChange,
          chain: { coarseLayer: 'brief', fineLayer: 'narrative' },
          getZoom: () => zoom,
        }),
      { wrapper },
    );

    // Mount: hook records baseline, no swap.
    expect(onLayerChange).not.toHaveBeenCalled();

    // Subsequent zoom change past threshold → fires.
    zoom = 0.8;
    rerender();
    expect(onLayerChange).toHaveBeenCalledWith('narrative');
  });

  it('does NOT fire on mount even when the initial zoom exceeds the threshold', () => {
    // Stronger regression guard: a Brief surface mounting at zoom 1.0
    // (which exceeds the 0.70 zoom-in threshold) MUST NOT immediately
    // swap to Narrative. The hook records the mount zoom as baseline.
    let zoom = 1.0;
    const onLayerChange = vi.fn();
    renderHook(
      () =>
        useSemanticZoom({
          activeLayer: 'brief',
          onLayerChange,
          chain: { coarseLayer: 'brief', fineLayer: 'narrative' },
          getZoom: () => zoom,
        }),
      { wrapper },
    );
    expect(onLayerChange).not.toHaveBeenCalled();
  });

  it('fires onLayerChange when zoom crosses the zoom-IN threshold (Brief → Narrative)', () => {
    // The test injects `getZoom` so the hook can be deterministic across
    // rerenders without driving a real RF viewport. Production wires to
    // `useViewport()` (smoke test below).
    let zoom = 0.5;
    const onLayerChange = vi.fn();
    const { rerender } = renderHook(
      () =>
        useSemanticZoom({
          activeLayer: 'brief',
          onLayerChange,
          chain: { coarseLayer: 'brief', fineLayer: 'narrative' },
          getZoom: () => zoom,
        }),
      { wrapper },
    );

    // Mount at 0.5 — baseline recorded, no swap.
    expect(onLayerChange).not.toHaveBeenCalled();

    // Zoom IN past 0.70 → hook fires Brief → Narrative.
    zoom = 0.8;
    rerender();

    expect(onLayerChange).toHaveBeenCalledTimes(1);
    expect(onLayerChange).toHaveBeenCalledWith('narrative');
  });

  it('fires onLayerChange when zoom crosses the zoom-OUT threshold (Narrative → Brief)', () => {
    let zoom = 0.65;
    const onLayerChange = vi.fn();
    const { rerender } = renderHook(
      () =>
        useSemanticZoom({
          activeLayer: 'narrative',
          onLayerChange,
          chain: { coarseLayer: 'brief', fineLayer: 'narrative' },
          getZoom: () => zoom,
        }),
      { wrapper },
    );

    expect(onLayerChange).not.toHaveBeenCalled();

    zoom = 0.4;
    rerender();

    expect(onLayerChange).toHaveBeenCalledWith('brief');
  });

  it('does NOT fire when zoom dwells inside the hysteresis band', () => {
    let zoom = 0.6;
    const onLayerChange = vi.fn();
    const { rerender } = renderHook(
      () =>
        useSemanticZoom({
          activeLayer: 'brief',
          onLayerChange,
          chain: { coarseLayer: 'brief', fineLayer: 'narrative' },
          getZoom: () => zoom,
        }),
      { wrapper },
    );

    // Move inside the band — no swap.
    zoom = 0.62;
    rerender();
    zoom = 0.68;
    rerender();
    zoom = 0.55; // exact threshold — strict inequality, no swap
    rerender();

    expect(onLayerChange).not.toHaveBeenCalled();
  });

  it('does NOT re-fire when the zoom value is unchanged across re-renders (no parent-render false positives)', () => {
    // When a parent re-renders for an unrelated reason (e.g., a state
    // change in the orchestrator), the hook's effect may re-run with the
    // same zoom value. The hook MUST no-op in that case so unrelated
    // orchestrator re-renders do not trigger phantom layer swaps.
    let zoom = 0.4;
    const onLayerChange = vi.fn();
    const { rerender } = renderHook(
      () =>
        useSemanticZoom({
          activeLayer: 'narrative',
          onLayerChange,
          chain: { coarseLayer: 'brief', fineLayer: 'narrative' },
          getZoom: () => zoom,
        }),
      { wrapper },
    );

    // Mount: no swap (baseline).
    expect(onLayerChange).not.toHaveBeenCalled();

    // Re-render with same zoom — still no swap.
    rerender();
    rerender();
    expect(onLayerChange).not.toHaveBeenCalled();
  });

  it('uses React Flow useViewport() when getZoom is omitted (production path smoke)', () => {
    // Production smoke test: the hook reads zoom from useViewport() inside
    // a ReactFlowProvider. The hook records the initial viewport (zoom=0
    // in jsdom since RF has no real graph) as baseline and does not fire
    // any swap.
    const onLayerChange = vi.fn();

    const { result } = renderHook(
      () =>
        useSemanticZoom({
          activeLayer: 'narrative',
          onLayerChange,
          chain: { coarseLayer: 'brief', fineLayer: 'narrative' },
        }),
      { wrapper },
    );
    expect(result.current).toBeUndefined();
    expect(onLayerChange).not.toHaveBeenCalled();
  });
});
