/**
 * Orchestrator-level state hook for the Strategy canvas.
 *
 * Keeps graph/node state, the shared edit form, per-section save triggers,
 * revision tracking, and the conflict modal state in one place so the
 * orchestrator component stays thin (R-V171P0-QC1-006).
 */
import { useEffect, useMemo, useRef, useState } from 'react';
import { useEdgesState, useNodesState, type Edge, type Node } from '@xyflow/react';

import {
  useActiveSession,
  useDerivedCreatorId,
  usePresetGraph,
  usePresetSchedules,
} from '@/lib/canvas/use-strategy-data';
import type { StrategyNodeData } from '@/lib/canvas/strategy-graph';

import {
  isSectionDirty,
  originalFormOf,
  selectedStateOf,
  templateRefOf,
  type EditForm,
  type SaveStatus,
  type Section,
} from '../state-machine';

export function useStrategyCanvas(presetId: string) {
  const graphQuery = usePresetGraph(presetId);
  const activeSession = useActiveSession(presetId);
  const schedules = usePresetSchedules(presetId);
  const creatorId = useDerivedCreatorId(presetId);

  const [nodes, setNodes, onNodesChange] = useNodesState<Node>([]);
  const [edges, setEdges, onEdgesChange] = useEdgesState<Edge>([]);

  const [form, setForm] = useState<EditForm>({ label: '', description: '', nextTarget: '', promptBody: '' });
  const [saveStatuses, setSaveStatuses] = useState<Partial<Record<Section, SaveStatus>>>({});
  const [activeSection, setActiveSection] = useState<Section>('state');
  const [conflict, setConflict] = useState<{ currentRevision: number; section: Section } | null>(null);
  const [saveTriggers, setSaveTriggers] = useState<Record<Section, number>>({
    state: 0,
    transition: 0,
    prompt: 0,
  });

  const workingRevisionRef = useRef(graphQuery.data?.revision ?? 0);

  useEffect(() => {
    if (graphQuery.data) {
      setNodes(graphQuery.data.graph.nodes as Node[]);
      setEdges(graphQuery.data.graph.edges as Edge[]);
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

  function handleConflict(currentRevision: number, section: Section) {
    setConflict({ currentRevision, section });
    void graphQuery.refetch();
  }

  function handleReapply() {
    if (!conflict) return;
    const section = conflict.section;
    setConflict(null);
    void graphQuery.refetch().then(() => {
      setSaveTriggers((prev) => ({ ...prev, [section]: prev[section] + 1 }));
    });
  }

  return {
    graphQuery,
    activeSession,
    schedules,
    creatorId,
    nodes,
    edges,
    onNodesChange,
    onEdgesChange,
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
  };
}
