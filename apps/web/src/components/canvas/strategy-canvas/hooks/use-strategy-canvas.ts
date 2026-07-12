/**
 * Orchestrator-level state hook for the Strategy canvas.
 *
 * Keeps graph/node state, the shared edit form, per-section save triggers,
 * revision tracking, and the conflict modal state in one place so the
 * orchestrator component stays thin (R-V171P0-QC1-006).
 */
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import {
  reconnectEdge,
  useEdgesState,
  useNodesState,
  type Connection,
  type Edge,
  type Node,
} from '@xyflow/react';

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
import type { StrategyNodeData } from '@/lib/canvas/strategy-graph';

import {
  createDraftTransitionEdge,
  isSectionDirty,
  originalFormOf,
  selectedStateOf,
  templateRefOf,
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

  const [nodes, setNodes, onNodesChange] = useNodesState<Node>([]);
  const [edges, setEdges, onEdgesChange] = useEdgesState<Edge>([]);

  const [form, setForm] = useState<EditForm>({ label: '', description: '', nextTarget: '', promptBody: '' });
  const [saveStatuses, setSaveStatuses] = useState<Partial<Record<Section, SaveStatus>>>({});
  const [activeSection, setActiveSection] = useState<Section>('state');
  const [conflict, setConflict] = useState<ConflictInfo | null>(null);
  const [saveTriggers, setSaveTriggers] = useState<Record<Section, number>>({
    state: 0,
    transition: 0,
    prompt: 0,
  });

  const workingRevisionRef = useRef(graphQuery.data?.revision ?? 0);

  useEffect(() => {
    if (graphQuery.data) {
      setNodes(graphQuery.data.graph.nodes as Node[]);
      // Preserve local draft edges while replacing server edges so a refetch
      // that completes after onConnect does not erase the in-progress draft
      // before the author commits or cancels it (Greptile Issue 4).
      setEdges((currentEdges) => {
        const serverEdges = graphQuery.data.graph.edges as Edge[];
        const serverEdgeIds = new Set(serverEdges.map((e) => e.id));
        const keptDrafts = currentEdges.filter(
          (e) => (e.data as { isDraft?: boolean } | undefined)?.isDraft && !serverEdgeIds.has(e.id),
        );
        return [...serverEdges, ...keptDrafts];
      });
    }
  }, [graphQuery.data, setNodes, setEdges]);

  useEffect(() => {
    if (!activeSession) {
      setNodes((nds) => nds.map((n) => ({ ...n, data: { ...n.data, status: undefined } })));
      return;
    }
    const currentTask = activeSession.current_task_id;
    const sessionStatus = activeSession.status;
    setNodes((nds) =>
      nds.map((n) => {
        const data = n.data as StrategyNodeData;
        const isCurrent =
          currentTask !== undefined &&
          (n.id === currentTask || data.stateId === currentTask || n.id.startsWith(`${currentTask}::`));
        return { ...n, data: { ...data, status: isCurrent ? sessionStatus ?? '__current__' : undefined } };
      }),
    );
  }, [activeSession, setNodes]);

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

  const selected = useMemo(() => nodes.find((n) => n.selected) ?? null, [nodes]);
  const selectedState = selectedStateOf(selected, graphQuery.data?.parsed.manifest.states);
  const baseRevision = graphQuery.data?.revision ?? 0;
  const promptTemplateRef = useMemo(() => templateRefOf(selectedState), [selectedState]);
  const original = useMemo(() => originalFormOf(selectedState), [selectedState]);
  const revisionStatus: 'clean' | 'dirty' | 'conflict' = conflict
    ? 'conflict'
    : isSectionDirty('state', form, original) ||
        isSectionDirty('transition', form, original) ||
        isSectionDirty('prompt', form, original)
      ? 'dirty'
      : 'clean';

  const summaryText = useMemo(() => {
    const count = nodes.length;
    const edgeCount = edges.length;
    const sel = selected ? ` Selected: ${selected.id}.` : '';
    const live = activeSession
      ? ` Current node: ${activeSession.current_task_id ?? 'none'}. Session status: ${activeSession.status}.`
      : ' No active session.';
    return `Strategy graph: ${count} states, ${edgeCount} transitions.${live}${sel}`;
  }, [nodes.length, edges.length, selected, activeSession]);

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
   * The draft is appended to edge state and selected so the edge inspector can
   * take focus. No daemon call happens here — the author commits (or cancels)
   * in the inspector (FB-SE-002). Re-attempting a connect from the same source
   * replaces any existing draft rather than stacking duplicates. Node selection
   * is cleared so the state inspector yields to the transition being drawn.
   */
  const onConnect = useCallback(
    (connection: Connection) => {
      const draft = createDraftTransitionEdge(connection, t('strategy.draftTransitionLabel'));
      if (!draft) return;
      setEdges((eds) => [
        ...eds.map((e) => ({ ...e, selected: false })).filter((e) => !(e.data as { isDraft?: boolean })?.isDraft),
        draft,
      ]);
      setNodes((nds) => nds.map((n) => ({ ...n, selected: false })));
    },
    [setEdges, setNodes, t],
  );

  /**
   * FB-SE-002 — the draft transition selected on the canvas. A draft edge is
   * any edge carrying `data.isDraft = true` (created by {@link onConnect}).
   * There is at most one draft at a time; {@link onConnect} replaces any
   * existing draft instead of stacking duplicates.
   */
  const selectedDraftEdge = useMemo(
    () => edges.find((e) => (e.data as { isDraft?: boolean } | undefined)?.isDraft) ?? null,
    [edges],
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
   * **`op: "create"`** (FB-SE-002). On success the draft is removed from local
   * edge state and the preset query is invalidated so the canonical edge —
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
      setEdges((eds) => eds.filter((e) => !(e.data as { isDraft?: boolean } | undefined)?.isDraft));
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
    setEdges((eds) => eds.filter((e) => !(e.data as { isDraft?: boolean } | undefined)?.isDraft));
  }, [setEdges]);

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
   * so the author ends with one edge for that logical rewiring. The edge is
   * updated optimistically; on failure (including 409 conflict) it reverts to
   * the previous target so no partial daemon state is visible.
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
      // Revert the optimistic edge update so the canvas shows the prior target.
      const revertedTarget = args.oldEdge.target;
      setEdges((eds) =>
        eds.map((e) => (e.id === args.oldEdge.id ? { ...e, target: revertedTarget } : e)),
      );
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
   * prior target (no daemon call). Valid reconnects update the edge
   * optimistically (id kept stable via `shouldReplaceId: false` so the revert
   * path in the mutation can find it) and fire the patch mutation.
   */
  const onReconnect = useCallback(
    (oldEdge: Edge, newConnection: Connection) => {
      if (!newConnection.target || newConnection.target === oldEdge.source) return;
      if (newConnection.target === oldEdge.target) return;
      setEdges((eds) => reconnectEdge(oldEdge, newConnection, eds, { shouldReplaceId: false }));
      reconnectTransitionMutation.mutate({ oldEdge, newConnection });
    },
    [setEdges, reconnectTransitionMutation],
  );

  return {
    graphQuery,
    activeSession,
    schedules,
    creatorId,
    nodes,
    edges,
    onNodesChange,
    onEdgesChange,
    onConnect,
    selected,
    selectedState,
    baseRevision,
    promptTemplateRef,
    revisionStatus,
    summaryText,
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
    selectedDraftEdge,
    draftSourceState,
    commitDraft: commitDraftMutation.mutate,
    isCommittingDraft: commitDraftMutation.isPending,
    cancelDraft,
    commitKeyboardCreate: commitKeyboardCreateMutation.mutate,
    isCommittingKeyboardCreate: commitKeyboardCreateMutation.isPending,
    onReconnect,
    isReconnecting: reconnectTransitionMutation.isPending,
  };
}
