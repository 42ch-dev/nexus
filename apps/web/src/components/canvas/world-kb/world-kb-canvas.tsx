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

import { CanvasShell, useNodeChangeHandler } from '@/components/canvas/canvas-shell';
import { ErrorState, LoadingState } from '@/components/ui/states';
import {
  useWorldKbCandidates,
  useWorldKbGraph,
} from '@/lib/canvas/use-world-kb-data';
import type { Node } from '@xyflow/react';

import { worldKbNodeTypes } from './entity-node';
import { anchorNodes, deriveEdges, entryCountOf, graphSummary, layoutNodes } from './graph-projection';
import { WorldKbAltView } from './world-kb-alt-view';
import { EntityConflictHost, PromoteConflictHost } from './world-kb-conflict-hosts';
import { WorldKbHeader } from './world-kb-canvas-header';
import { InspectorPanel } from './world-kb-inspector-panel';
import {
  type EntityConflictState,
  type PromoteConflictState,
  type Selection,
} from './world-kb-canvas-types';
import { formatRelative, nodesToData } from './world-kb-canvas-utils';
import { useReducedMotionPreference } from './use-view-preference';
import { worldKbNodeId, type WorldKbNodeData } from './types';

export type { EntityField } from './world-kb-canvas-types';
export { patchFromForm } from './world-kb-canvas-utils';

export interface WorldKbCanvasProps {
  worldId: string;
}

