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

import type { WorkflowDocument } from '@42ch/nexus-contracts';

import { CanvasShell } from '@/components/canvas/canvas-shell';
import { ErrorState, LoadingState, UnavailableState } from '@/components/ui/states';
import { useRegisterCommand } from '@/lib/canvas/command-registry';
import { useSimulationTrajectory } from '@/lib/canvas/use-simulation-trajectory';
import { isOrchestrationEngineUnavailable } from '@/lib/nexus/errors';
import type { StrategyEdgeData, StrategyNodeData } from '@/lib/canvas/strategy-graph';
import { useCanvasSurface, type CanvasSurfaceQueryResult } from '@/components/canvas/use-canvas-surface';

import { useStrategyCanvas } from '@/components/canvas/strategy-canvas/hooks/use-strategy-canvas';
import { useStrategyCompose } from '@/components/canvas/strategy-canvas/hooks/use-strategy-compose';
import { useApproveStrategy } from '@/components/canvas/strategy-canvas/hooks/use-approve-strategy';
import { useApplyStrategy } from '@/components/canvas/strategy-canvas/hooks/use-apply-strategy';
import { useStrategyApplied } from '@/components/canvas/strategy-canvas/hooks/use-strategy-applied';
import { useStrategyCanvasComposeFlag } from '@/lib/canvas/strategy-compose-flag';
import { i18n } from '@/lib/i18n/config';
import { CanvasFooter, CanvasHeader } from './strategy-canvas/canvas-layout';
import {
  createStrategyCanvasAdapter,
  type StrategyCanvasAdapterContext,
  type StrategySurfaceGraph,
} from './strategy-canvas/strategy-canvas-adapter';
import { ApproveDisclosure } from './strategy-canvas/approve-disclosure';
import { ComposeConflictDialog } from './strategy-canvas/compose-conflict-dialog';
import { PatternPickerDialog } from './strategy-canvas/pattern-picker-dialog';
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
  /**
   * The open candidate IR. Approve disclosure and the canvas overlay exist
   * only for a candidate: the static walk answers questions about that
   * document, not about the stored preset.
   */
  candidate?: WorkflowDocument;
  /** Approval write path for a fresh trajectory. */
  onApproveCandidate?: () => void;
}

