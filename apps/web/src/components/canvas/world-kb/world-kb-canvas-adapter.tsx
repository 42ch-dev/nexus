/**
 * World KB canvas adapter — projects the daemon graph + candidates into React
 * Flow nodes/edges and renders surface-specific chrome (inspectors, alt-view,
 * a11y summary).
 *
 * V1.114 P0 T3: the adapter implements the shared {@link CanvasSurfaceAdapter}
 * interface so the orchestrator delegates projection, node types, summary, and
 * alt-view to the surface. The returned adapter object is stable; it reads
 * mutable values from the supplied context ref so the orchestrator can update
 * state without invalidating `useCanvasSurface`'s memoized graph projection.
 */
import type { MutableRefObject } from 'react';
import type { Edge, Node } from '@xyflow/react';

import type { CanvasSurfaceAdapter } from '../canvas-surface-adapter';
import type { ConflictModalProps } from '../conflict-modal';
import { worldKbNodeTypes } from './entity-node';
import {
  anchorNodes,
  deriveEdges,
  graphSummary,
  layoutNodes,
} from './graph-projection';
import { nodesToData } from './world-kb-canvas-utils';
import {
  deriveRelationshipEdges,
  filterRelationshipEdgesByConfidence,
} from './relationship-projection';
import { WorldKbAltView } from './world-kb-alt-view';
import { EntityInspector, type EntityEditForm } from './entity-inspector';
import { PromotionInspector } from './promotion-inspector';
import type { RelationshipForm } from './relationship-inspector';
import type { EntityField, Selection } from './world-kb-canvas-types';
import type { WorldKbNodeData, WorldKbEdgeData } from './types';
import type {
  WorldKbCandidateProjection,
  WorldKbEntityProjection,
  WorldKbGraphResponse,
  WorldKbRelationshipProjection,
  WorldKbSourceAnchorProjection,
} from '@42ch/nexus-contracts';

/** Graph payload consumed by the World KB surface adapter. */
export interface WorldKbSurfaceGraph {
  worldId: string;
  graph: WorldKbGraphResponse;
  candidates: WorldKbCandidateProjection[];
  /** Confidence threshold for confirmed relationship edges (T4 stub; 0 = show all). */
  confidenceThreshold: number;
}

/**
 * Mutable context supplied by the orchestrator so the adapter can render
 * inspectors / alt-view without closing over stale values.
 *
 * All fields are read at render time from the ref; the adapter object itself is
 * stable and never recreated.
 */
export interface WorldKbCanvasAdapterContext {
  worldId: string;
  selection: Selection;
  confirmedEntities: WorldKbEntityProjection[];
  anchors: WorldKbSourceAnchorProjection[];
  relationships: WorldKbRelationshipProjection[];
  reseedSignal: number;
  onEntityConflict: (payload: {
    currentVersion: number;
    entityId: string;
    conflictingPath: string;
    draft: EntityEditForm;
    dirtyFields: EntityField[];
  }) => void;
  onPromoteConflict: (payload: {
    currentVersion: number;
    candidateName: string;
    newStatus: 'adopted' | 'rejected' | 'merged';
    action: 'adopt' | 'reject' | 'merge';
    mergeTargetId?: string;
    mergeTargetLabel?: string;
  }) => void;
  onRelationshipConflict: (payload: {
    currentVersion: number;
    relationshipId: string;
    draft: RelationshipForm;
  }) => void;
  onRelationshipSaved: () => void;
  onSelectNode: (node: WorldKbNodeData) => void;
  onSelectRelationship: (relationship: WorldKbRelationshipProjection) => void;
  onCreateRelationship: (initial?: {
    sourceEntityId?: string;
    targetEntityId?: string;
  }) => void;
  onDeleteRelationship: (rel: WorldKbRelationshipProjection) => void;
  onPromoteSuggestion: (rel: WorldKbRelationshipProjection) => void;
  onDeleteSuggestion: (rel: WorldKbRelationshipProjection) => void;
  onPromoteAllSuggestions: (
    rels: WorldKbRelationshipProjection[],
  ) => Promise<{ succeeded: number; failed: number }>;
  patchRelationshipIsPending: boolean;
  onActiveTabChange: (tab: 'entities' | 'relationships' | 'suggested') => void;
  selectedNodeId: string | null;
  selectedRelationshipId: string | null;
  nodes: Node<WorldKbNodeData>[];
}