export function WorldKbCanvas({ worldId }: WorldKbCanvasProps) {
  const graph = useWorldKbGraph(worldId);
  const candidates = useWorldKbCandidates(worldId);

  const [selection, setSelection] = useState<Selection>(null);
  const [entityConflict, setEntityConflict] = useState<EntityConflictState | null>(null);
  const [promoteConflict, setPromoteConflict] = useState<PromoteConflictState | null>(null);
  const [reseedSignal, setReseedSignal] = useState(0);

  // List view is the default for keyboard-only / screen-reader users.
  const prefersReducedMotion = useReducedMotionPreference();
  const [showList, setShowList] = useState<boolean>(prefersReducedMotion);

  const entities = graph.data?.entities ?? [];
  const candidateItems = candidates.data?.items ?? [];
  const anchors = graph.data?.source_anchors ?? [];

  const projected = useMemo(() => {
    const entityNodes = layoutNodes(entities, candidateItems, worldId);
    const allNodes = [...anchorNodes(anchors), ...entityNodes] as Node[];
    return { nodes: allNodes, edges: deriveEdges(anchors) };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [entities, candidateItems, anchors, worldId]);

  // Hold nodes in local state so React Flow drag/select moves persist; reseed
  // when the server projection changes (refetch or selection-driven invalidation).
  const [nodes, setNodes] = useState<Node[]>(projected.nodes);
  const edges = projected.edges;
  useEffect(() => {
    setNodes(projected.nodes);
  }, [projected.nodes]);
  const onNodesChange = useNodeChangeHandler(setNodes);

  // Reset selection when the graph refetch drops the backing projection.
  useEffect(() => {
    if (!selection) return;
    if (selection.kind === 'entity') {
      const fresh = entities.find((e) => e.key_block_id === selection.entity.key_block_id);
      if (!fresh) setSelection(null);
    } else {
      const fresh = candidateItems.find((c) => c.candidate_id === selection.candidate.candidate_id);
      if (!fresh) setSelection(null);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [entities, candidateItems]);

  /** Universal selection setter from a node data payload (alt-view + graph). */
  function onSelectNode(node: WorldKbNodeData) {
    if (node.candidateId) {
      const candidate = candidateItems.find((c) => c.candidate_id === node.candidateId);
      if (candidate) setSelection({ kind: 'candidate', node, candidate });
    } else if (node.keyBlockId) {
      const entity = entities.find((e) => e.key_block_id === node.keyBlockId);
      if (entity) setSelection({ kind: 'entity', node, entity });
    }
  }

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

  if (graph.isLoading || candidates.isLoading) return <LoadingState label="Loading World KB…" />;
  if (graph.isError)
    return (
      <ErrorState description="Could not load the World KB graph." onRetry={() => graph.refetch()} />
    );

  const summary = graphSummary(graph.data, candidateItems.length);
  const entryCount = entryCountOf(graph.data, candidateItems.length);
  const lastFetched = graph.dataUpdatedAt ? formatRelative(graph.dataUpdatedAt) : '—';
  const confirmedEntities = entities.filter((e) => e.status?.toLowerCase() !== 'rejected');

  /** Build the KB-flavored entity conflict state from an inspector conflict payload. */
  function buildEntityConflict(payload: {
    currentVersion: number;
    entityId: string;
    conflictingPath: string;
    draft: import('./entity-inspector').EntityEditForm;
    dirtyFields: ('title' | 'body' | 'aliases' | 'block_type')[];
  }): EntityConflictState {
    const entityName = selection?.kind === 'entity' ? selection.node.name : 'this entity';
    const draftValues: Partial<Record<'title' | 'body' | 'aliases' | 'block_type', string>> = {};
    if (payload.dirtyFields.includes('title')) draftValues.title = payload.draft.title;
    if (payload.dirtyFields.includes('block_type')) draftValues.block_type = payload.draft.block_type;
    if (payload.dirtyFields.includes('aliases')) draftValues.aliases = payload.draft.aliasesText;
    if (payload.dirtyFields.includes('body')) draftValues.body = payload.draft.bodyText;
    return {
      currentVersion: payload.currentVersion,
      reapplyForm: payload.draft,
      dirtyFields: payload.dirtyFields,
      modalDraft: {
        entityName,
        fields: payload.dirtyFields,
        // 409 details do not carry per-field server values; surface the path.
        changedFields: [],
        draftValues,
      },
    };
  }

  const handleEntityConflict = (payload: Parameters<typeof buildEntityConflict>[0]) =>
    setEntityConflict(buildEntityConflict(payload));
  const handlePromoteConflict = (payload: {
    currentVersion: number;
    candidateName: string;
    newStatus: 'adopted' | 'rejected' | 'merged';
    action: 'adopt' | 'reject' | 'merge';
    mergeTargetId?: string;
    mergeTargetLabel?: string;
  }) => setPromoteConflict({ currentVersion: payload.currentVersion, draft: payload });

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
            selectedId={selection ? worldKbNodeId(selection.node) : null}
            onSelect={(n) => onSelectNode(n)}
          />
          <InspectorPanel
            selection={selection}
            worldId={worldId}
            confirmedEntities={confirmedEntities}
            reseedSignal={reseedSignal}
            onEntityConflict={handleEntityConflict}
            onPromoteConflict={handlePromoteConflict}
          />
        </div>
      ) : (
        <CanvasShell
          nodes={nodes}
          edges={edges}
          nodeTypes={worldKbNodeTypes}
          onNodesChange={onNodesChange}
          summaryText={summary}
          ariaLabel="World KB entity graph"
        >
          <div className="pointer-events-none absolute inset-0" />
          <div className="pointer-events-auto absolute right-3 top-3 w-[340px] max-w-[calc(100%-1.5rem)] rounded-card border border-gray-alpha-400 bg-background-100 p-4 shadow-popover">
            <InspectorPanel
              selection={selection}
              worldId={worldId}
              confirmedEntities={confirmedEntities}
              reseedSignal={reseedSignal}
              onEntityConflict={handleEntityConflict}
              onPromoteConflict={handlePromoteConflict}
            />
          </div>
        </CanvasShell>
      )}

      <EntityConflictHost
        state={entityConflict}
        selection={selection}
        worldId={worldId}
        onUseCurrent={() => {
          setEntityConflict(null);
          setReseedSignal((s) => s + 1);
          void graph.refetch();
        }}
        onDismiss={() => setEntityConflict(null)}
        onResolved={() => {
          setEntityConflict(null);
          setReseedSignal((s) => s + 1);
        }}
      />

      <PromoteConflictHost
        state={promoteConflict}
        selection={selection}
        worldId={worldId}
        onUseCurrent={() => {
          setPromoteConflict(null);
          setReseedSignal((s) => s + 1);
          void graph.refetch();
          void candidates.refetch();
        }}
        onDismiss={() => setPromoteConflict(null)}
        onResolved={() => {
          setPromoteConflict(null);
          setReseedSignal((s) => s + 1);
        }}
      />
    </div>
  );
}
