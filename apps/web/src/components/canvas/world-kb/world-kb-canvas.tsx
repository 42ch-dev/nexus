/**
 * World KB canvas — orchestrator facade (V1.73 P0 A6–A8).
 *
 * Third adapter on the shared V1.70 Canvas Shell (canvas-strategy-surface.md
 * §3.3 surface 3 + §3.4 WorldKbNodeData/WorldKbEdgeData). Projects the World KB
 * graph (entities + source-anchor provenance) onto React Flow, wires the entity
 * inspector (`world_kb.patch_entity`) and promotion inspector
 * (`world_kb.promote_candidate`) with per-row OCC, renders the KB-flavored
 * conflict modal on 409, and offers a non-spatial alternate view with full
 * write parity. The command-palette freshness indicator shows entry count +
 * fetched staleness with an empty-state CTA.
 */
import { useEffect, useMemo, useState } from 'react';
import { RefreshCw, List, Workflow } from 'lucide-react';

import { CanvasShell } from '@/components/canvas/canvas-shell';
import { Button } from '@/components/ui/button';
import { ErrorState, LoadingState } from '@/components/ui/states';
import { useNodeChangeHandler } from '@/components/canvas/canvas-shell';
import { worldKbNodeTypes } from './entity-node';
import { WorldKbAltView } from './world-kb-alt-view';
import { EntityInspector, type EntityEditForm } from './entity-inspector';
import { PromotionInspector } from './promotion-inspector';
import {
  WorldKbEntityConflictModal,
  WorldKbPromoteConflictModal,
  type WorldKbEntityConflictDraft,
  type WorldKbPromoteConflictDraft,
} from './world-kb-conflict-modal';
import {
  anchorNodes,
  deriveEdges,
  entryCountOf,
  graphSummary,
  layoutNodes,
} from './graph-projection';
import {
  useWorldKbCandidates,
  useWorldKbGraph,
  usePatchWorldKbEntity,
  usePromoteWorldKbCandidate,
  isWorldKbConflictError,
} from '@/lib/canvas/use-world-kb-data';
import { useReducedMotionPreference } from './use-view-preference';
import { worldKbNodeId, type WorldKbNodeData } from './types';
import type { Node } from '@xyflow/react';
import type {
  WorldKbCandidateProjection,
  WorldKbEntityProjection,
} from '@42ch/nexus-contracts';

export interface WorldKbCanvasProps {
  worldId: string;
}

type Selection =
  | { kind: 'entity'; node: WorldKbNodeData; entity: WorldKbEntityProjection }
  | { kind: 'candidate'; node: WorldKbNodeData; candidate: WorldKbCandidateProjection }
  | null;

type EntityField = 'title' | 'body' | 'aliases' | 'block_type';

interface EntityConflictState {
  /** Modal draft (KB-flavored copy) — includes entityName. */
  modalDraft: WorldKbEntityConflictDraft;
  /** Raw form captured at conflict time, used to reapply the user's edit. */
  reapplyForm: EntityEditForm;
  dirtyFields: EntityField[];
  currentVersion: number;
}
interface PromoteConflictState {
  draft: WorldKbPromoteConflictDraft;
  currentVersion: number;
}

