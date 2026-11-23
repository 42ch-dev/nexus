/**
 * Thin composition root that coordinates graph read, candidate read, entity
 * promotion, and conflict resolution. Implementation detail lives in split
 * modules under `world-kb/`: header, inspector panel, conflict hosts, graph
 * projection, and alt view. Public exports (`WorldKbCanvas`, `patchFromForm`,
 * `EntityField`) are preserved for existing consumers.
 */
import { useEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import type { Connection } from '@xyflow/react';

import { CanvasShell } from '@/components/canvas/canvas-shell';
import { ErrorState, LoadingState } from '@/components/ui/states';
import {
  usePatchWorldKbRelationship,
  useWorldKbCandidates,
  useWorldKbChangesWatch,
  useWorldKbGraph,
  isWorldKbConflictError,
} from '@/lib/canvas/use-world-kb-data';
import { useRegisterCommand } from '@/lib/canvas/command-registry';
import { useCanvasSurface, type CanvasSurfaceQueryResult } from '@/components/canvas/use-canvas-surface';

import { buildRelationshipRemoveRequest } from './relationship-inspector-logic';
import { WorldKbCanvasConflicts } from './world-kb-canvas-conflicts';
import { WorldKbHeader } from './world-kb-canvas-header';
import { InspectorPanel } from './world-kb-inspector-panel';
import {
  useWorldKbCanvasState,
  buildEntityConflict,
  handleRelationshipConflict,
  handlePromoteConflict,
} from './use-world-kb-canvas-state';
import { formatRelative } from './world-kb-canvas-utils';
import { useReducedMotionPreference } from './use-view-preference';
import type { WorldKbNodeData } from './types';
import type { WorldKbRelationshipProjection } from '@42ch/nexus-contracts';

import {
  createWorldKbCanvasAdapter,
  type WorldKbCanvasAdapterContext,
  type WorldKbSurfaceGraph,
} from './world-kb-canvas-adapter';

export type { EntityField } from './world-kb-canvas-types';
export { patchFromForm } from './world-kb-canvas-utils';

const PROMOTE_BATCH_SIZE = 5;

export interface WorldKbCanvasProps {
  worldId: string;
  watchCoreChanges?: boolean;
}

function WorldKbChangesWatch({ worldId }: { worldId: string }) {
  useWorldKbChangesWatch(worldId, true);
  return null;
}

export function WorldKbCanvas({ worldId, watchCoreChanges = false }: WorldKbCanvasProps) {
  const { t } = useTranslation('canvas');
  const prefersReducedMotion = useReducedMotionPreference();
  const [showList, setShowList] = useState<boolean>(prefersReducedMotion);

  const [altTab, setAltTab] = useState<'entities' | 'relationships' | 'suggested'>('entities');
  const includeSuggested = showList && altTab === 'suggested';

  // Functional `setShowList(v => !v)` so the mount-captured handler reads
  // current state. No node-create command: the World KB canvas exposes no
  // clean canvas-level creation button (relationships are created via graph
  // edge-drag or alt-view row action, both of which require entity context).
  useRegisterCommand({
    id: 'world-kb.toggle-view',
    labelKey: 'world-kb.toggle-view.label',
    groupKey: 'group.world-kb',
    keywordKeys: [
      'world-kb.toggle-view.keywords.graph',
      'world-kb.toggle-view.keywords.list',
      'world-kb.toggle-view.keywords.alt-view',
      'world-kb.toggle-view.keywords.switch',
    ],
    handler: () => setShowList((v) => !v),
  });

  const graph = useWorldKbGraph(worldId, includeSuggested);
  const candidates = useWorldKbCandidates(worldId);
  const patchRelationship = usePatchWorldKbRelationship(worldId);

  const entities = graph.data?.entities ?? [];
  const candidateItems = candidates.data?.items ?? [];
  const anchors = graph.data?.source_anchors ?? [];
  const relationships = graph.data?.relationships ?? [];

  const canvasState = useWorldKbCanvasState({
    entities,
    candidateItems,
    relationships,
  });

  const [confidenceThreshold, setConfidenceThreshold] = useState(0);

  const surfaceQuery = useMemo<CanvasSurfaceQueryResult<WorldKbSurfaceGraph>>(() => {
    const data = graph.data;
    if (!data) {
      return {
        data: undefined,
        isLoading: graph.isLoading,
        isError: graph.isError,
        error: graph.error,
        refetch: () => {
          void graph.refetch();
          void candidates.refetch();
        },
      };
    }
    return {
      data: {
        worldId,
        graph: data,
        candidates: candidateItems,
        confidenceThreshold,
      },
      isLoading: graph.isLoading,
      isError: graph.isError,
      error: graph.error,
      refetch: () => {
        void graph.refetch();
        void candidates.refetch();
      },
    };
  }, [
    graph.data,
    graph.isLoading,
    graph.isError,
    graph.error,
    graph.refetch,
    candidates.refetch,
    candidateItems,
    worldId,
    confidenceThreshold,
  ]);

  const ctxRef = useRef<WorldKbCanvasAdapterContext>({
    worldId,
    selection: null,
    entities: [],
    candidates: [],
    confirmedEntities: [],
    anchors: [],
    relationships: [],
    reseedSignal: 0,
    onEntityConflict: () => {},
    onPromoteConflict: () => {},
    onRelationshipConflict: () => {},
    onRelationshipSaved: () => {},
    onSelectNode: () => {},
    onSelectRelationship: () => {},
    onCreateRelationship: () => {},
    onDeleteRelationship: () => {},
    onPromoteSuggestion: () => {},
    onDeleteSuggestion: () => {},
    onPromoteAllSuggestions: async () => ({ succeeded: 0, failed: 0 }),
    patchRelationshipIsPending: false,
    onActiveTabChange: () => {},
    selectedNodeId: null,
    selectedRelationshipId: null,
    nodes: [],
  });

  const adapter = useMemo(() => createWorldKbCanvasAdapter(ctxRef), []);

  const surface = useCanvasSurface(adapter, surfaceQuery);

  useEffect(() => {
    if (showList) return;
    const selected = surface.selectedNode;
    if (!selected) return;
    canvasState.onSelectNode(selected.data as unknown as WorldKbNodeData);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [surface.selectedNode, showList]);

  useEffect(() => {
    function onConnectTo(event: Event) {
      const custom = event as CustomEvent<{ sourceEntityId: string }>;
      const sourceEntityId = custom.detail.sourceEntityId;
      if (!sourceEntityId) return;
      canvasState.setSelection({
        kind: 'new-relationship',
        initialSourceEntityId: sourceEntityId,
      });
    }
    window.addEventListener('world-kb-connect-to', onConnectTo);
    return () => window.removeEventListener('world-kb-connect-to', onConnectTo);
  }, [canvasState.setSelection]);

  if (graph.isLoading || candidates.isLoading) return <LoadingState label={t('worldKb.loading')} />;
  if (graph.isError)
    return (
      <ErrorState description={t('worldKb.loadError')} onRetry={() => graph.refetch()} />
    );

  const confirmedEntities = entities.filter((e) => e.status?.toLowerCase() !== 'rejected');

  const handleEntityConflict = (payload: Parameters<typeof buildEntityConflict>[1]) =>
    canvasState.setEntityConflict(buildEntityConflict(canvasState.selection, payload));
  const handleConnect = ({ source, target }: Connection) => {
    const sourceId = source?.startsWith('entity:') ? source.slice('entity:'.length) : undefined;
    const targetId = target?.startsWith('entity:') ? target.slice('entity:'.length) : undefined;
    if (sourceId && targetId && sourceId !== targetId) {
      canvasState.onCreateRelationship({ sourceEntityId: sourceId, targetEntityId: targetId });
    }
  };
  const onPromoteConflict = (payload: Parameters<typeof handlePromoteConflict>[1]) =>
    handlePromoteConflict(canvasState.setPromoteConflict, payload);
  const onRelationshipConflict = (payload: Parameters<typeof handleRelationshipConflict>[1]) =>
    handleRelationshipConflict(canvasState.setRelationshipConflict, payload);
  const onRelationshipSaved = () => {
    canvasState.setSelection(null);
    canvasState.bumpReseed();
  };
  const onDeleteRelationship = (rel: WorldKbRelationshipProjection) => {
    patchRelationship.mutate(buildRelationshipRemoveRequest(rel), {
      onSuccess: () => {
        if (
          canvasState.selection?.kind === 'relationship' &&
          canvasState.selection.relationship.relationship_id === rel.relationship_id
        ) {
          canvasState.setSelection(null);
        }
        canvasState.bumpReseed();
      },
      onError: (error) => {
        if (isWorldKbConflictError(error)) {
          if (
            canvasState.selection?.kind === 'relationship' &&
            canvasState.selection.relationship.relationship_id === rel.relationship_id
          ) {
            canvasState.setSelection(null);
          }
        }
      },
    });
  };

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
      { onSuccess: () => canvasState.bumpReseed() },
    );
  };
  const onDeleteSuggestion = onDeleteRelationship;
  const onPromoteAllSuggestions = async (
    rels: WorldKbRelationshipProjection[],
  ): Promise<{ succeeded: number; failed: number }> => {
    const results: PromiseSettledResult<unknown>[] = [];
    for (let i = 0; i < rels.length; i += PROMOTE_BATCH_SIZE) {
      const batch = rels.slice(i, i + PROMOTE_BATCH_SIZE);
      const settled = await Promise.allSettled(
        batch.map((rel) =>
          patchRelationship.mutateAsync({
            relationship_id: rel.relationship_id,
            action: 'update' as const,
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
          }),
        ),
      );
      results.push(...settled);
    }
    const failed = results.filter(
      (r): r is PromiseRejectedResult => r.status === 'rejected',
    ).length;
    const succeeded = results.length - failed;
    if (failed > 0) {
      console.warn(`promoteAll: ${failed}/${rels.length} suggestions failed`);
    }
    canvasState.bumpReseed();
    return { succeeded, failed };
  };

  ctxRef.current = {
    worldId,
    selection: canvasState.selection,
    entities,
    candidates: candidateItems,
    confirmedEntities,
    anchors,
    relationships,
    reseedSignal: canvasState.reseedSignal,
    onEntityConflict: handleEntityConflict,
    onPromoteConflict,
    onRelationshipConflict,
    onRelationshipSaved,
    onSelectNode: canvasState.onSelectNode,
    onSelectRelationship: canvasState.onSelectRelationship,
    onCreateRelationship: canvasState.onCreateRelationship,
    onDeleteRelationship,
    onPromoteSuggestion,
    onDeleteSuggestion,
    onPromoteAllSuggestions,
    patchRelationshipIsPending: patchRelationship.isPending,
    onActiveTabChange: setAltTab,
    selectedNodeId: canvasState.selectedNodeId,
    selectedRelationshipId: canvasState.selectedRelationshipId,
    nodes: surface.nodes,
  };

  const entryCount = (graph.data?.entities.length ?? 0) + candidateItems.length;
  const lastFetched = graph.dataUpdatedAt ? formatRelative(graph.dataUpdatedAt) : '—';

  const hasProjectableContent = confirmedEntities.length > 0 || candidateItems.length > 0;
  const graphPendingProjection = !showList && hasProjectableContent && surface.nodes.length === 0;

  const inspectorPanelProps = {
    selection: canvasState.selection,
    worldId,
    confirmedEntities,
    anchors,
    reseedSignal: canvasState.reseedSignal,
    onEntityConflict: handleEntityConflict,
    onPromoteConflict,
    onRelationshipConflict,
    onRelationshipSaved,
    nodeInspector: surface.inspector,
  };

  return (
    <div className="flex flex-col gap-4">
      {/* P4-T3: opt-in only — the default canvas never mounts this. */}
      {watchCoreChanges ? <WorldKbChangesWatch worldId={worldId} /> : null}
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
          {surface.altView}
          <InspectorPanel {...inspectorPanelProps} />
        </div>
      ) : graphPendingProjection ? (
        <LoadingState label={t('worldKb.loading')} />
      ) : (
        <CanvasShell
          nodes={surface.nodes}
          edges={surface.edges}
          nodeTypes={surface.nodeTypes}
          onNodesChange={surface.onNodesChange}
          onEdgeClick={canvasState.onEdgeClick}
          onConnect={handleConnect}
          summaryText={surface.summaryText}
          ariaLabel={t('worldKb.graphAriaLabel')}
          minimapAccent="var(--color-canvas-worldkb-accent)"
        >
          <div className="pointer-events-none absolute inset-0" />
          {/* V1.76: confidence threshold filter (confirmed edges below the
              threshold are hidden; manual + suggested edges always show).
              Slider emits 0.0–1.0 with step 0.05 (21 steps) so the label
              tracks the same granularity as stored confidence values and the
              compass Phase 2b stepped bands at 0.4 / 0.7 without oversensitive
              micro-adjustments. */}
          <div className="pointer-events-auto absolute left-3 top-3 flex items-center gap-2 rounded-card border border-gray-alpha-400 bg-background-100 px-3 py-2 shadow-card">
            <label
              htmlFor="kb-confidence-threshold"
              className="text-label-12 text-gray-700"
            >
              {t('worldKb.confidenceLabel', { value: confidenceThreshold.toFixed(2) })}
            </label>
            <input
              id="kb-confidence-threshold"
              type="range"
              min={0}
              max={1}
              step={0.05}
              value={confidenceThreshold}
              onChange={(e) => setConfidenceThreshold(Number(e.target.value))}
              className="h-1 w-32 cursor-pointer accent-canvas-strategy-accent"
              aria-label={t('worldKb.confidenceThresholdAria')}
            />
          </div>
          <div className="pointer-events-auto absolute right-3 top-3 w-[340px] max-w-[calc(100%-1.5rem)] rounded-card border border-gray-alpha-400 bg-background-100 p-4 shadow-popover">
            <InspectorPanel {...inspectorPanelProps} />
          </div>
        </CanvasShell>
      )}

      <WorldKbCanvasConflicts
        entityConflict={canvasState.entityConflict}
        promoteConflict={canvasState.promoteConflict}
        relationshipConflict={canvasState.relationshipConflict}
        selection={canvasState.selection}
        worldId={worldId}
        confirmedEntities={confirmedEntities}
        setEntityConflict={canvasState.setEntityConflict}
        setPromoteConflict={canvasState.setPromoteConflict}
        setRelationshipConflict={canvasState.setRelationshipConflict}
        bumpReseed={canvasState.bumpReseed}
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
