/**
 * World KB canvas — orchestrator facade (V1.74 A10 split).
 *
 * Thin composition root that coordinates graph read, candidate read, entity
 * promotion, and conflict resolution. Implementation detail lives in split
 * modules under `world-kb/`: header, inspector panel, conflict hosts, graph
 * projection, and alt view. Public exports (`WorldKbCanvas`, `patchFromForm`,
 * `EntityField`) are preserved for existing consumers.
 */
import { useEffect, useMemo, useState } from 'react';
import type { Connection, Node } from '@xyflow/react';

import { CanvasShell, useNodeChangeHandler } from '@/components/canvas/canvas-shell';
import { ErrorState, LoadingState } from '@/components/ui/states';
import {
  usePatchWorldKbRelationship,
  useWorldKbCandidates,
  useWorldKbGraph,
  isWorldKbConflictError,
} from '@/lib/canvas/use-world-kb-data';

import { buildRelationshipRemoveRequest } from './relationship-inspector-logic';
import { worldKbNodeTypes } from './entity-node';
import { anchorNodes, deriveEdges, entryCountOf, graphSummary, layoutNodes } from './graph-projection';
import { deriveRelationshipEdges } from './relationship-projection';
import { WorldKbAltView } from './world-kb-alt-view';
import { WorldKbCanvasConflicts } from './world-kb-canvas-conflicts';
import { WorldKbHeader } from './world-kb-canvas-header';
import { InspectorPanel } from './world-kb-inspector-panel';
import { useWorldKbCanvasState, buildEntityConflict, handleRelationshipConflict, handlePromoteConflict } from './use-world-kb-canvas-state';
import { formatRelative, nodesToData } from './world-kb-canvas-utils';
import { useReducedMotionPreference } from './use-view-preference';
import type { WorldKbNodeData, WorldKbEdgeData } from './types';
import type { WorldKbRelationshipProjection } from '@42ch/nexus-contracts';

export type { EntityField } from './world-kb-canvas-types';
export { patchFromForm } from './world-kb-canvas-utils';

export interface WorldKbCanvasProps {
  worldId: string;
}