export function WorldKbCanvas({ worldId }: WorldKbCanvasProps) {
  const graph = useWorldKbGraph(worldId);
  const candidates = useWorldKbCandidates(worldId);
  const patchEntity = usePatchWorldKbEntity(worldId);
  const promoteCandidate = usePromoteWorldKbCandidate(worldId);

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
    draft: EntityEditForm;
    dirtyFields: EntityField[];
  }): EntityConflictState {
    const entityName = selection?.kind === 'entity' ? selection.node.name : 'this entity';
    const draftValues: Partial<Record<EntityField, string>> = {};
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
        onUseCurrent={() => {
          setEntityConflict(null);
          setReseedSignal((s) => s + 1);
          void graph.refetch();
        }}
        onReapply={() => {
          if (!entityConflict || !selection || selection.kind !== 'entity') return;
          patchEntity.mutate(
            {
              entity_id: selection.entity.key_block_id,
              expected_version: entityConflict.currentVersion,
              patch: patchFromForm(entityConflict.reapplyForm, entityConflict.dirtyFields),
            },
            {
              onSuccess: () => setEntityConflict(null),
              onError: (error) => {
                if (isWorldKbConflictError(error)) {
                  const details = error.details;
                  setEntityConflict({
                    ...entityConflict,
                    currentVersion: details.current_version,
                  });
                }
              },
            },
          );
        }}
        onDismiss={() => setEntityConflict(null)}
      />

      <PromoteConflictHost
        state={promoteConflict}
        selection={selection}
        onUseCurrent={() => {
          setPromoteConflict(null);
          setReseedSignal((s) => s + 1);
          void graph.refetch();
          void candidates.refetch();
        }}
        onReapply={() => {
          if (!promoteConflict || !selection || selection.kind !== 'candidate') return;
          promoteCandidate.mutate(
            {
              job_id: selection.candidate.job_id,
              candidate_id: selection.candidate.candidate_id,
              action: promoteConflict.draft.action,
              expected_version: promoteConflict.currentVersion,
              merge_target_id:
                promoteConflict.draft.action === 'merge'
                  ? promoteConflict.draft.mergeTargetId
                  : undefined,
            },
            {
              onSuccess: () => setPromoteConflict(null),
            },
          );
        }}
        onDismiss={() => setPromoteConflict(null)}
      />
    </div>
  );
}

/** Extract node data for the alt view (filters to entity/candidate nodes only). */
function nodesToData(nodes: Node[]): WorldKbNodeData[] {
  return nodes
    .filter((n) => n.type === 'worldkb-entity')
    .map((n) => n.data as unknown as WorldKbNodeData)
    .filter(Boolean);
}

/** Build a patch payload from the captured conflict form + dirty fields. */
function patchFromForm(form: EntityEditForm, dirty: EntityField[]) {
  const patch: {
    title?: string;
    body?: Record<string, unknown>;
    aliases?: string[];
    block_type?: EntityEditForm['block_type'];
  } = {};
  if (dirty.includes('title')) patch.title = form.title.trim() || undefined;
  if (dirty.includes('block_type')) patch.block_type = form.block_type;
  if (dirty.includes('aliases')) {
    patch.aliases = form.aliasesText
      .split(',')
      .map((a: string) => a.trim())
      .filter(Boolean);
  }
  if (dirty.includes('body')) {
    patch.body = form.bodyText.trim() ? safeJson(form.bodyText) : undefined;
  }
  return patch;
}

function safeJson(text: string): Record<string, unknown> | undefined {
  try {
    return JSON.parse(text);
  } catch {
    return undefined;
  }
}

function formatRelative(ts: number): string {
  const diff = Date.now() - ts;
  const mins = Math.round(diff / 60_000);
  if (mins < 1) return 'just now';
  if (mins < 60) return `${mins}m ago`;
  const hrs = Math.round(mins / 60);
  return hrs < 24 ? `${hrs}h ago` : `${Math.round(hrs / 24)}d ago`;
}

function WorldKbHeader({
  entryCount,
  lastFetched,
  showList,
  onToggleView,
  onRefresh,
  refreshing,
}: {
  entryCount: number;
  lastFetched: string;
  showList: boolean;
  onToggleView: () => void;
  onRefresh: () => void;
  refreshing: boolean;
}) {
  return (
    <div className="flex flex-wrap items-center justify-between gap-2">
      <div>
        <div className="flex items-center gap-2">
          <h2 className="text-heading-20 font-heading text-gray-1000">World KB</h2>
          {entryCount === 0 ? (
            <span className="rounded-pill bg-gray-alpha-100 px-2 py-0.5 text-label-12 text-gray-700">
              no entries yet
            </span>
          ) : (
            <span className="rounded-pill bg-gray-alpha-100 px-2 py-0.5 text-label-12 text-gray-700">
              {entryCount} {entryCount === 1 ? 'entry' : 'entries'} · fetched {lastFetched}
            </span>
          )}
        </div>
        <p className="text-copy-13 text-gray-700">
          {entryCount === 0
            ? 'Start adding characters, abilities, or scenes from the command palette (kb adopt/snapshot).'
            : 'Browse entities and promotion candidates. Edits are guarded by per-row version checks.'}
        </p>
      </div>
      <div className="flex items-center gap-2">
        <Button
          type="button"
          variant="tertiary"
          size="small"
          onClick={onToggleView}
          aria-pressed={showList}
        >
          {showList ? (
            <>
              <Workflow className="h-4 w-4" aria-hidden /> Show graph
            </>
          ) : (
            <>
              <List className="h-4 w-4" aria-hidden /> Show list view
            </>
          )}
        </Button>
        <Button type="button" variant="secondary" size="small" onClick={onRefresh} disabled={refreshing}>
          <RefreshCw className={`h-4 w-4 ${refreshing ? 'animate-spin' : ''}`} aria-hidden />
          Refresh now
        </Button>
      </div>
    </div>
  );
}

