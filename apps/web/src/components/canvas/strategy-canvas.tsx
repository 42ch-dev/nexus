/**
 * Strategy canvas — public orchestrator facade.
 *
 * B1: per-inspector saves (R-V171P0-QC1-004).
 * B2: split into focused sibling modules ≤200 lines (R-V171P0-QC1-006).
 * V1.114 P0 T2: migrated to the shared `CanvasSurfaceAdapter` abstraction via
 * `useCanvasSurface()` so graph projection, summary, inspector routing, and
 * alt-view are adapter-driven.
 * V1.115 P0 T2 (W001): the adapter's `projectGraph` now owns the real
 * projection (`buildStrategyGraph(parsed)`). `useStrategyCanvas` is a thin
 * query + state hook — it delegates projection to the adapter and owns only
 * form/save/conflict/draft state. Draft edges are merged into the projection
 * via `ctxRef.current.localEdges`; `danglingTargets` flows back via
 * `ctxRef.current.danglingTargets`.
 */
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import type { Connection, Edge, Node } from '@xyflow/react';

import { CanvasShell } from '@/components/canvas/canvas-shell';
import { ErrorState, LoadingState } from '@/components/ui/states';
import { useRegisterCommand } from '@/lib/canvas/command-registry';
import type { StrategyEdgeData, StrategyNodeData } from '@/lib/canvas/strategy-graph';
import { useCanvasSurface, type CanvasSurfaceQueryResult } from '@/components/canvas/use-canvas-surface';

import { useStrategyCanvas } from '@/components/canvas/strategy-canvas/hooks/use-strategy-canvas';
import { CanvasFooter, CanvasHeader } from './strategy-canvas/canvas-layout';
import {
  createStrategyCanvasAdapter,
  type StrategyCanvasAdapterContext,
  type StrategySurfaceGraph,
} from './strategy-canvas/strategy-canvas-adapter';
import { InspectorPanel, StrategyConflictModal } from './strategy-canvas/inspector-panel';
import { EdgeCreateDialog } from './strategy-canvas/edge-create-dialog';
import { DraftEdgeInspector } from './strategy-canvas/inspectors/edge-inspector';
import {
  ValidationPanel,
  isSectionDirty,
  originalFormOf,
  templateRefOf,
  type SaveStatus,
  type Section,
} from './strategy-canvas/state-machine';
import type { IdeaArtifact } from '@/components/canvas/idea-input';

export interface StrategyCanvasProps {
  presetId: string;
}

