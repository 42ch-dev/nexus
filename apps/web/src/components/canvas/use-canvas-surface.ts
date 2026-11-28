import { useCallback, useEffect, useMemo, useState, type ReactNode } from 'react';
import { type Edge, type Node, type NodeChangeset, type Viewport } from '@xyflow/react';

import type { CanvasSurfaceAdapter } from './canvas-surface-adapter';
import { useCanvasViewport } from './use-canvas-viewport';
import { useAutoLayout } from './use-auto-layout';

export interface CanvasSurfaceQueryResult<TGraph> {
  data: TGraph | undefined;
  isLoading: boolean;
  isError: boolean;
  error: unknown;
  refetch: () => void;
}

export interface UseCanvasSurfaceResult<TNodeData extends Record<string, unknown>, TEdgeData extends Record<string, unknown>> {
  nodes: Node<TNodeData>[];
  edges: Edge<TEdgeData>[];
  nodeTypes: import('@xyflow/react').NodeTypes;
  edgeTypes: import('@xyflow/react').EdgeTypes | undefined;
  onNodesChange: import('@xyflow/react').OnNodesChange;
  summaryText: string;
  viewport: {
    cachedViewport: Viewport | null;
    onViewportChange: (viewport: Viewport) => void;
  };
  showAlt: boolean;
  setShowAlt: (value: boolean) => void;
  altView: ReactNode;
  briefTimeBands: ReactNode;
  inspector: ReactNode;
  selectedNode: Node<TNodeData> | null;
  selectedNodeId: string | null;
  isLoading: boolean;
  isError: boolean;
  refetch: () => void;
  relayout?: () => void;
}

/**
 * Shared composition hook for canvas surfaces.
 *
 * Delegates viewport caching to {@link useCanvasViewport} and layout to
 * {@link useAutoLayout}. Owns graph projection, alt-view toggle,
 * and inspector selection so surface orchestrators do not duplicate shell wiring.
 */
export function useCanvasSurface<TGraph, TNodeData extends Record<string, unknown>, TEdgeData extends Record<string, unknown>>(
  adapter: CanvasSurfaceAdapter<TGraph, TNodeData, TEdgeData>,
  queryResult: CanvasSurfaceQueryResult<TGraph>,
): UseCanvasSurfaceResult<TNodeData, TEdgeData> {
  const surfaceKey = adapter.surfaceKind;
  const { cachedViewport, onViewportChange } = useCanvasViewport(surfaceKey);

  const [showAlt, setShowAlt] = useState(false);

  const graph = queryResult.data;

  const projected = useMemo(() => {
    if (!graph) return { nodes: [] as Node<TNodeData>[], edges: [] as Edge<TEdgeData>[] };
    return adapter.projectGraph(graph);
  }, [graph, adapter]);

  const [nodes, setNodes] = useState<Node<TNodeData>[]>(projected.nodes);

  useEffect(() => {
    setNodes((prev) => {
      if (prev.length === 0) return projected.nodes;
      const prevById = new Map(prev.map((n) => [n.id, n]));
      return projected.nodes.map((node) => {
        const existing = prevById.get(node.id);
        if (!existing) return node;
        return { ...node, position: existing.position, selected: existing.selected };
      });
    });
  }, [projected.nodes]);

  const onNodesChange = useCallback(
    (changes: NodeChangeset) => {
      setNodes((nds) => changes.applyTo(nds) as Node<TNodeData>[]);
    },
    [setNodes],
  );

  const layout = useAutoLayout(nodes, projected.edges, adapter.layoutOptions);

  useEffect(() => {
    if (layout.nodes === nodes) return;
    const layoutById = new Map(layout.nodes.map((n) => [n.id, n]));
    setNodes((prev) =>
      prev.map((node) => {
        const laidOut = layoutById.get(node.id);
        return laidOut ? { ...node, position: laidOut.position } : node;
      }),
    );
  }, [layout.nodes, nodes]);

  const selectedNode = useMemo(
    () => layout.nodes.find((n) => n.selected) ?? null,
    [layout.nodes],
  );
  const selectedNodeId = selectedNode?.id ?? null;

  const summaryText = useMemo(() => {
    if (!graph) return '';
    return adapter.summarizeGraph(graph);
  }, [graph, adapter]);

  const altView = useMemo(() => adapter.renderAltView?.() ?? null, [adapter]);
  const briefTimeBands = useMemo(() => {
    if (!graph) return null;
    const renderBriefTimeBands = (adapter as {
      renderBriefTimeBands?: (g: TGraph) => ReactNode;
    }).renderBriefTimeBands;
    return renderBriefTimeBands ? renderBriefTimeBands(graph) : null;
  }, [graph, adapter]);
  const inspector = useMemo(() => {
    if (!selectedNode) return null;
    return adapter.renderInspector?.(selectedNode) ?? null;
  }, [selectedNode, adapter]);

  return {
    nodes: layout.nodes,
    edges: projected.edges,
    nodeTypes: adapter.nodeTypes,
    edgeTypes: adapter.edgeTypes,
    onNodesChange,
    summaryText,
    viewport: { cachedViewport, onViewportChange },
    showAlt,
    setShowAlt,
    altView,
    briefTimeBands,
    inspector,
    selectedNode,
    selectedNodeId,
    isLoading: queryResult.isLoading,
    isError: queryResult.isError,
    refetch: queryResult.refetch,
    relayout: adapter.layoutOptions ? layout.relayout : undefined,
  };
}
