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
import type { Node } from '@xyflow/react';

import type { StrategyApprovals, WorkflowDocument } from '@42ch/nexus-contracts';

import { CanvasShell } from '@/components/canvas/canvas-shell';
import { ErrorState, LoadingState, UnavailableState } from '@/components/ui/states';
import { useRegisterCommand } from '@/lib/canvas/command-registry';
import { useSimulationTrajectory } from '@/lib/canvas/use-simulation-trajectory';
import { isOrchestrationEngineUnavailable } from '@/lib/nexus/errors';
import type { StrategyNodeData } from '@/lib/canvas/strategy-graph';
import { useCanvasSurface, type CanvasSurfaceQueryResult } from '@/components/canvas/use-canvas-surface';

import { useStrategyCanvas } from '@/components/canvas/strategy-canvas/hooks/use-strategy-canvas';
import { useStrategyCompose } from '@/components/canvas/strategy-canvas/hooks/use-strategy-compose';
import { useApproveStrategy } from '@/components/canvas/strategy-canvas/hooks/use-approve-strategy';
import { useApplyStrategy } from '@/components/canvas/strategy-canvas/hooks/use-apply-strategy';
import { useStrategyApplied } from '@/components/canvas/strategy-canvas/hooks/use-strategy-applied';
import { useStrategyApprovals } from '@/components/canvas/strategy-canvas/hooks/use-strategy-approvals';
import { useStrategyCanvasComposeFlag } from '@/lib/canvas/strategy-compose-flag';
import { i18n } from '@/lib/i18n/config';
import { CanvasFooter, CanvasHeader } from './strategy-canvas/canvas-layout';
import {
  createStrategyCanvasAdapter,
  type StrategyCanvasAdapterContext,
  type StrategySurfaceGraph,
} from './strategy-canvas/strategy-canvas-adapter';
import { ApproveDisclosure } from './strategy-canvas/approve-disclosure';
import { StrategyAppliedState } from './strategy-canvas/applied-state';
import { ComposeConflictDialog } from './strategy-canvas/compose-conflict-dialog';
import { PatternPickerDialog } from './strategy-canvas/pattern-picker-dialog';
import { InspectorPanel, StrategyConflictModal } from './strategy-canvas/inspector-panel';
import {
  ValidationPanel,
  isSectionDirty,
  originalFormOf,
  templateRefOf,
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
  // Canvas renders the applied preset with no composition surface.
  const composeFlag = useStrategyCanvasComposeFlag();
  // D16(a): Compose is the never-applied state's one action, so it must be
  // reachable for a creator whose entrance defaults the flag off — and from the
  // read-only applied view, which otherwise renders a strategy nobody can edit.
  const [composeByUser, setComposeByUser] = useState(false);
  const composeEnabled = composeFlag || composeByUser;
  // D18: one status read carries the identity, the frozen applied document and
  // the refusal that separates "nothing applied" from "unreadable". It is the
  // Canvas's document source — the Canvas parses no YAML (P6 condition 2).
  const applied = useStrategyApplied(presetId, true);
  const appliedDocument = applied.data?.applied_document ?? null;
  const currentApply = applied.data?.current_apply ?? null;
  const refusal = applied.data?.applied_document_refusal ?? null;
  const compose = useStrategyCompose(presetId, composeEnabled);
  // The document being rendered: the candidate while composing, else the
  // applied preset. Both are IR.
  const document = composeEnabled
    ? (compose.workingDocument ?? appliedDocument)
    : appliedDocument;
  // The document being rendered is the applied (running) strategy: it is what
  // apply/approve produced, so this view is read-only and editing goes through
  // Compose (P6 condition 2, D18).
  const appliedView = document === appliedDocument;
  const strategyState = useStrategyCanvas(presetId, {
    document,
    refetch: () => applied.refetch(),
  });
  const approval = useApproveStrategy(presetId);
  const application = useApplyStrategy(presetId);
  // T4: every approval, so Apply can be aimed at a superseded one (rollback).
  const approvals = useStrategyApprovals(presetId, composeEnabled);
  // The compose path simulates the working candidate it holds in hand; the
  // flag-off path keeps the shipped preset/candidate behaviour.
  const simulationCandidate = composeEnabled
    ? (compose.workingDocument ?? undefined)
    : candidate;
  const simulation = useSimulationTrajectory(simulationCandidate);

  const [artifacts, setArtifacts] = useState<IdeaArtifact[]>([]);
  const [isEditing, setIsEditing] = useState(false);
  const [patternPickerOpen, setPatternPickerOpen] = useState(false);
  const [approveOpen, setApproveOpen] = useState(false);
  // Which listed approval Apply is aimed at; null means the newest (T4).
  const [applyTargetId, setApplyTargetId] = useState<string | null>(null);

  const ctxRef = useRef<StrategyCanvasAdapterContext>({
    form: strategyState.form,
    onChange: () => {},
    setActiveSection: strategyState.setActiveSection,
    selectedState: undefined,
    promptTemplateRef: undefined,
    selectedNode: null,
    parsed: undefined,
    onUseCurrent: () => {},
    onReapply: () => {},
    onDismiss: () => {},
    danglingTargets: [],
  });

  const adapter = useMemo(() => createStrategyCanvasAdapter(ctxRef), []);

  // `useCanvasSurface` only calls this from an error state, but its identity is
  // part of the payload: a value that changed per render would re-project the
  // graph every render (the surface syncs the projection into its node state).
  const refetchApplied = useRef<() => void>(() => {});
  refetchApplied.current = () => void applied.refetch();
  const refetchSurface = useCallback(() => refetchApplied.current(), []);

  const surfaceQuery = useMemo<CanvasSurfaceQueryResult<StrategySurfaceGraph>>(() => {
    if (!document) {
      return {
        data: undefined,
        isLoading: false,
        isError: false,
        error: null,
        refetch: refetchSurface,
      };
    }
    // The payload carries the document only: the adapter projects it from IR
    // and merges draft edges via ctxRef.localEdges.
    return {
      data: {
        revision: document.revision,
        activeSession: strategyState.activeSession,
        trajectory: simulation.trajectory,
        document,
        patterns: compose.patternsQuery.data?.items,
        expandedSlots: compose.expandedSlots,
        containment: new Map(Object.entries(compose.containment)),
      },
      isLoading: false,
      isError: false,
      error: null,
      refetch: refetchSurface,
    };
  }, [
    document,
    refetchSurface,
    strategyState.activeSession,
    simulation.trajectory,
    compose.patternsQuery.data,
    compose.expandedSlots,
    compose.containment,
  ]);

  const surface = useCanvasSurface(adapter, surfaceQuery);

  const selectedState = useMemo(() => {
    const node = surface.selectedNode;
    if (!node) return undefined;
    const stateId = (node.data as StrategyNodeData).stateId;
    return strategyState.parsed?.manifest.states.find((s) => s.id === stateId);
  }, [surface.selectedNode, strategyState.parsed]);

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
    // A newer approval supersedes the previous apply block; never render both.
    application.dismissApplication();
    // The fresh approval is the newest, so Apply goes back to the default target.
    setApplyTargetId(null);
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
   * v241 P3.5 T2/T4 — apply the approval the control is aimed at, listed or
   * freshly approved. The request names the approval by content hash only, so a
   * rollback is this same call; the applied identity comes back from the
   * response, so the surface never reloads to show it.
   */
  function handleApply(target: StrategyApprovals['items'][number]) {
    void application.apply(target).catch(() => {
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
    void applied.refetch();
  }

  // Update the mutable adapter context every render. The adapter object is
  // stable, so useCanvasSurface's projection memo survives state changes.
  // V1.115 T2: localEdges feeds draft transition edges into the adapter's
  // projectGraph so they appear on the canvas without a second projection
  // path. danglingTargets is written BY the adapter during projection and
  // read below for the ValidationPanel.
  ctxRef.current = {
    form: strategyState.form,
    onChange: updateField,
    setActiveSection: strategyState.setActiveSection,
    selectedState,
    promptTemplateRef,
    selectedNode: surface.selectedNode as unknown as Node<StrategyNodeData> | null,
    parsed: strategyState.parsed,
    onUseCurrent: handleUseCurrent,
    onReapply: strategyState.handleReapply,
    onDismiss: () => strategyState.setConflict(null),
    danglingTargets: ctxRef.current.danglingTargets,
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

  useEffect(() => {
    if (!isEditing || !selectedState) {
      strategyState.setForm({ label: '', description: '', nextTarget: '', promptBody: '' });
      return;
    }
    strategyState.setForm(originalFormOf(selectedState));
  }, [isEditing, selectedState?.id, strategyState.setForm]);

  // D16(c): an apply is recorded but its document no longer verifies. This is
  // not an empty state — presenting it as "not yet applied" would invite an
  // apply that overwrites a live ledger — so it wins over every other view and
  // carries no import or apply action.
  if (currentApply && !appliedDocument) {
    return (
      <StrategyAppliedState
        state="unreadable"
        reason={refusal?.message ?? refusal?.reason ?? ''}
        onRecheck={() => void applied.refetch()}
      />
    );
  }
  // Compose path: the working candidate seeds from the one IR read the wire
  // serves; until it lands there is nothing to project.
  if (!document && composeEnabled && compose.seedQuery.isError) {
    return (
      <ErrorState
        title={t('strategy.loadErrorTitle')}
        description={t('strategy.loadError')}
        onRetry={() => void compose.seedQuery.refetch()}
      />
    );
  }
  if (!document && composeEnabled) {
    return <LoadingState label={t('strategy.composeLoading')} />;
  }
  if (!document && applied.isLoading) return <LoadingState label={t('strategy.loading')} />;
  if (!document && applied.isError) {
    if (isOrchestrationEngineUnavailable(applied.error)) {
      return (
        <UnavailableState
          title={t('strategy.engineUnavailableTitle')}
          description={t('strategy.engineUnavailableDescription')}
          onRetry={() => void applied.refetch()}
        />
      );
    }
    return (
      <ErrorState
        title={t('strategy.loadErrorTitle')}
        description={t('strategy.loadError')}
        onRetry={() => void applied.refetch()}
      />
    );
  }
  // D16(b): an approved candidate with no live version — apply leads, Compose
  // does not. D16(a): neither — Compose is the only action.
  if (!document) {
    const newest = applied.data?.newest_approval ?? null;
    if (newest) {
      return (
        <StrategyAppliedState
          state="approved-unapplied"
          refusal={application.application?.kind === 'refused' ? application.application.message : null}
          onApplyLatest={() => handleApply(newest)}
        />
      );
    }
    return (
      <StrategyAppliedState state="never-applied" onOpenCompose={() => setComposeByUser(true)} />
    );
  }

  const parsed = strategyState.parsed;
  const problems = parsed?.problems ?? [];
  // V1.115 T2: danglingTargets is a projection byproduct written by the
  // adapter's projectGraph into ctxRef.current.danglingTargets.
  const dangling = ctxRef.current.danglingTargets ?? [];

  return (
    <div className="flex flex-col gap-4">
      <CanvasHeader
        revision={strategyState.revision}
        status={revisionStatus}
        activeSession={strategyState.activeSession}
        showAlt={surface.showAlt}
        setShowAlt={surface.setShowAlt}
        onSimulate={handleSimulate}
        staging={composeEnabled}
        onAddPattern={composeEnabled ? () => setPatternPickerOpen(true) : undefined}
        // D16(a) read from a state that has content: the flag-off applied view
        // is read-only, so this is the one way to author — Compose, seeded from
        // the strategy's candidate (the hint in the header says so).
        onEditApplied={appliedView && !composeEnabled ? () => setComposeByUser(true) : undefined}
      />

      {surface.showAlt && parsed ? (
        surface.altView
      ) : (
        <CanvasShell
          nodes={surface.nodes}
          edges={surface.edges}
          nodeTypes={surface.nodeTypes}
          onNodesChange={surface.onNodesChange}
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
        approvals={composeEnabled ? (approvals.data?.items ?? null) : null}
        applyTargetId={applyTargetId}
        onSelectApplyTarget={setApplyTargetId}
        approvalsError={composeEnabled ? (approvals.error?.message ?? null) : null}
        application={composeEnabled ? application.application : null}
        appliedStatus={composeEnabled ? (applied.data ?? null) : null}
        appliedStatusPending={composeEnabled ? applied.isPending : false}
        appliedStatusError={composeEnabled ? (applied.error?.message ?? null) : null}
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
        canonicalState={selectedState}
        promptTemplateRef={promptTemplateRef}
        onUseCurrent={handleUseCurrent}
        onReapply={strategyState.handleReapply}
        onDismiss={() => strategyState.setConflict(null)}
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
