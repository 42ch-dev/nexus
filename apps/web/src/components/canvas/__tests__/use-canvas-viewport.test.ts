/**
 * `useCanvasViewport` — viewport preservation across graph↔list toggle
 * (FB-GS-000, V1.109 P2 T1).
 *
 * The alt-view toggle unmounts `CanvasShell` (and therefore the React Flow
 * tree), dropping the pan/zoom viewport. This hook caches `{ x, y, zoom }`
 * in a module-level store keyed by surface so a re-mount restores the last
 * user viewport instead of re-fitting.
 *
 * The hook is exercised directly via `renderHook`; React Flow is never
 * mounted. Each test simulates the graph→list→graph lifecycle by unmounting
 * one hook instance and rendering a fresh one with the same surface key.
 */
import { beforeEach, describe, expect, it } from 'vitest';
import { renderHook } from '@testing-library/react';
import type { Viewport } from '@xyflow/react';

import { useCanvasViewport, clearViewportCache } from '../use-canvas-viewport';

const SURFACE_KEY = 'outline:wk_test';

const VIEWPORT_A: Viewport = { x: 100, y: 200, zoom: 1.5 };
const VIEWPORT_B: Viewport = { x: -50, y: 75, zoom: 0.75 };

describe('useCanvasViewport — viewport caching across remounts (FB-GS-000)', () => {
  beforeEach(() => {
    clearViewportCache();
  });

  it('returns null cached viewport on first mount (no prior cache)', () => {
    const { result } = renderHook(() => useCanvasViewport(SURFACE_KEY));
    expect(result.current.cachedViewport).toBeNull();
  });

  it('caches viewport changes and restores them on remount', () => {
    // First "mount" — graph is shown; no cached viewport yet.
    const first = renderHook(() => useCanvasViewport(SURFACE_KEY));
    expect(first.result.current.cachedViewport).toBeNull();

    // Simulate the user panning/zooming — RF onMove fires.
    first.result.current.onViewportChange(VIEWPORT_A);

    // Simulate graph→list toggle (unmount) then list→graph (remount).
    first.unmount();
    const second = renderHook(() => useCanvasViewport(SURFACE_KEY));

    // The cached viewport is restored.
    expect(second.result.current.cachedViewport).toEqual(VIEWPORT_A);
  });

  it('updates the cache when the viewport changes again after remount', () => {
    const first = renderHook(() => useCanvasViewport(SURFACE_KEY));
    first.result.current.onViewportChange(VIEWPORT_A);
    first.unmount();

    const second = renderHook(() => useCanvasViewport(SURFACE_KEY));
    expect(second.result.current.cachedViewport).toEqual(VIEWPORT_A);

    // Pan to a different position after remount.
    second.result.current.onViewportChange(VIEWPORT_B);
    second.unmount();

    const third = renderHook(() => useCanvasViewport(SURFACE_KEY));
    expect(third.result.current.cachedViewport).toEqual(VIEWPORT_B);
  });

  it('namespaces caches by surface key (no cross-surface leak)', () => {
    const outlineHook = renderHook(() => useCanvasViewport('outline:wk_test'));
    outlineHook.result.current.onViewportChange(VIEWPORT_A);
    outlineHook.unmount();

    const strategyHook = renderHook(() => useCanvasViewport('strategy:wk_test'));
    // Strategy surface has its own cache slot — Outline viewport does not leak.
    expect(strategyHook.result.current.cachedViewport).toBeNull();
  });

  it('does not cache when surfaceKey is undefined (opt-out)', () => {
    const first = renderHook(() => useCanvasViewport());
    first.result.current.onViewportChange(VIEWPORT_A);
    first.unmount();

    const second = renderHook(() => useCanvasViewport());
    expect(second.result.current.cachedViewport).toBeNull();
  });

  it('exposes a stable onViewportChange callback across re-renders', () => {
    const hook = renderHook(() => useCanvasViewport(SURFACE_KEY));
    const first = hook.result.current.onViewportChange;
    hook.rerender();
    expect(hook.result.current.onViewportChange).toBe(first);
  });
});