function InspectorPanel({
  selection,
  worldId,
  confirmedEntities,
  reseedSignal,
  onEntityConflict,
  onPromoteConflict,
}: {
  selection: Selection;
  worldId: string;
  confirmedEntities: WorldKbEntityProjection[];
  reseedSignal: number;
  onEntityConflict: (payload: {
    currentVersion: number;
    entityId: string;
    conflictingPath: string;
    draft: import('./entity-inspector').EntityEditForm;
    dirtyFields: ('title' | 'body' | 'aliases' | 'block_type')[];
  }) => void;
  onPromoteConflict: (payload: {
    currentVersion: number;
    candidateName: string;
    newStatus: 'adopted' | 'rejected' | 'merged';
    action: 'adopt' | 'reject' | 'merge';
    mergeTargetId?: string;
    mergeTargetLabel?: string;
  }) => void;
}) {
  if (!selection) {
    return (
      <aside
        aria-label="World KB inspector"
        className="rounded-card border border-gray-alpha-400 bg-background-100 p-4 text-copy-13 text-gray-700 shadow-card"
      >
        Select an entity or candidate to inspect it.
      </aside>
    );
  }
  if (selection.kind === 'entity') {
    return (
      <aside
        aria-label={`Entity inspector: ${selection.node.name}`}
        className="rounded-card border border-gray-alpha-400 bg-background-100 p-4 shadow-card"
      >
        <EntityInspector
          worldId={worldId}
          node={selection.node}
          entity={selection.entity}
          onConflict={onEntityConflict}
          reseedSignal={reseedSignal}
        />
      </aside>
    );
  }
  return (
    <aside
      aria-label={`Promotion inspector: ${selection.node.name}`}
      className="rounded-card border border-gray-alpha-400 bg-background-100 p-4 shadow-card"
    >
      <PromotionInspector
        worldId={worldId}
        node={selection.node}
        candidate={selection.candidate}
        confirmedEntities={confirmedEntities}
        onConflict={onPromoteConflict}
        reseedSignal={reseedSignal}
      />
    </aside>
  );
}

function EntityConflictHost({
  state,
  selection,
  onUseCurrent,
  onReapply,
  onDismiss,
}: {
  state: EntityConflictState | null;
  selection: Selection;
  onUseCurrent: () => void;
  onReapply: () => void;
  onDismiss: () => void;
}) {
  if (!state || !selection || selection.kind !== 'entity') return null;
  return (
    <WorldKbEntityConflictModal
      open
      draft={state.modalDraft}
      currentVersion={state.currentVersion}
      onUseCurrent={onUseCurrent}
      onReapply={onReapply}
      onDismiss={onDismiss}
    />
  );
}

function PromoteConflictHost({
  state,
  selection,
  onUseCurrent,
  onReapply,
  onDismiss,
}: {
  state: PromoteConflictState | null;
  selection: Selection;
  onUseCurrent: () => void;
  onReapply: () => void;
  onDismiss: () => void;
}) {
  if (!state || !selection || selection.kind !== 'candidate') return null;
  return (
    <WorldKbPromoteConflictModal
      open
      draft={state.draft}
      currentVersion={state.currentVersion}
      onUseCurrent={onUseCurrent}
      onReapply={onReapply}
      onDismiss={onDismiss}
    />
  );
}
