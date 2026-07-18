import type { ReactNode } from 'react';
import type { Edge, EdgeTypes, Node, NodeTypes } from '@xyflow/react';

import type { ConflictModalProps } from './conflict-modal';

export type CanvasSurfaceKind =
  | 'strategy'
  | 'outline'
  | 'world-kb-entities'
  | 'world-kb-relationships'
  // V1.122 P1 T1 — additive peer surface. The Timeline hero surface projects
  // a World's `WorldKbGraphResponse` onto a left-to-right when-axis. Reuses
  // the V1.73/V1.74 World KB DTOs verbatim (wire_contracts_changed: false);
  // see `timeline-canvas/` + `iterations/v1.122/specs/timeline-canvas-architecture.md`.
  | 'timeline'
  // V1.123 P2 Task 2 — additive Work Timeline peer surface (compiler-forced
  // scope expansion: the Work Timeline adapter declares
  // `surfaceKind: 'work-timeline'` and the enum must accept it for the
  // adapter to satisfy `CanvasSurfaceAdapter`). Task 5 owns the full peer
  // integration (route `/works/:workId/timeline` + canvas-nav resolver +
  // sidebar entry); this enum value is the minimum additive addition so
  // Tasks 2–4 ship adapter + projection + layer switcher without breaking
  // type-safety. See `work-timeline-canvas/` +
  // `iterations/v1.123/specs/three-layer-architecture.md` §7.4.
  | 'work-timeline';

export interface CanvasSurfaceLayoutOptions {
  direction?: 'TB' | 'LR';
  rankSep?: number;
  nodeSep?: number;
  /**
   * When true, the surface projects meaningful node positions (e.g. persisted
   * author positions, or a deterministic layout it wants preserved) and
   * `useAutoLayout` must NOT override them on first open. The author can still
   * trigger an explicit `relayout()` to force dagre.
   *
   * When false/undefined (default), `useAutoLayout` runs dagre on first open —
   * the historical behavior for Strategy and World KB.
   *
   * (R-V1114P0QC1-W003)
   */
  hasSuppliedPositions?: boolean;
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
