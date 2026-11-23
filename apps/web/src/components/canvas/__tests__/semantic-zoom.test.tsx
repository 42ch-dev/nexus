/**
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

describe('computeSemanticZoomTransition — pure threshold logic (P4 Task 3)', () => {
  const worldChain = { coarseLayer: 'brief', fineLayer: 'narrative' } as const;
  const workChain = { coarseLayer: 'narrative', fineLayer: 'moment' } as const;

  it('World Timeline: zoom-IN past 0.70 while on Brief → swap to Narrative', () => {
    const result = computeSemanticZoomTransition({
      currentLayer: 'brief',
      currentZoom: 0.75,
      chain: worldChain,
    });
    expect(result).toBe('narrative');
  });

  it('World Timeline: zoom-OUT past 0.55 while on Narrative → swap to Brief', () => {
    const result = computeSemanticZoomTransition({
      currentLayer: 'narrative',
      currentZoom: 0.5,
      chain: worldChain,
    });
    expect(result).toBe('brief');
  });

  it('Work Timeline: zoom-IN past 0.70 while on Narrative → swap to Moment', () => {
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
    expect(
      computeSemanticZoomTransition({
        currentLayer: 'brief',
        currentZoom: 0.65,
        chain: worldChain,
      }),
    ).toBeNull();

    expect(
      computeSemanticZoomTransition({
        currentLayer: 'narrative',
        currentZoom: 0.65,
        chain: worldChain,
      }),
    ).toBeNull();

    expect(
      computeSemanticZoomTransition({
        currentLayer: 'narrative',
        currentZoom: 0.6,
        chain: workChain,
      }),
    ).toBeNull();
  });

  it('Default thresholds: exact threshold values do NOT trigger a swap (strict inequality)', () => {
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

describe('useSemanticZoom — React hook (P4 Task 3)', () => {
  const wrapper = ({ children }: { children: ReactNode }) => (
    <ReactFlowProvider>{children}</ReactFlowProvider>
  );

  it('skips the FIRST observation so initial-mount zoom does not fire a swap (regression: Brief at zoom 1.0 must NOT immediately swap to Narrative)', () => {
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

    expect(onLayerChange).not.toHaveBeenCalled();

    zoom = 0.8;
    rerender();
    expect(onLayerChange).toHaveBeenCalledWith('narrative');
  });

  it('does NOT fire on mount even when the initial zoom exceeds the threshold', () => {
    const zoom = 1.0;
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

    expect(onLayerChange).not.toHaveBeenCalled();

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

    zoom = 0.62;
    rerender();
    zoom = 0.68;
    rerender();
    zoom = 0.55; // exact threshold — strict inequality, no swap
    rerender();

    expect(onLayerChange).not.toHaveBeenCalled();
  });

  it('does NOT re-fire when the zoom value is unchanged across re-renders (no parent-render false positives)', () => {
    const zoom = 0.4;
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

    rerender();
    rerender();
    expect(onLayerChange).not.toHaveBeenCalled();
  });

  it('uses React Flow useViewport() when getZoom is omitted (production path smoke)', () => {
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
