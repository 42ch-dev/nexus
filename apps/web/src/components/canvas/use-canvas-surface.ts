import { useCallback, useEffect, useMemo, useState, type ReactNode } from 'react';
import { applyNodeChanges, type Edge, type Node, type NodeChange, type Viewport } from '@xyflow/react';

import type { ConflictModalProps } from './conflict-modal';
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
  inspector: ReactNode;
  selectedNode: Node<TNodeData> | null;
  selectedNodeId: string | null;
  conflict: ConflictModalProps | null;
  setConflict: (conflict: ConflictModalProps | null) => void;
  handleConflict: (error: unknown) => void;
  isLoading: boolean;
  isError: boolean;
  refetch: () => void;
  relayout?: () => void;
}

/**
 * Shared composition hook for canvas surfaces.
 *
 * Delegates viewport caching to {@link useCanvasViewport} and layout to
 * {@link useAutoLayout}. Owns graph projection, conflict state, alt-view toggle,
 * and inspector selection so surface orchestrators do not duplicate shell wiring.
 */
export function useCanvasSurface<TGraph, TNodeData extends Record<string, unknown>, TEdgeData extends Record<string, unknown>>(
  adapter: CanvasSurfaceAdapter<TGraph, TNodeData, TEdgeData>,
  queryResult: CanvasSurfaceQueryResult<TGraph>,
): UseCanvasSurfaceResult<TNodeData, TEdgeData> {
  // Viewport key is the surface kind per product direction; per-instance overrides
  // should call useCanvasViewport directly.
  const surfaceKey = adapter.surfaceKind;
  const { cachedViewport, onViewportChange } = useCanvasViewport(surfaceKey);

  const [showAlt, setShowAlt] = useState(false);
  const [conflict, setConflict] = useState<ConflictModalProps | null>(null);

  const graph = queryResult.data;

  const projected = useMemo(() => {
    if (!graph) return { nodes: [] as Node<TNodeData>[], edges: [] as Edge<TEdgeData>[] };
    return adapter.projectGraph(graph);
  }, [graph, adapter]);

  const [nodes, setNodes] = useState<Node<TNodeData>[]>(projected.nodes);

  // Sync the projected graph into local RF state, preserving manual positions and
  // selection for nodes that persist across the rebuild (same as outline-canvas-graph).
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
    (changes: NodeChange[]) => {
      setNodes((nds) => applyNodeChanges(changes, nds) as Node<TNodeData>[]);
    },
    [setNodes],
  );

  const layout = useAutoLayout(nodes, projected.edges, adapter.layoutOptions);

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
  const inspector = useMemo(() => {
    if (!selectedNode) return null;
    return adapter.renderInspector?.(selectedNode) ?? null;
  }, [selectedNode, adapter]);

  // Auto-populate conflict state from query errors.
  useEffect(() => {
    if (queryResult.isError && adapter.adaptConflict) {
      setConflict(adapter.adaptConflict(queryResult.error));
    }
  }, [queryResult.isError, queryResult.error, adapter]);

  const handleConflict = useCallback(
    (error: unknown) => {
      if (!adapter.adaptConflict) return;
      setConflict(adapter.adaptConflict(error));
    },
    [adapter],
  );

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
    inspector,
    selectedNode,
    selectedNodeId,
    conflict,
    setConflict,
    handleConflict,
    isLoading: queryResult.isLoading,
    isError: queryResult.isError,
    refetch: queryResult.refetch,
    relayout: adapter.layoutOptions ? layout.relayout : undefined,
  };
}
