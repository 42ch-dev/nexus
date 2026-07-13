/**
 * Orchestrator-level state hook for the Strategy canvas.
 *
 * V1.115 P0 T2 (W001): this hook is now a thin query + state hook — graph
 * projection is delegated to the Strategy adapter's `projectGraph` (called by
 * `useCanvasSurface`). The hook owns: the shared edit form, per-section save
 * triggers/statuses, revision tracking, conflict/reapply coordination, draft
 * transition edges (local-only), and the three transition-write mutations
 * (commit draft, keyboard create, reconnect). It no longer manages base
 * node/edge state — the adapter projects from `parsed` and merges drafts.
 */
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import type { Connection, Edge } from '@xyflow/react';

import {
  isStrategyConflictError,
  useActiveSession,
  useDerivedCreatorId,
  usePresetGraph,
  usePresetSchedules,
} from '@/lib/canvas/use-strategy-data';
import { useNexusClient } from '@/lib/client-context';
import { queryKeys } from '@/lib/nexus/query-keys';
import { useToast } from '@/lib/use-toast';

import {
  createDraftTransitionEdge,
  type EditForm,
  type SaveStatus,
  type Section,
} from '../state-machine';

/**
 * Conflict state shared across the canvas write boundary.
 *
 * `retry` is present when the conflict originated from a transition
 * create/reconnect command (not a state-edit save). In that case
 * {@link useStrategyCanvas.handleReapply} replays the original transition
 * command instead of incrementing the section save trigger, which would
 * wrongly replay a state-edit save (QC1 W-001).
 */
export interface ConflictInfo {
  currentRevision: number;
  section: Section;
  retry?: () => void;
}