export function StrategyCanvas({ presetId, candidate, onApproveCandidate }: StrategyCanvasProps) {
  const { t } = useTranslation('canvas');
  // v239-p3-canvas-compose-ui: per-surface flag (compass D14/D18). Off → the
  // canvas renders exactly as today (YAML projection, disabled Approve).
  const composeEnabled = useStrategyCanvasComposeFlag();
  // With the flag on the candidate is the one structural authority, so a draft
  // authored while it was off cannot commit through the YAML path.
  const strategyState = useStrategyCanvas(presetId, composeEnabled);
  const compose = useStrategyCompose(presetId, composeEnabled);
  const approval = useApproveStrategy(presetId);
  const application = useApplyStrategy(presetId);
  // T3: which apply is live + newest approval, for the live block and badge.
  const appliedStatus = useStrategyApplied(presetId, composeEnabled);
  // The compose path simulates the working candidate it holds in hand; the
  // flag-off path keeps the shipped preset/candidate behaviour.
  const simulationCandidate = composeEnabled
    ? (compose.workingDocument ?? undefined)
    : candidate;
  const simulation = useSimulationTrajectory(simulationCandidate);

  const [artifacts, setArtifacts] = useState<IdeaArtifact[]>([]);
  const [isEditing, setIsEditing] = useState(false);
  const [createDialogOpen, setCreateDialogOpen] = useState(false);
  const [patternPickerOpen, setPatternPickerOpen] = useState(false);
  const [approveOpen, setApproveOpen] = useState(false);

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
        trajectory: simulation.trajectory,
        // Compose path only: the working candidate + slot chrome. Absent with
        // the flag off, so the adapter projects from YAML exactly as today.
        ...(composeEnabled
          ? {
              document: compose.workingDocument ?? undefined,
              patterns: compose.patternsQuery.data?.items,
              expandedSlots: compose.expandedSlots,
              containment: new Map(Object.entries(compose.containment)),
            }
          : {}),
      },
      isLoading: strategyState.graphQuery.isLoading,
      isError: strategyState.graphQuery.isError,
      error: strategyState.graphQuery.error,
      refetch: strategyState.graphQuery.refetch,
    };
  }, [
    strategyState.graphQuery.data,
    strategyState.graphQuery.isLoading,
    strategyState.graphQuery.isError,
    strategyState.graphQuery.error,
    strategyState.graphQuery.refetch,
    strategyState.activeSession,
    simulation.trajectory,
    composeEnabled,
    compose.workingDocument,
    compose.patternsQuery.data,
    compose.expandedSlots,
    compose.containment,
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

  // One Simulate action for the open strategy: a supplied candidate is walked
  // as-is; otherwise the installed preset's IR is re-read and walked only when
  // it no longer matches the trajectory in hand. On the compose path the
  // working candidate (committed document + authored connecting edges) is the
  // walked document, so a wired slot is named by the trajectory (T3b).
  function handleSimulate() {
    setApproveOpen(true);
    if (simulation.status === 'running') return;
    if (!simulationCandidate) {
      void simulation.runStrategy(presetId);
      return;
    }
    // A candidate that still matches its trajectory is reopened, never re-walked.
    if (simulation.status !== 'ready') void simulation.run(simulationCandidate);
  }

  /**
   * T6 — the real approve route, compose path only. A current successful
   * trajectory is required (the disclosure disables Approve otherwise) and an
   * unbound required slot gates the button with its named reason. The daemon's
   * refusal renders verbatim with its recovery copy; nothing auto-retries.
   */
  function handleApprove() {
    void approval
      .approve({
        baseRevision: compose.revision,
        trajectory: simulation.trajectory,
      })
      .catch(() => {
        // The refusal renders from `approval.approval`; no toast duplication.
      });
  }

  /**
   * v241 P3.5 T2 — apply the frozen snapshot the approve response carries. The
   * request names the approval by content hash only; the applied identity comes
   * back from the response, so the surface never reloads to show it.
   */
  function handleApply() {
    const approved = approval.approval;
    if (!approved || approved.kind !== 'approved') return;
    void application.apply(approved.snapshot).catch(() => {
      // The refusal renders from `application.application`; no toast duplication.
    });
  }

  /**
   * Approve stays disabled while a required slot is unbound (named per pattern).
   * Only a listing with no data at all gates (a pending or failed first load),
   * because that is when `unboundSlots` cannot see the slots. A failed
   * background refetch keeps the last read's items, so it stays ungated.
   */
  const approveGateReason = !composeEnabled
    ? null
    : compose.patternsQuery.data === undefined
      ? compose.patternsQuery.error
        ? i18n.t('strategy.approve.listingUnreadable', {
            ns: 'canvas',
            message: compose.patternsQuery.error.message,
          })
        : i18n.t('strategy.approve.listingPending', { ns: 'canvas' })
      : compose.unboundSlots.length > 0
        ? compose.unboundSlots
            .map((unbound) =>
              i18n.t('strategy.approve.slotUnbound', {
                ns: 'canvas',
                slot: unbound.slots.join(', '),
                pattern: unbound.patternId,
              }),
            )
            .join(' ')
        : null;

  /** Nest attaches at the selected node, else the candidate entry. */
  const nestAttachNodeId =
    (surface.selectedNode?.data as StrategyNodeData | undefined)?.stateId ??
    compose.workingDocument?.entry;

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
    patterns: compose.patternsQuery.data?.items,
    ...(composeEnabled
      ? {
          compose: {
            isComposing: compose.isComposing,
            onToggleSlotExpanded: compose.toggleSlotExpanded,
            onBindSlot: compose.bindSlot,
            onReplaceSubtree: compose.replaceSubtree,
            onUnnest: compose.unnest,
          },
        }
      : {}),
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
    // Compose path: the candidate is the one structural authority, so the YAML
    // transition author stays out of the palette (D: one writer per path).
    available: () => !composeEnabled,
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
  // Compose path: the working candidate seeds from the one IR read the wire
  // serves; until it lands there is nothing to project.
  if (composeEnabled && compose.seedQuery.isError) {
    return (
      <ErrorState
        title={t('strategy.loadErrorTitle')}
        description={t('strategy.loadError')}
        onRetry={() => void compose.seedQuery.refetch()}
      />
    );
  }
  if (composeEnabled && !compose.workingDocument) {
    return <LoadingState label={t('strategy.composeLoading')} />;
  }
  if (strategyState.graphQuery.isError) {
    if (isOrchestrationEngineUnavailable(strategyState.graphQuery.error)) {
      return (
        <UnavailableState
          title={t('strategy.engineUnavailableTitle')}
          description={t('strategy.engineUnavailableDescription')}
          onRetry={() => strategyState.graphQuery.refetch()}
        />
      );
    }
    return (
      <ErrorState
        title={t('strategy.loadErrorTitle')}
        description={t('strategy.loadError')}
        onRetry={() => strategyState.graphQuery.refetch()}
      />
    );
  }

  const parsed = strategyState.graphQuery.data?.parsed;
  const problems = parsed?.problems ?? [];
  // V1.115 T2: danglingTargets is a projection byproduct written by the
  // adapter's projectGraph into ctxRef.current.danglingTargets.
  const dangling = ctxRef.current.danglingTargets ?? [];

  return (
    <div className="flex flex-col gap-4">
      <CanvasHeader
        revision={composeEnabled ? compose.revision : strategyState.baseRevision}
        status={revisionStatus}
        activeSession={strategyState.activeSession}
        showAlt={surface.showAlt}
        setShowAlt={surface.setShowAlt}
        onOpenCreateTransition={composeEnabled ? undefined : () => setCreateDialogOpen(true)}
        onSimulate={handleSimulate}
        staging={composeEnabled}
        onAddPattern={composeEnabled ? () => setPatternPickerOpen(true) : undefined}
      />

      {surface.showAlt && parsed ? (
        surface.altView
      ) : (
        <CanvasShell
          nodes={surface.nodes}
          edges={surface.edges}
          nodeTypes={surface.nodeTypes}
          onNodesChange={surface.onNodesChange}
          // Compose path: a displayed edge is never authored through the YAML
          // authority — connect/reconnect write the preset, not the candidate.
          onConnect={composeEnabled ? undefined : onConnect}
          onReconnect={composeEnabled ? undefined : strategyState.onReconnect}
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
            {selectedState || surface.selectedNode?.type === 'strategy-pattern-slot'
              ? surface.inspector
              : null}
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
                structuralWritesDisabled={composeEnabled}
              />
            </aside>
          ) : null}
          <ValidationPanel problems={problems} dangling={dangling} />
        </CanvasShell>
      )}

      <ApproveDisclosure
        open={approveOpen}
        onOpenChange={setApproveOpen}
        status={simulation.status}
        isStale={simulation.isStale}
        trajectory={simulation.trajectory}
        error={simulation.error}
        onRun={handleSimulate}
        onApprove={composeEnabled ? handleApprove : onApproveCandidate}
        approval={composeEnabled ? approval.approval : null}
        gateReason={composeEnabled ? approveGateReason : null}
        onApply={composeEnabled ? handleApply : undefined}
        application={composeEnabled ? application.application : null}
        appliedStatus={composeEnabled ? (appliedStatus.data ?? null) : null}
      />

      <ComposeConflictDialog
        conflict={compose.conflict}
        refusal={compose.refusal}
        onReload={() => {
          void compose.reload();
        }}
        onDismiss={() => {
          compose.dismissConflict();
          compose.dismissRefusal();
        }}
      />

      <PatternPickerDialog
        open={patternPickerOpen}
        onOpenChange={setPatternPickerOpen}
        patterns={compose.patternsQuery.data?.items ?? []}
        error={compose.patternsQuery.isError ? compose.patternsQuery.error.message : null}
        attachNodeId={nestAttachNodeId ?? compose.workingDocument?.entry ?? presetId}
        isComposing={compose.isComposing}
        onNest={compose.nestPattern}
      />

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
