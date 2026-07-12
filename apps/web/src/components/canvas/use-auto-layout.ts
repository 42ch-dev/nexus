import { useCallback, useMemo } from 'react';
import type { Edge, Node } from '@xyflow/react';

import type { CanvasSurfaceLayoutOptions } from './canvas-surface-adapter';

export interface UseAutoLayoutResult<TNodeData extends Record<string, unknown>> {
  nodes: Node<TNodeData>[];
  isLayouting: boolean;
  /** Trigger a re-layout. No-op in the T1 stub; T4 will implement. */
  relayout: () => void;
}

/**
 * T1 stub: passes nodes through unchanged.
 *
 * T4 will replace this with a real dagre integration. The `options` parameter is
 * accepted now so callers do not have to change when the real implementation lands.
 */
export function useAutoLayout<TNodeData extends Record<string, unknown>>(
  nodes: Node<TNodeData>[],
  edges: Edge[],
  options?: CanvasSurfaceLayoutOptions,
): UseAutoLayoutResult<TNodeData> {
  const relayout = useCallback(() => {}, []);
  return useMemo(
    () => ({
      nodes,
      isLayouting: false,
      relayout,
    }),
    [nodes, edges, options, relayout],
  );
}
