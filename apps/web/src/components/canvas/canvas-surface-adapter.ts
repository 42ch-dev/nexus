import type { ReactNode } from 'react';
import type { Edge, EdgeTypes, Node, NodeTypes } from '@xyflow/react';

import type { ConflictModalProps } from './conflict-modal';

export type CanvasSurfaceKind =
  | 'strategy'
  | 'outline'
  | 'world-kb-entities'
  | 'world-kb-relationships';

export interface CanvasSurfaceLayoutOptions {
  direction?: 'TB' | 'LR';
  rankSep?: number;
  nodeSep?: number;
}

export interface CanvasSurfaceAdapter<TGraph, TNodeData extends Record<string, unknown>, TEdgeData extends Record<string, unknown>> {
  surfaceKind: CanvasSurfaceKind;
  /** Project daemon graph DTO → React Flow nodes + edges. Owns parentId/extent nesting for sub-flows. */
  projectGraph(graph: TGraph): { nodes: Node<TNodeData>[]; edges: Edge<TEdgeData>[] };
  /** Node types registry for this surface. */
  nodeTypes: NodeTypes;
  /** Edge types registry (optional). */
  edgeTypes?: EdgeTypes;
  /** Layout options for dagre (T4 integration point — stub for now). */
  layoutOptions?: CanvasSurfaceLayoutOptions;
  /** Conflict DTO → conflict-modal props. */
  adaptConflict?(error: unknown): ConflictModalProps | null;
  /** Inspector routing: which inspector renders for a selected node. */
  renderInspector?(node: Node<TNodeData>): ReactNode;
  /** Alt-view companion (table/list). */
  renderAltView?(): ReactNode;
  /** Graph-level a11y summary (required). */
  summarizeGraph(graph: TGraph): string;
}