export function StrategyCanvas({ presetId }: StrategyCanvasProps) {
  const { t } = useTranslation('canvas');
  const strategyState = useStrategyCanvas(presetId);

  const [artifacts, setArtifacts] = useState<IdeaArtifact[]>([]);
  const [isEditing, setIsEditing] = useState(false);
  const [createDialogOpen, setCreateDialogOpen] = useState(false);

  const ctxRef = useRef<StrategyCanvasAdapterContext>({
    presetId,
    form: strategyState.form,
    saveTriggers: strategyState.saveTriggers,
    saveStatuses: strategyState.saveStatuses,
    workingRevisionRef: strategyState.workingRevisionRef,
    handleConflict: strategyState.handleConflict,
    onChange: () => {},
    onSaveStatus: () => {},
    setActiveSection: strategyState.setActiveSection,
    selectedState: undefined,
    promptTemplateRef: undefined,
    selectedNode: null,
    parsed: undefined,
    onUseCurrent: () => {},
    onReapply: () => {},
    onDismiss: () => {},
    danglingTargets: [],
    localEdges: [],
  });

  const adapter = useMemo(() => createStrategyCanvasAdapter(ctxRef), []);

  const surfaceQuery = useMemo<CanvasSurfaceQueryResult<StrategySurfaceGraph>>(() => {
    const data = strategyState.graphQuery.data;
    if (!data) {
      return {
        data: undefined,
        isLoading: strategyState.graphQuery.isLoading,
        isError: strategyState.graphQuery.isError,
        error: strategyState.graphQuery.error,
        refetch: strategyState.graphQuery.refetch,
      };
    }
    // V1.115 P0 T2: the adapter projects from `parsed` (not a pre-projected
    // graph). The hook's draft edges are merged via ctxRef.localEdges inside
    // the adapter's projectGraph — no graph field in the surface payload.
    return {
      data: {
        revision: data.revision,
        parsed: data.parsed,
        activeSession: strategyState.activeSession,
      },
      isLoading: strategyState.graphQuery.isLoading,
      isError: strategyState.graphQuery.isError,
      error: strategyState.graphQuery.error,
      refetch: strategyState.graphQuery.refetch,
    };
  }, [
    strategyState.graphQuery,
    strategyState.activeSession,
  ]);

  const surface = useCanvasSurface(adapter, surfaceQuery);

  const selectedState = useMemo(() => {
    const node = surface.selectedNode;
    if (!node) return undefined;
    const stateId = (node.data as StrategyNodeData).stateId;
    return strategyState.graphQuery.data?.parsed.manifest.states.find((s) => s.id === stateId);
  }, [surface.selectedNode, strategyState.graphQuery.data]);

  const promptTemplateRef = useMemo(() => {
    return selectedState ? templateRefOf(selectedState) : undefined;
  }, [selectedState]);

  // V1.115 T2: revisionStatus was previously owned by useStrategyCanvas.
  // It moved here because it depends on selectedState (now resolved from
  // surface.selectedNode by the orchestrator, not from hook-owned node state).
  const original = useMemo(() => originalFormOf(selectedState), [selectedState]);
  const revisionStatus: 'clean' | 'dirty' | 'conflict' = strategyState.conflict
    ? 'conflict'
    : isSectionDirty('state', strategyState.form, original) ||
        isSectionDirty('transition', strategyState.form, original) ||
        isSectionDirty('prompt', strategyState.form, original)
      ? 'dirty'
      : 'clean';

  function updateField<K extends keyof typeof strategyState.form>(
    field: K,
    value: (typeof strategyState.form)[K],
  ) {
    strategyState.setForm((prev) => ({ ...prev, [field]: value }));
  }

  function onSaveStatus(section: Section, status: SaveStatus | undefined) {
    strategyState.setSaveStatuses((prev) => ({ ...prev, [section]: status }));
  }

  function handleUseCurrent() {
    strategyState.setConflict(null);
    setIsEditing(false);
    void strategyState.graphQuery.refetch();
  }

  // Update the mutable adapter context every render. The adapter object is
  // stable, so useCanvasSurface's projection memo survives state changes.
  // V1.115 T2: localEdges feeds draft transition edges into the adapter's
  // projectGraph so they appear on the canvas without a second projection
  // path. danglingTargets is written BY the adapter during projection and
  // read below for the ValidationPanel.
  ctxRef.current = {
    presetId,
    form: strategyState.form,
    saveTriggers: strategyState.saveTriggers,
    saveStatuses: strategyState.saveStatuses,
    workingRevisionRef: strategyState.workingRevisionRef,
    handleConflict: strategyState.handleConflict,
    onChange: updateField,
    onSaveStatus,
    setActiveSection: strategyState.setActiveSection,
    selectedState,
    promptTemplateRef,
    selectedNode: surface.selectedNode as unknown as Node<StrategyNodeData> | null,
    parsed: strategyState.graphQuery.data?.parsed,
    onUseCurrent: handleUseCurrent,
    onReapply: strategyState.handleReapply,
    onDismiss: () => strategyState.setConflict(null),
    danglingTargets: ctxRef.current.danglingTargets,
    localEdges: strategyState.draftEdges as Edge<StrategyEdgeData>[],
  };

  const showAltRef = useRef(surface.showAlt);
  showAltRef.current = surface.showAlt;

  // V1.111 P0 T4 — register Strategy-surface palette commands. The handlers
  // read the current showAlt value through a ref so the command captured on
  // mount never closes over a stale boolean.
  useRegisterCommand({
    id: 'strategy.toggle-view',
    labelKey: 'strategy.toggle-view.label',
    groupKey: 'group.strategy',
    keywordKeys: [
      'strategy.toggle-view.keywords.graph',
      'strategy.toggle-view.keywords.list',
      'strategy.toggle-view.keywords.alt-view',
      'strategy.toggle-view.keywords.switch',
    ],
    handler: () => surface.setShowAlt(!showAltRef.current),
  });
  useRegisterCommand({
    id: 'strategy.create-transition',
    labelKey: 'strategy.create-transition.label',
    groupKey: 'group.strategy',
    keywordKeys: [
      'strategy.create-transition.keywords.add-edge',
      'strategy.create-transition.keywords.new-transition',
      'strategy.create-transition.keywords.state-machine',
      'strategy.create-transition.keywords.link-states',
    ],
    handler: () => setCreateDialogOpen(true),
  });

  useEffect(() => {
    if (!isEditing || !selectedState) {
      strategyState.setForm({ label: '', description: '', nextTarget: '', promptBody: '' });
      strategyState.setSaveStatuses({});
      return;
    }
    strategyState.setForm(originalFormOf(selectedState));
    strategyState.setSaveStatuses({});
  }, [isEditing, selectedState?.id, strategyState.setForm, strategyState.setSaveStatuses]);

  const onConnect = useCallback(
    (connection: Connection) => {
      strategyState.onConnect(connection);
    },
    [strategyState.onConnect],
  );

  if (strategyState.graphQuery.isLoading) return <LoadingState label={t('strategy.loading')} />;
  if (strategyState.graphQuery.isError)
    return <ErrorState description={t('strategy.loadError')} onRetry={() => strategyState.graphQuery.refetch()} />;

  const parsed = strategyState.graphQuery.data?.parsed;
  const problems = parsed?.problems ?? [];
  // V1.115 T2: danglingTargets is a projection byproduct written by the
  // adapter's projectGraph into ctxRef.current.danglingTargets.
  const dangling = ctxRef.current.danglingTargets ?? [];

  return (
    <div className="flex flex-col gap-4">
      <CanvasHeader
        revision={strategyState.baseRevision}
        status={revisionStatus}
        activeSession={strategyState.activeSession}
        showAlt={surface.showAlt}
        setShowAlt={surface.setShowAlt}
        onOpenCreateTransition={() => setCreateDialogOpen(true)}
      />

      {surface.showAlt && parsed ? (
        surface.altView
      ) : (
        <CanvasShell
          nodes={surface.nodes}
          edges={surface.edges}
          nodeTypes={surface.nodeTypes}
          onNodesChange={surface.onNodesChange}
          onConnect={onConnect}
          onReconnect={strategyState.onReconnect}
          summaryText={surface.summaryText}
          ariaLabel={t('strategy.graphAriaLabel')}
          relayout={surface.relayout}
        >
          <InspectorPanel
            selected={surface.selectedNode}
            selectedState={selectedState}
            isEditing={isEditing}
            setIsEditing={setIsEditing}
            onFocusSection={strategyState.setActiveSection}
          >
            {selectedState ? surface.inspector : null}
          </InspectorPanel>
          {strategyState.selectedDraftEdge ? (
            <aside
              className="absolute right-3 top-3 w-[280px] rounded-card border border-gray-alpha-400 bg-background-100 p-3 shadow-popover"
              aria-label={t('strategy.draftTransitionEditor')}
            >
              <DraftEdgeInspector
                sourceStateId={strategyState.selectedDraftEdge.source}
                targetStateId={strategyState.selectedDraftEdge.target}
                isCommitting={strategyState.isCommittingDraft}
                onCommit={strategyState.commitDraft}
                onCancel={strategyState.cancelDraft}
              />
            </aside>
          ) : null}
          <ValidationPanel problems={problems} dangling={dangling} />
        </CanvasShell>
      )}

      <StrategyConflictModal
        conflict={strategyState.conflict}
        form={strategyState.form}
        canonicalState={selectedState ?? strategyState.draftSourceState}
        promptTemplateRef={promptTemplateRef}
        onUseCurrent={handleUseCurrent}
        onReapply={strategyState.handleReapply}
        onDismiss={() => strategyState.setConflict(null)}
      />

      <EdgeCreateDialog
        open={createDialogOpen}
        onOpenChange={setCreateDialogOpen}
        states={parsed?.manifest.states ?? []}
        isCommitting={strategyState.isCommittingKeyboardCreate}
        onCommit={(args) => {
          strategyState.commitKeyboardCreate(args);
          setCreateDialogOpen(false);
        }}
      />

      <CanvasFooter
        presetId={presetId}
        creatorId={strategyState.creatorId}
        scheduleId={strategyState.activeScheduleId}
        artifacts={artifacts}
        setArtifacts={setArtifacts}
      />
    </div>
  );
}
