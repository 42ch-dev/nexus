/**
 * Viewport preservation hook — caches React Flow pan/zoom across re-mounts
 * (FB-GS-000, V1.109 P2 T1; residual R-V1108P0QC3-S002).
 *
 * The graph↔list alt-view toggle unmounts `CanvasShell` (and therefore the
 * RF tree), which drops the viewport state. This hook stores the last
 * user viewport in a module-level `Map` keyed by `surfaceKey` so a
 * re-mount restores the pan/zoom instead of re-fitting.
 *
 * Lifecycle (matches the product bar in `canvas-graph-scale.md` §FB-GS-000):
 * - **When saved:** on user pan/zoom (RF `onMove`) while the graph is mounted.
 * - **When restored:** on re-mount of the graph view if a cached viewport
 *   exists for this surface.
 * - **When cleared:** hard navigation / full page reload clears the
 *   module-level store (in-memory only — no disk persistence required).
 *
 * The store is module-level (not React state) so it survives component
 * unmount. It is keyed by a caller-supplied `surfaceKey` (e.g.
 * `outline:<workId>`) so concurrent surfaces do not collide.
 */
import { useCallback, useState } from 'react';
import type { Viewport } from '@xyflow/react';

// simplify: single module-level Map, no eviction. Acceptable for a local-first
// SPA with a handful of concurrent surfaces; add LRU if surface count grows.
const viewportCache = new Map<string, Viewport>();

export interface UseCanvasViewportResult {
  /**
   * The viewport cached for this surface, read once on mount. `null` when no
   * viewport has been cached yet (first mount) or when `surfaceKey` is not
   * provided (opt-out). Pass to RF `defaultViewport` and use to gate `fitView`.
   */
  readonly cachedViewport: Viewport | null;
  /**
   * Stable callback to attach to React Flow `onMove`. Updates the cache as the
   * user pans/zooms. No-op when `surfaceKey` is undefined.
   */
  onViewportChange: (viewport: Viewport) => void;
}

/**
 * Cache and restore the React Flow viewport for a canvas surface.
 *
 * @param surfaceKey - Stable key namespacing the cache (e.g. `outline:<workId>`).
 *   When omitted, the hook opts out of caching entirely (backward-compatible
 *   default for surfaces that have not opted in yet).
 */
export function useCanvasViewport(surfaceKey?: string): UseCanvasViewportResult {
  // Read once on mount via the useState initializer so the cached value is
  // stable across re-renders within the same mount lifecycle.
  const [cachedViewport] = useState<Viewport | null>(
    () => (surfaceKey ? (viewportCache.get(surfaceKey) ?? null) : null),
  );

  const onViewportChange = useCallback(
    (viewport: Viewport) => {
      if (surfaceKey) {
        viewportCache.set(surfaceKey, viewport);
      }
    },
    [surfaceKey],
  );

  return { cachedViewport, onViewportChange };
}

/**
 * Clear cached viewport(s). Used by tests to isolate cases and available for
 * hard-navigation resets where the product wants to drop the in-memory store.
 */
export function clearViewportCache(surfaceKey?: string): void {
  if (surfaceKey) {
    viewportCache.delete(surfaceKey);
  } else {
    viewportCache.clear();
  }
}