export function WorldKbCanvas({ worldId }: WorldKbCanvasProps) {
  const graph = useWorldKbGraph(worldId);
  const candidates = useWorldKbCandidates(worldId);
  const patchRelationship = usePatchWorldKbRelationship(worldId);

  // List view is the default for keyboard-only / screen-reader users.
  const prefersReducedMotion = useReducedMotionPreference();
  const [showList, setShowList] = useState<boolean>(prefersReducedMotion);

  const entities = graph.data?.entities ?? [];
  const candidateItems = candidates.data?.items ?? [];
  const anchors = graph.data?.source_anchors ?? [];
  const relationships = graph.data?.relationships ?? [];

  const {
    selection,
    setSelection,
    selectedNodeId,
    selectedRelationshipId,
    entityConflict,
    promoteConflict,
    relationshipConflict,
    reseedSignal,
    bumpReseed,
    setEntityConflict,
    setPromoteConflict,
    setRelationshipConflict,
    onSelectNode,
    onSelectRelationship,
    onCreateRelationship,
    onEdgeClick,
  } = useWorldKbCanvasState({ entities, candidateItems, relationships });

  // V1.76: confidence threshold for the graph view. Confirmed edges with
  // confidence below the threshold are hidden; manual edges (no confidence)
  // and suggested (needs_review) edges always show. Default 0.0 = show all.
  const [confidenceThreshold, setConfidenceThreshold] = useState(0);

  const projected = useMemo(() => {
    const entityNodes = layoutNodes(entities, candidateItems, worldId);
    const allNodes = [...anchorNodes(anchors), ...entityNodes] as Node[];
    const relEdges = deriveRelationshipEdges(relationships);
    // Apply the confidence threshold to confirmed relationship edges only.
    const threshold = confidenceThreshold;
    const visibleRelEdges =
      threshold > 0
        ? relEdges.filter((e) => {
            const data = e.data as WorldKbEdgeData | undefined;
            // Suggested edges always show; manual (no confidence) always show.
            if (data?.needsReview) return true;
            if (data?.confidence == null) return true;
            return data.confidence >= threshold;
          })
        : relEdges;
    return {
      nodes: allNodes,
      edges: [...deriveEdges(anchors), ...visibleRelEdges],
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [entities, candidateItems, anchors, relationships, worldId, confidenceThreshold]);

  // Hold nodes in local state so React Flow drag/select moves persist; reseed
  // when the server projection changes (refetch or selection-driven invalidation).
  const [nodes, setNodes] = useState<Node[]>(projected.nodes);
  const edges = projected.edges;
  useEffect(() => {
    setNodes(projected.nodes);
  }, [projected.nodes]);
  const onNodesChange = useNodeChangeHandler(setNodes);

  // Graph mode: React Flow tracks selection via the node `selected` flag (set
  // through onNodesChange). Resolve it to a World KB selection so the inspector
  // updates from graph clicks just like alt-view row activation.
  useEffect(() => {
    if (showList) return;
    const selected = nodes.find((n) => n.selected && n.type === 'worldkb-entity');
    if (!selected) return;
    onSelectNode(selected.data as unknown as WorldKbNodeData);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [nodes, showList]);

  useEffect(() => {
    function onConnectTo(event: Event) {
      const custom = event as CustomEvent<{ sourceEntityId: string }>;
      const sourceEntityId = custom.detail.sourceEntityId;
      if (!sourceEntityId) return;
      setSelection({
        kind: 'new-relationship',
        initialSourceEntityId: sourceEntityId,
      });
    }
    window.addEventListener('world-kb-connect-to', onConnectTo);
    return () => window.removeEventListener('world-kb-connect-to', onConnectTo);
  }, [setSelection]);

  if (graph.isLoading || candidates.isLoading) return <LoadingState label="Loading World KB…" />;
  if (graph.isError)
    return (
      <ErrorState description="Could not load the World KB graph." onRetry={() => graph.refetch()} />
    );

  const summary = graphSummary(graph.data, candidateItems.length);
  const entryCount = entryCountOf(graph.data, candidateItems.length);
  const lastFetched = graph.dataUpdatedAt ? formatRelative(graph.dataUpdatedAt) : '—';
  const confirmedEntities = entities.filter((e) => e.status?.toLowerCase() !== 'rejected');

  const handleEntityConflict = (payload: Parameters<typeof buildEntityConflict>[1]) =>
    setEntityConflict(buildEntityConflict(selection, payload));
  const handleConnect = ({ source, target }: Connection) => {
    const sourceId = source?.startsWith('entity:') ? source.slice('entity:'.length) : undefined;
    const targetId = target?.startsWith('entity:') ? target.slice('entity:'.length) : undefined;
    if (sourceId && targetId && sourceId !== targetId) {
      onCreateRelationship({ sourceEntityId: sourceId, targetEntityId: targetId });
    }
  };
  const onPromoteConflict = (payload: Parameters<typeof handlePromoteConflict>[1]) =>
    handlePromoteConflict(setPromoteConflict, payload);
  const onRelationshipConflict = (payload: Parameters<typeof handleRelationshipConflict>[1]) =>
    handleRelationshipConflict(setRelationshipConflict, payload);
  const onRelationshipSaved = () => {
    setSelection(null);
    bumpReseed();
  };
  const onDeleteRelationship = (rel: WorldKbRelationshipProjection) => {
    patchRelationship.mutate(buildRelationshipRemoveRequest(rel), {
      onSuccess: () => {
        if (selection?.kind === 'relationship' && selection.relationship.relationship_id === rel.relationship_id) {
          setSelection(null);
        }
        bumpReseed();
      },
      onError: (error) => {
        // A 409 on delete = the relationship changed concurrently. The hook's
        // global onError already refetches the graph to canonical state; here we
        // clear the selection so the inspector does not keep editing a stale row.
        if (isWorldKbConflictError(error)) {
          if (selection?.kind === 'relationship' && selection.relationship.relationship_id === rel.relationship_id) {
            setSelection(null);
          }
        }
      },
    });
  };

  // V1.76: promote an extraction suggestion (clear needs_review) via the
  // existing patch-relationship update route — no second promotion state machine.
  const onPromoteSuggestion = (rel: WorldKbRelationshipProjection) => {
    patchRelationship.mutate(
      {
        relationship_id: rel.relationship_id,
        action: 'update',
        expected_version: rel.version,
        relationship: {
          source_entity_id: rel.source_entity_id,
          target_entity_id: rel.target_entity_id,
          relation_type: rel.relation_type,
          custom_label: rel.custom_label,
          symmetric: rel.symmetric,
          confidence: rel.confidence,
          source_anchor_ids: rel.source_anchor_ids,
          metadata: rel.metadata,
          needs_review: false,
        },
      },
      { onSuccess: () => bumpReseed() },
    );
  };
  const onDeleteSuggestion = onDeleteRelationship;
  const onPromoteAllSuggestions = (rels: WorldKbRelationshipProjection[]) => {
    for (const rel of rels) {
      onPromoteSuggestion(rel);
    }
  };

  const inspectorPanelProps = {
    selection,
    worldId,
    confirmedEntities,
    anchors,
    reseedSignal,
    onEntityConflict: handleEntityConflict,
    onPromoteConflict,
    onRelationshipConflict,
    onRelationshipSaved,
  };

  return (
    <div className="flex flex-col gap-4">
      <WorldKbHeader
        entryCount={entryCount}
        lastFetched={lastFetched}
        showList={showList}
        onToggleView={() => setShowList((v) => !v)}
        onRefresh={() => {
          void graph.refetch();
          void candidates.refetch();
        }}
        refreshing={graph.isFetching}
      />

      {showList ? (
        <div className="grid gap-4 lg:grid-cols-[1fr_360px]">
          <WorldKbAltView
            nodes={nodesToData(nodes)}
            relationships={relationships}
            entities={confirmedEntities}
            selectedNodeId={selectedNodeId}
            selectedRelationshipId={selectedRelationshipId}
            onSelectNode={(n) => onSelectNode(n)}
            onSelectRelationship={onSelectRelationship}
            onCreateRelationship={onCreateRelationship}
            onDeleteRelationship={onDeleteRelationship}
            onPromoteSuggestion={onPromoteSuggestion}
            onDeleteSuggestion={onDeleteSuggestion}
            onPromoteAllSuggestions={onPromoteAllSuggestions}
            suggestionPending={patchRelationship.isPending}
          />
          <InspectorPanel {...inspectorPanelProps} />
        </div>
      ) : (
        <CanvasShell
          nodes={nodes}
          edges={edges}
          nodeTypes={worldKbNodeTypes}
          onNodesChange={onNodesChange}
          onEdgeClick={onEdgeClick}
          onConnect={handleConnect}
          summaryText={summary}
          ariaLabel="World KB entity graph"
        >
          <div className="pointer-events-none absolute inset-0" />
          {/* V1.76: confidence threshold filter (confirmed edges below the
              threshold are hidden; manual + suggested edges always show). */}
          <div className="pointer-events-auto absolute left-3 top-3 flex items-center gap-2 rounded-card border border-gray-alpha-400 bg-background-100 px-3 py-2 shadow-card">
            <label
              htmlFor="kb-confidence-threshold"
              className="text-label-12 text-gray-700"
            >
              Confidence ≥ {(confidenceThreshold / 100).toFixed(2)}
            </label>
            <input
              id="kb-confidence-threshold"
              type="range"
              min={0}
              max={100}
              step={5}
              value={confidenceThreshold}
              onChange={(e) => setConfidenceThreshold(Number(e.target.value))}
              className="h-1 w-32 cursor-pointer accent-canvas-strategy-accent"
              aria-label="Minimum confidence threshold for confirmed relationship edges"
            />
          </div>
          <div className="pointer-events-auto absolute right-3 top-3 w-[340px] max-w-[calc(100%-1.5rem)] rounded-card border border-gray-alpha-400 bg-background-100 p-4 shadow-popover">
            <InspectorPanel {...inspectorPanelProps} />
          </div>
        </CanvasShell>
      )}

      <WorldKbCanvasConflicts
        entityConflict={entityConflict}
        promoteConflict={promoteConflict}
        relationshipConflict={relationshipConflict}
        selection={selection}
        worldId={worldId}
        confirmedEntities={confirmedEntities}
        setEntityConflict={setEntityConflict}
        setPromoteConflict={setPromoteConflict}
        setRelationshipConflict={setRelationshipConflict}
        bumpReseed={bumpReseed}
        refetchGraph={() => {
          void graph.refetch();
        }}
        refetchCandidates={() => {
          void candidates.refetch();
        }}
      />
    </div>
  );
}
