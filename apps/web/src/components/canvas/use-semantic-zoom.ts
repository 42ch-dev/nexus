/**
 * useSemanticZoom — V1.123 P4 Task 3.
 *
 * Layer change is a discrete semantic swap (projection + feel + default
 * zoom), NOT continuous infinite viewport zoom that slowly morphs layouts
 * (layer-feel-differentiation.md §3.1). Within a layer, ordinary canvas
 * pan/zoom remains available; the semantic zoom hook observes the React
 * Flow viewport zoom and emits a discrete `layer-switch` event when the
 * zoom crosses architect-locked thresholds (layer-feel §3.2 + §3.3).
 *
 * Architect verdict (plan Global Constraints §"Semantic zoom feasibility"):
 * the 0.55–0.70 band is feasible against React Flow viewport semantics.
 * Default viewport zoom is `1.0`; the hook reads the current zoom via
 * `useViewport()` and dispatches a layer swap when:
 *   - World Timeline (Brief ↔ Narrative): zoom-in past 0.70 → swap Brief →
 *     Narrative; zoom-out past 0.55 → swap Narrative → Brief.
 *   - Work Timeline (Narrative ↔ Moment): zoom-in past 0.70 → swap
 *     Narrative → Moment; zoom-out past 0.55 → swap Moment → Narrative.
 *
 * Hysteresis (0.55–0.70 band, 0.15-unit gap) prevents flicker at the
 * threshold: a swap requires clear overshoot past the opposing boundary.
 *
 * This module exposes:
 *   - `computeSemanticZoomTransition` — pure threshold logic, trivially
 *     testable without a React tree.
 *   - `useSemanticZoom` — React hook that composes `useViewport()` with
 *     the pure logic and fires `onLayerChange` on threshold cross.
 *
 * `useSemanticZoom` MUST be used inside a `ReactFlowProvider` (it reads
 * the viewport from React Flow's store via `useViewport`). The two canvas
 * orchestrators (Timeline + WorkTimeline) already mount inside `CanvasShell`
 * which wraps the inner shell in a `ReactFlowProvider` — but the hook is
 * called from the orchestrator OUTSIDE that provider, so the orchestrator
 * must render a small `<SemanticZoomBridge>` inside `CanvasShell` children
 * to keep `useViewport()` inside the provider.
 *
 * To keep the hook testable + the call site simple, the hook also accepts
 * an optional `getZoom` override. Tests inject a stub; production wires
 * the bridge via `<SemanticZoomBridge>` (below).
 */
import { useEffect, useRef } from 'react';
import { useViewport } from '@xyflow/react';

/**
 * Hysteresis band — the architect-locked threshold pair (layer-feel §3.2).
 *
 * - `zoomInThreshold`: zoom strictly greater than this → swap to the finer
 *   layer (Brief → Narrative, or Narrative → Moment).
 * - `zoomOutThreshold`: zoom strictly less than this → swap to the coarser
 *   layer (Narrative → Brief, or Moment → Narrative).
 *
 * Defaults per plan Global Constraints + architect §3.2 feasibility review:
 *   zoomInThreshold = 0.70, zoomOutThreshold = 0.55 (0.15-unit hysteresis).
 */
export interface SemanticZoomThresholds {
  zoomInThreshold: number;
  zoomOutThreshold: number;
}

export const DEFAULT_SEMANTIC_ZOOM_THRESHOLDS: SemanticZoomThresholds = {
  zoomInThreshold: 0.7,
  zoomOutThreshold: 0.55,
};

/**
 * Ordered pair of layers in the semantic-zoom chain.
 *
 * - `coarseLayer` is the further-out reading distance (Brief on World
 *   Timeline; Narrative on Work Timeline).
 * - `fineLayer` is the closer-in reading distance (Narrative on World
 *   Timeline; Moment on Work Timeline).
 *
 * Zooming IN past `zoomInThreshold` swaps coarse → fine; zooming OUT past
 * `zoomOutThreshold` swaps fine → coarse.
 */
export interface SemanticZoomLayerChain<TLayer extends string> {
  coarseLayer: TLayer;
  fineLayer: TLayer;
}

/**
 * Pure threshold logic — given the current layer + zoom + the layer chain
 * + thresholds, return the layer the surface SHOULD switch to (or `null`
 * if no swap is warranted).
 *
 * Extracted as a pure function so the threshold contract is testable
 * without a React tree. The React hook below composes this with
 * `useViewport()`.
 *
 * Hysteresis is encoded structurally: when the current layer is the coarse
 * layer, only the `zoomInThreshold` can fire; when the current layer is
 * the fine layer, only the `zoomOutThreshold` can fire. A user dwelling
 * between the two thresholds stays on the current layer — no flicker.
 *
 * Edge cases:
 *   - If `currentLayer` is not part of the chain, returns `null` (defensive
 *     — the orchestrator may pass an unrelated layer if the surface grows
 *     a fourth layer in a future iteration).
 *   - If `currentZoom` is NaN or non-finite, returns `null` (defensive —
 *     React Flow emits finite viewports but a future RF version could
 *     surface transient non-finite values during rapid wheel events).
 */