export function useStrategyCanvas(presetId: string) {
  const { t } = useTranslation('canvas');
  const graphQuery = usePresetGraph(presetId);
  const activeSession = useActiveSession(presetId);
  const schedules = usePresetSchedules(presetId);
  const creatorId = useDerivedCreatorId(presetId);

  const [form, setForm] = useState<EditForm>({ label: '', description: '', nextTarget: '', promptBody: '' });
  const [saveStatuses, setSaveStatuses] = useState<Partial<Record<Section, SaveStatus>>>({});
  const [activeSection, setActiveSection] = useState<Section>('state');
  const [conflict, setConflict] = useState<ConflictInfo | null>(null);
  const [saveTriggers, setSaveTriggers] = useState<Record<Section, number>>({
    state: 0,
    transition: 0,
    prompt: 0,
  });

  // Draft transition edges (local-only — created by onConnect, committed or
  // cancelled in the inspector). The adapter merges these with the projected
  // daemon edges so drafts appear on the canvas without the hook managing base
  // node/edge state.
  const [draftEdges, setDraftEdges] = useState<Edge[]>([]);

  const workingRevisionRef = useRef(graphQuery.data?.revision ?? 0);

  useEffect(() => {
    workingRevisionRef.current = graphQuery.data?.revision ?? 0;
  }, [graphQuery.data?.revision]);

  useEffect(() => {
    if (conflict && graphQuery.data && graphQuery.data.revision !== conflict.currentRevision) {
      setConflict(null);
    }
  }, [graphQuery.data, conflict]);

  useEffect(() => {
    function onKeyDown(e: KeyboardEvent) {
      if ((e.metaKey || e.ctrlKey) && e.key === 's') {
        e.preventDefault();
        setSaveTriggers((prev) => ({ ...prev, [activeSection]: prev[activeSection] + 1 }));
      }
    }
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [activeSection]);

  const baseRevision = graphQuery.data?.revision ?? 0;

  const activeScheduleId = useMemo(() => {
    if (!activeSession) return undefined;
    const list = schedules.data ?? [];
    return [...list].sort((a, b) => b.updated_at.localeCompare(a.updated_at))[0]?.schedule_id;
  }, [activeSession, schedules.data]);

  function handleConflict(currentRevision: number, section: Section, retry?: () => void) {
    setConflict({ currentRevision, section, retry });
    void graphQuery.refetch();
  }

  function handleReapply() {
    if (!conflict) return;
    const { section, retry } = conflict;
    setConflict(null);
    void graphQuery.refetch().then(() => {
      // Transition create/reconnect conflicts replay the original transition
      // command; state-edit conflicts replay the section save trigger
      // (QC1 W-001).
      if (retry) {
        retry();
      } else {
        setSaveTriggers((prev) => ({ ...prev, [section]: prev[section] + 1 }));
      }
    });
  }

  /**
   * Spatial connect gesture → local draft transition edge (FB-SE-000).
   *
   * The draft is appended to `draftEdges` and selected so the edge inspector
   * can take focus. No daemon call happens here — the author commits (or
   * cancels) in the inspector (FB-SE-002). Re-attempting a connect from the
   * same source replaces any existing draft rather than stacking duplicates.
   */
  const onConnect = useCallback(
    (connection: Connection) => {
      const draft = createDraftTransitionEdge(connection, t('strategy.draftTransitionLabel'));
      if (!draft) return;
      setDraftEdges((prev) => [
        ...prev.map((e) => ({ ...e, selected: false })).filter(
          (e) => !(e.data as { isDraft?: boolean })?.isDraft,
        ),
        draft,
      ]);
    },
    [t],
  );

  /**
   * FB-SE-002 — the draft transition selected on the canvas. A draft edge is
   * any edge carrying `data.isDraft = true` (created by {@link onConnect}).
   * There is at most one draft at a time; {@link onConnect} replaces any
   * existing draft instead of stacking duplicates.
   */
  const selectedDraftEdge = useMemo(
    () => draftEdges.find((e) => (e.data as { isDraft?: boolean } | undefined)?.isDraft) ?? null,
    [draftEdges],
  );

  const states = graphQuery.data?.parsed.manifest.states;
  const draftSourceState = useMemo(() => {
    if (!selectedDraftEdge) return undefined;
    return states?.find((s) => s.id === selectedDraftEdge.source);
  }, [selectedDraftEdge, states]);

  const nexusClient = useNexusClient();
  const qc = useQueryClient();
  const { toast } = useToast();

  /**
   * Commit a draft transition via `strategy.patch_transition` with
   * **`op: "create"`** (FB-SE-002). On success the draft is removed from
   * `draftEdges` and the preset query is invalidated so the canonical edge —
   * built by the daemon from the structured op — replaces it on refetch. A 409
   * revision conflict keeps the draft and routes through the same
   * {@link handleConflict} path the existing edit inspector uses, so the
   * conflict modal with Use current / Reapply / Review side-by-side opens.
   *
   * `handleConflict` is a hoisted function declaration, so it is safe to
   * reference here even though it is defined textually later in the hook.
   */
  const commitDraftMutation = useMutation({
    mutationFn: async (args: { condition?: string }) => {
      if (!selectedDraftEdge) throw new Error('No draft transition to commit');
      return nexusClient.strategyPatchTransition(presetId, {
        strategy_id: presetId,
        base_revision: workingRevisionRef.current,
        source_state_id: selectedDraftEdge.source,
        new_target: selectedDraftEdge.target,
        condition: args.condition,
        op: 'create',
        transition_kind: 'next',
      });
    },
    onSuccess: (res) => {
      workingRevisionRef.current = Number(res.new_revision);
      setDraftEdges((prev) => prev.filter((e) => !(e.data as { isDraft?: boolean } | undefined)?.isDraft));
      toast({
        variant: 'success',
        title: 'Transition created',
        description: `${selectedDraftEdge?.source} → ${selectedDraftEdge?.target}`,
      });
      void qc.invalidateQueries({ queryKey: queryKeys.presets.detail(presetId) });
    },
    onError: (error, args) => {
      if (isStrategyConflictError(error)) {
        const currentRevision =
          typeof error.details === 'object' && error.details !== null
            ? (error.details as { current_revision?: number }).current_revision ?? 0
            : 0;
        // Keep the draft so the author can reconcile; refetch canonical.
        // Pass a retry that replays the original transition command so
        // "Reapply my edit" re-issues the create, not a state-edit save (QC1 W-001).
        handleConflict(currentRevision, 'transition', () => commitDraftMutation.mutate(args));
      } else {
        const message = error instanceof Error ? error.message : 'Failed to create transition';
        toast({ variant: 'error', title: message });
      }
    },
  });

  /** Discard the current draft transition without a daemon call (FB-SE-001 #4). */
  const cancelDraft = useCallback(() => {
    setDraftEdges((prev) => prev.filter((e) => !(e.data as { isDraft?: boolean } | undefined)?.isDraft));
  }, []);

  /**
   * FB-SE-004 — keyboard edge-creation commit. Sends the same
   * `strategy.patch_transition` with **`op: "create"`** as the spatial path
   * (FB-SE-002), but with explicit source/target from the dialog instead of a
   * draft edge. On 409, routes through the same {@link handleConflict} path so
   * the conflict modal opens. There is no local draft edge to remove on
   * success — the dialog closes and the preset refetch brings the canonical
   * edge.
   *
   * `handleConflict` is a hoisted function declaration, so it is safe to
   * reference here even though it is defined textually later in the hook.
   */
  const commitKeyboardCreateMutation = useMutation({
    mutationFn: async (args: {
      sourceStateId: string;
      targetStateId: string;
      transitionKind?: 'next' | 'branch' | 'default';
      condition?: string;
    }) => {
      return nexusClient.strategyPatchTransition(presetId, {
        strategy_id: presetId,
        base_revision: workingRevisionRef.current,
        source_state_id: args.sourceStateId,
        new_target: args.targetStateId,
        condition: args.condition,
        transition_kind: args.transitionKind ?? 'next',
        op: 'create',
      });
    },
    onSuccess: (res, args) => {
      workingRevisionRef.current = Number(res.new_revision);
      toast({
        variant: 'success',
        title: 'Transition created',
        description: `${args.sourceStateId} → ${args.targetStateId}`,
      });
      void qc.invalidateQueries({ queryKey: queryKeys.presets.detail(presetId) });
    },
    onError: (error, args) => {
      if (isStrategyConflictError(error)) {
        const currentRevision =
          typeof error.details === 'object' && error.details !== null
            ? (error.details as { current_revision?: number }).current_revision ?? 0
            : 0;
        // Replay the original transition command on reapply (QC1 W-001).
        handleConflict(currentRevision, 'transition', () =>
          commitKeyboardCreateMutation.mutate(args),
        );
      } else {
        const message = error instanceof Error ? error.message : 'Failed to create transition';
        toast({ variant: 'error', title: message });
      }
    },
  });

  /**
   * FB-SE-003 — reconnect an existing transition edge to a new target by drag.
   *
   * Sends a single `strategy.patch_transition` with `op: "update"` (default) +
   * `old_target` (the previous target) + `new_target` (the drag's new target).
   * The daemon replaces the matched transition atomically — no delete+create,
   * so the author ends with one edge for that logical rewiring.
   *
   * V1.115 T2: the optimistic edge update (and its revert-on-failure) are no
   * longer managed in local edge state. The adapter projects from `parsed` on
   * every render, so the canonical edge moves to the new target once the query
   * refetch completes. On failure, the old target persists because no daemon
   * state changed.
   *
   * `handleConflict` is a hoisted function declaration, so it is safe to
   * reference here even though it is defined textually later in the hook.
   */
  const reconnectTransitionMutation = useMutation({
    mutationFn: async (args: { oldEdge: Edge; newConnection: Connection }) => {
      const { oldEdge, newConnection } = args;
      if (!newConnection.target) throw new Error('Reconnect is missing a new target');
      const data = oldEdge.data as { transitionKind?: string; condition?: string } | undefined;
      return nexusClient.strategyPatchTransition(presetId, {
        strategy_id: presetId,
        base_revision: workingRevisionRef.current,
        source_state_id: oldEdge.source,
        old_target: oldEdge.target,
        new_target: newConnection.target,
        condition: data?.condition,
        transition_kind: (data?.transitionKind ?? 'next') as 'next' | 'branch' | 'default',
        op: 'update',
      });
    },
    onSuccess: (res, args) => {
      workingRevisionRef.current = Number(res.new_revision);
      toast({
        variant: 'success',
        title: 'Transition reconnected',
        description: `${args.oldEdge.source} → ${args.newConnection.target}`,
      });
      void qc.invalidateQueries({ queryKey: queryKeys.presets.detail(presetId) });
    },
    onError: (error, args) => {
      if (isStrategyConflictError(error)) {
        const currentRevision =
          typeof error.details === 'object' && error.details !== null
            ? (error.details as { current_revision?: number }).current_revision ?? 0
            : 0;
        // Replay the original reconnect command on reapply (QC1 W-001).
        handleConflict(currentRevision, 'transition', () =>
          reconnectTransitionMutation.mutate(args),
        );
      } else {
        const message = error instanceof Error ? error.message : 'Failed to reconnect transition';
        toast({ variant: 'error', title: message });
      }
    },
  });

  /**
   * React Flow reconnect gesture (dragging an existing edge end to a new
   * target) → single `patch_transition` reconnect payload (FB-SE-003).
   *
   * Self-loops and missing endpoints are ignored so the edge snaps back to its
   * prior target (no daemon call). V1.115 T2: no local optimistic update —
   * the edge moves to the new target once the query refetch completes.
   */
  const onReconnect = useCallback(
    (oldEdge: Edge, newConnection: Connection) => {
      if (!newConnection.target || newConnection.target === oldEdge.source) return;
      if (newConnection.target === oldEdge.target) return;
      reconnectTransitionMutation.mutate({ oldEdge, newConnection });
    },
    [reconnectTransitionMutation],
  );

  return {
    graphQuery,
    activeSession,
    schedules,
    creatorId,
    baseRevision,
    activeScheduleId,
    form,
    setForm,
    saveStatuses,
    setSaveStatuses,
    activeSection,
    setActiveSection,
    conflict,
    setConflict,
    saveTriggers,
    setSaveTriggers,
    workingRevisionRef,
    handleConflict,
    handleReapply,
    draftEdges,
    selectedDraftEdge,
    draftSourceState,
    onConnect,
    commitDraft: commitDraftMutation.mutate,
    isCommittingDraft: commitDraftMutation.isPending,
    cancelDraft,
    commitKeyboardCreate: commitKeyboardCreateMutation.mutate,
    isCommittingKeyboardCreate: commitKeyboardCreateMutation.isPending,
    onReconnect,
    isReconnecting: reconnectTransitionMutation.isPending,
  };
}
