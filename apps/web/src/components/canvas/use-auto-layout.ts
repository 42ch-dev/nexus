import { useCallback, useLayoutEffect, useMemo, useRef, useState } from 'react';
import dagre, { Graph, type NodeLabel as DagreNodeLabel } from '@dagrejs/dagre';
import type { Edge, Node } from '@xyflow/react';

import type { CanvasSurfaceLayoutOptions } from './canvas-surface-adapter';

export interface UseAutoLayoutResult<TNodeData extends Record<string, unknown>> {
  nodes: Node<TNodeData>[];
  isLayouting: boolean;
  /** Trigger a re-layout. */
  relayout: () => void;
}

const EPSILON = 0.5;

const DEFAULT_WIDTH = 180;
const DEFAULT_HEIGHT = 60;
const INNER_DEFAULT_WIDTH = 150;
const INNER_DEFAULT_HEIGHT = 40;

const LAYOUT_WARN_MS = 200;

function nodeDimensions<TNodeData extends Record<string, unknown>>(
  node: Node<TNodeData>,
): { width: number; height: number } {
  const measured = node.measured;
  const width =
    measured?.width ?? (node.type === 'strategy-inner' ? INNER_DEFAULT_WIDTH : DEFAULT_WIDTH);
  const height =
    measured?.height ?? (node.type === 'strategy-inner' ? INNER_DEFAULT_HEIGHT : DEFAULT_HEIGHT);
  return { width, height };
}

function computeDagreLayout<TNodeData extends Record<string, unknown>>(
  nodes: Node<TNodeData>[],
  edges: Edge[],
  options: CanvasSurfaceLayoutOptions,
): Node<TNodeData>[] {
  const graph = new Graph({ compound: true });
  graph.setDefaultEdgeLabel(() => ({}));
  graph.setGraph({
    rankdir: options.direction ?? 'TB',
    ranksep: options.rankSep ?? 80,
    nodesep: options.nodeSep ?? 80,
    edgesep: 40,
    marginx: 0,
    marginy: 0,
  });

  for (const node of nodes) {
    const { width, height } = nodeDimensions(node);
    graph.setNode(node.id, { width, height });
  }

  for (const edge of edges) {
    graph.setEdge(edge.source, edge.target);
  }

  for (const node of nodes) {
    if (node.parentId) {
      graph.setParent(node.id, node.parentId);
    }
  }

  const start = performance.now();
  dagre.layout(graph);
  const elapsed = performance.now() - start;
  if (elapsed > LAYOUT_WARN_MS) {
    // eslint-disable-next-line no-console
    console.warn(
      `[useAutoLayout] dagre layout took ${elapsed.toFixed(1)}ms on ${nodes.length} nodes; per-architect flag this should be recorded as a residual if observed in production`,
    );
  }

  const absolute = new Map<string, { x: number; y: number; width: number; height: number }>();
  for (const node of nodes) {
    const label = graph.node(node.id) as DagreNodeLabel | undefined;
    // Defensive: dagre may not assign coordinates to every setNode'd node
    // (compound-graph ranker edge cases). Fall back to the origin and the
    // node's own dimensions so the layout never crashes on a missing or
    // partial label (R-V1114P0QC2-M001).
    const fallback = nodeDimensions(node);
    const width = label?.width ?? fallback.width;
    const height = label?.height ?? fallback.height;
    absolute.set(node.id, {
      x: (label?.x ?? 0) - width / 2,
      y: (label?.y ?? 0) - height / 2,
      width,
      height,
    });
  }

  return nodes.map((node) => {
    const abs = absolute.get(node.id)!;
    let position = { x: abs.x, y: abs.y };
    if (node.parentId) {
      const parentAbs = absolute.get(node.parentId);
      if (parentAbs) {
        position = {
          x: abs.x - parentAbs.x,
          y: abs.y - parentAbs.y,
        };
      }
    }

    const style: React.CSSProperties = {
      ...node.style,
      width: abs.width,
      height: abs.height,
    };

    return { ...node, position, style };
  });
}

/**
 * Dagre-powered auto-layout for canvas surfaces.
 *
 * - Surfaces opt in by supplying `layoutOptions` on their adapter. Surfaces that
 *   do not opt in receive a pass-through (no position changes).
 * - First projection with opt-in options triggers an automatic layout — unless
 *   `layoutOptions.hasSuppliedPositions` is set, in which case the surface's
 *   positions are preserved until an explicit `relayout()` (W003).
 * - Manual drags suppress auto-layout until the user explicitly re-layouts.
 * - `relayout()` clears manual overrides and re-runs dagre.
 */