export function computeSemanticZoomTransition<TLayer extends string>(args: {
  currentLayer: TLayer;
  currentZoom: number;
  chain: SemanticZoomLayerChain<TLayer>;
  thresholds?: SemanticZoomThresholds;
}): TLayer | null {
  const { currentLayer, currentZoom, chain, thresholds } = args;
  const t = thresholds ?? DEFAULT_SEMANTIC_ZOOM_THRESHOLDS;

  if (!Number.isFinite(currentZoom)) return null;

  if (currentLayer === chain.coarseLayer) {
    // Zooming IN past the upper threshold swaps coarse → fine.
    if (currentZoom > t.zoomInThreshold) {
      return chain.fineLayer;
    }
    return null;
  }

  if (currentLayer === chain.fineLayer) {
    // Zooming OUT past the lower threshold swaps fine → coarse.
    if (currentZoom < t.zoomOutThreshold) {
      return chain.coarseLayer;
    }
    return null;
  }

  // Current layer is outside the chain — no swap.
  return null;
}

/**
 * React hook that observes React Flow viewport zoom and emits layer-change
 * events when the surface crosses the architect-locked thresholds.
 *
 * Call inside a `ReactFlowProvider` ancestor (the `<CanvasShell>` wrapper
 * provides one; orchestrators that mount the shell can also use the
 * `<SemanticZoomBridge>` helper below to keep the hook call inside the
 * provider).
 *
 * @param activeLayer  Current active layer.
 * @param onLayerChange  Callback fired when the hook decides a swap is due.
 *   The hook passes the target layer; the orchestrator owns the actual
 *   state update (so the layer swap can compose with URL persistence,
 *   breadcrumbs, etc. in Tasks 5+6).
 * @param chain  Ordered coarse → fine layer pair.
 * @param thresholds  Optional override of the 0.55–0.70 default band.
 *   Production callers omit this; tests inject edge-case thresholds.
 * @param getZoom  Optional test-only override of the zoom source. When
 *   provided, the hook reads zoom from this getter instead of React Flow's
 *   `useViewport()`. Production callers omit this so the hook wires to RF.
 */
export interface UseSemanticZoomOptions<TLayer extends string> {
  activeLayer: TLayer;
  onLayerChange: (layer: TLayer) => void;
  chain: SemanticZoomLayerChain<TLayer>;
  thresholds?: SemanticZoomThresholds;
  /**
   * Test-only override of the zoom source. Production callers MUST omit
   * this; the hook wires to `useViewport()` from `@xyflow/react`.
   */
  getZoom?: () => number;
}

export function useSemanticZoom<TLayer extends string>({
  activeLayer,
  onLayerChange,
  chain,
  thresholds,
  getZoom,
}: UseSemanticZoomOptions<TLayer>): void {
  // `useViewport()` re-renders the subscriber on every viewport change.
  // Always called (Rules of Hooks); when `getZoom` is provided (tests), the
  // RF read is discarded in favor of the test stub. Production callers omit
  // `getZoom` so the hook reads the live RF zoom.
  const viewport = useViewport();
  const currentZoom = getZoom ? getZoom() : viewport.zoom;

  // Skip the FIRST observation so the hook does not fire on initial mount.
  // React Flow's default viewport zoom is 1.0; on a Brief (coarse) surface
  // entry, 1.0 > 0.70 would immediately swap to Narrative — wrong. The
  // semantic zoom contract is "fire on user-initiated zoom change", not
  // "fire on mount". We track the previous observed zoom in a ref; when
  // the previous value is `null`, the current observation is the mount
  // snapshot and we suppress the swap, then record the value for the next
  // pass.
  //
  // The skip is per-mount, not per-layer-swap: when the orchestrator
  // swaps layers via the explicit tab click, the keyed wrapper remounts
  // the bridge, resetting this ref. The new layer inherits the current
  // viewport zoom as its "baseline" — so a layer swap to Brief at zoom
  // 1.0 does not immediately bounce back to Narrative. The user must
  // explicitly zoom in again to trigger the semantic swap.
  const previousZoomRef = useRef<number | null>(null);

  useEffect(() => {
    if (previousZoomRef.current === null) {
      // First observation after mount — record baseline, do not fire.
      previousZoomRef.current = currentZoom;
      return;
    }
    if (previousZoomRef.current === currentZoom) {
      // No zoom change since last observation (e.g., parent re-rendered
      // for an unrelated reason). Do not fire.
      return;
    }
    previousZoomRef.current = currentZoom;

    const target = computeSemanticZoomTransition({
      currentLayer: activeLayer,
      currentZoom,
      chain,
      thresholds,
    });
    if (target !== null && target !== activeLayer) {
      onLayerChange(target);
    }
  }, [activeLayer, currentZoom, chain, thresholds, onLayerChange]);
}

// ─── React Flow bridge component ───────────────────────────────────────────

import type { ReactNode } from 'react';

/**
 * Thin bridge that lets a canvas orchestrator wire `useSemanticZoom` even
 * though the orchestrator itself sits OUTSIDE the `<ReactFlowProvider>`
 * that `<CanvasShell>` mounts internally.
 *
 * The orchestrator renders `<SemanticZoomBridge ... />` as a child of
 * `<CanvasShell>` (alongside the inspector overlay). Inside the shell's
 * provider, the bridge calls `useSemanticZoom` with the orchestrator's
 * `activeLayer` + `onLayerChange` props. When the RF viewport crosses a
 * threshold, the bridge fires `onLayerChange` and the orchestrator updates
 * its layer state — the layer swap then re-derives the adapter + projection
 * via the existing `useMemo([activeLayer], ...)` pattern.
 *
 * Renders nothing visible — purely a hook host.
 */
export function SemanticZoomBridge<TLayer extends string>(
  props: UseSemanticZoomOptions<TLayer>,
): ReactNode {
  useSemanticZoom(props);
  return null;
}