export type WorldKbCanvasAdapter = CanvasSurfaceAdapter<
  WorldKbSurfaceGraph,
  WorldKbNodeData,
  WorldKbEdgeData
>;

/**
 * World KB canvas adapter — projects the daemon graph into React Flow nodes and
 * edges and renders surface-specific chrome.
 *
 * The returned adapter is stable; it reads mutable values from the supplied
 * context ref so the orchestrator can update state without invalidating the
 * hook's memoized graph projection.
 */
export function createWorldKbCanvasAdapter(
  ctxRef: MutableRefObject<WorldKbCanvasAdapterContext>,
): WorldKbCanvasAdapter {
  return {
    // CanvasSurfaceKind currently distinguishes 'world-kb-entities' from
    // 'world-kb-relationships'; this surface is the entity projection.
    surfaceKind: 'world-kb-entities',
    nodeTypes: worldKbNodeTypes,
    edgeTypes: undefined,
    layoutOptions: undefined,

    projectGraph(graph) {
      const entityNodes = layoutNodes(
        graph.graph.entities,
        graph.candidates,
        graph.worldId,
      );
      const allNodes = [
        ...anchorNodes(graph.graph.source_anchors),
        ...entityNodes,
      ] as Node<WorldKbNodeData>[];
      const relEdges = deriveRelationshipEdges(
        graph.graph.relationships,
      ) as Edge<WorldKbEdgeData>[];
      const visibleRelEdges = filterRelationshipEdgesByConfidence(
        relEdges,
        graph.confidenceThreshold,
      ) as Edge<WorldKbEdgeData>[];
      return {
        nodes: allNodes,
        edges: [
          ...(deriveEdges(graph.graph.source_anchors) as Edge<WorldKbEdgeData>[]),
          ...visibleRelEdges,
        ],
      };
    },

    adaptConflict(_error) {
      // World KB conflicts are write-boundary (entity / promote / relationship)
      // and are handled by WorldKbCanvasConflicts; no single query-level conflict
      // modal exists for this surface.
      return null as ConflictModalProps | null;
    },

    renderInspector(_node) {
      return <WorldKbInspectorWrapper ctxRef={ctxRef} />;
    },

    renderAltView() {
      return <WorldKbAltViewWrapper ctxRef={ctxRef} />;
    },

    summarizeGraph(graph) {
      return graphSummary(graph.graph, graph.candidates.length);
    },
  };
}

/** Adapter-driven inspector for node-based selections (entity / candidate). */
function WorldKbInspectorWrapper({
  ctxRef,
}: {
  ctxRef: MutableRefObject<WorldKbCanvasAdapterContext>;
}) {
  const ctx = ctxRef.current;
  const selection = ctx.selection;
  if (selection?.kind === 'entity') {
    return (
      <EntityInspector
        worldId={ctx.worldId}
        node={selection.node}
        entity={selection.entity}
        onConflict={ctx.onEntityConflict}
        reseedSignal={ctx.reseedSignal}
      />
    );
  }
  if (selection?.kind === 'candidate') {
    return (
      <PromotionInspector
        worldId={ctx.worldId}
        node={selection.node}
        candidate={selection.candidate}
        confirmedEntities={ctx.confirmedEntities}
        onConflict={ctx.onPromoteConflict}
        reseedSignal={ctx.reseedSignal}
      />
    );
  }
  return null;
}

/** Adapter-driven alt view; reads current nodes/context at render time. */
function WorldKbAltViewWrapper({
  ctxRef,
}: {
  ctxRef: MutableRefObject<WorldKbCanvasAdapterContext>;
}) {
  const ctx = ctxRef.current;
  return (
    <WorldKbAltView
      nodes={nodesToData(ctx.nodes ?? [])}
      relationships={ctx.relationships}
      entities={ctx.confirmedEntities}
      selectedNodeId={ctx.selectedNodeId}
      selectedRelationshipId={ctx.selectedRelationshipId}
      onSelectNode={ctx.onSelectNode}
      onSelectRelationship={ctx.onSelectRelationship}
      onCreateRelationship={ctx.onCreateRelationship}
      onDeleteRelationship={ctx.onDeleteRelationship}
      onPromoteSuggestion={ctx.onPromoteSuggestion}
      onDeleteSuggestion={ctx.onDeleteSuggestion}
      onPromoteAllSuggestions={ctx.onPromoteAllSuggestions}
      suggestionPending={ctx.patchRelationshipIsPending}
      onActiveTabChange={ctx.onActiveTabChange}
    />
  );
}