export function useAutoLayout<TNodeData extends Record<string, unknown>>(
  nodes: Node<TNodeData>[],
  edges: Edge[],
  options?: CanvasSurfaceLayoutOptions,
): UseAutoLayoutResult<TNodeData> {
  const [layoutGeneration, setLayoutGeneration] = useState(0);
  const relayout = useCallback(() => setLayoutGeneration((g) => g + 1), []);

  const initialLayoutDoneRef = useRef(false);
  const lastLayoutGenRef = useRef(0);
  const hasManualPositionRef = useRef(false);
  const manualNodeIdsRef = useRef(new Set<string>());
  const lastLayoutPositionsRef = useRef(new Map<string, { x: number; y: number }>());

  const result = useMemo(() => {
    if (!options) {
      return { nodes, isLayouting: false, relayout };
    }

    // Detect manual drags by comparing incoming positions against the snapshot
    // from the last layout run. Any deviation marks the surface as dirty and
    // suppresses automatic re-layouts. This loop is read-only: refs are updated
    // in the layout effect below so strict-mode double-invocation cannot leak
    // state between memo runs.
    let hasManualPosition = hasManualPositionRef.current;
    for (const node of nodes) {
      if (manualNodeIdsRef.current.has(node.id)) continue;
      const last = lastLayoutPositionsRef.current.get(node.id);
      if (last === undefined) continue;
      if (
        Math.abs(node.position.x - last.x) > EPSILON ||
        Math.abs(node.position.y - last.y) > EPSILON
      ) {
        hasManualPosition = true;
      }
    }

    const isRelayout = layoutGeneration !== lastLayoutGenRef.current;
    const isFirstLayout = !initialLayoutDoneRef.current;
    // A surface may signal that it already supplies meaningful positions
    // (hasSuppliedPositions). On first open, respect those positions and skip
    // dagre; the author can still trigger an explicit relayout() (W003).
    const hasSuppliedPositions = options.hasSuppliedPositions === true;
    const shouldLayout =
      isRelayout || (isFirstLayout && !hasManualPosition && !hasSuppliedPositions);

    if (!shouldLayout) {
      return { nodes, isLayouting: false, relayout };
    }

    const laidOut = computeDagreLayout(nodes, edges, options);
    return { nodes: laidOut, isLayouting: false, relayout };
  }, [nodes, edges, options, layoutGeneration, relayout]);

  useLayoutEffect(() => {
    if (!options) {
      return;
    }

    // Apply the manual-override side effects that were intentionally kept out
    // of the memo above.
    for (const node of nodes) {
      if (manualNodeIdsRef.current.has(node.id)) continue;
      const last = lastLayoutPositionsRef.current.get(node.id);
      if (last === undefined) continue;
      if (
        Math.abs(node.position.x - last.x) > EPSILON ||
        Math.abs(node.position.y - last.y) > EPSILON
      ) {
        manualNodeIdsRef.current.add(node.id);
        hasManualPositionRef.current = true;
      }
    }

    const layoutApplied = result.nodes !== nodes;
    const isRelayout = layoutGeneration !== lastLayoutGenRef.current;
    const firstCheckpoint = !initialLayoutDoneRef.current;

    // When dagre did not run AND we have already passed the first checkpoint,
    // there is nothing to snapshot — manual-drag detection continues to use
    // the existing baseline. On the first checkpoint, snapshot even when dagre
    // was skipped (supplied-positions mode) so subsequent manual drags can be
    // detected against the preserved baseline (W003).
    if (!layoutApplied && !firstCheckpoint) {
      return;
    }

    lastLayoutPositionsRef.current = new Map(
      result.nodes.map((n) => [n.id, { ...n.position }]),
    );

    if (layoutApplied && isRelayout) {
      manualNodeIdsRef.current = new Set();
      hasManualPositionRef.current = false;
    }

    initialLayoutDoneRef.current = true;
    if (layoutApplied) {
      lastLayoutGenRef.current = layoutGeneration;
    }
  }, [nodes, edges, options, layoutGeneration, result]);

  return result;
}
