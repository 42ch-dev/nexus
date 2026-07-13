import type { MutableRefObject } from 'react';
import type { Edge, Node } from '@xyflow/react';

import type { CanvasSurfaceAdapter } from '../canvas-surface-adapter';
import type { ConflictModalProps } from '../conflict-modal';
import { StrategyAltView } from '../strategy-alt-view';
import { strategyNodeTypes } from '../strategy-nodes';
import { buildStrategyGraph } from '@/lib/canvas/strategy-graph';
import type { StrategyEdgeData, StrategyNodeData } from '@/lib/canvas/strategy-graph';
import type { ParsedPreset, PresetState } from '@/lib/canvas/preset-yaml';
import { isStrategyConflictError } from '@/lib/canvas/use-strategy-data';

import { EdgeInspector } from './inspectors/edge-inspector';
import { PromptInspector } from './inspectors/prompt-inspector';
import { StateInspector } from './inspectors/state-inspector';
import {
  getChangedFields,
  getConflictDraft,
  originalFormOf,
  type EditForm,
  type SaveStatus,
  type Section,
} from './state-machine';

/** Live session overlay data attached to the graph payload. */
export interface ActiveSession {
  current_task_id?: string;
  status: string;
}

/**
 * Graph payload consumed by the Strategy surface adapter.
 *
 * V1.115 P0 T2 (W001): the pre-projected `graph` field is dropped — the adapter
 * projects from `parsed` via `buildStrategyGraph` inside `projectGraph`. This
 * makes the adapter's contract honest (it owns projection, not a passthrough).
 */
export interface StrategySurfaceGraph {
  revision: number;
  parsed: ParsedPreset;
  activeSession: ActiveSession | null | undefined;
}

/**
 * Mutable context supplied by the orchestrator so the adapter can render
 * inspectors / alt-view / conflict-modal without closing over stale values.
 *
 * All fields are read at render time from the ref; the adapter object itself is
 * stable and never recreated.
 */
export interface StrategyCanvasAdapterContext {
  presetId: string;
  form: EditForm;
  saveTriggers: Record<Section, number>;
  saveStatuses: Partial<Record<Section, SaveStatus>>;
  workingRevisionRef: MutableRefObject<number>;
  handleConflict: (currentRevision: number, section: Section) => void;
  onChange: <K extends keyof EditForm>(field: K, value: EditForm[K]) => void;
  onSaveStatus: (section: Section, status: SaveStatus | undefined) => void;
  setActiveSection: (section: Section) => void;
  selectedState: PresetState | undefined;
  promptTemplateRef: string | undefined;
  selectedNode: Node<StrategyNodeData> | null;
  parsed: ParsedPreset | undefined;
  onUseCurrent: () => void;
  onReapply: () => void;
  onDismiss: () => void;
  /**
   * Projection byproduct — the adapter writes `danglingTargets` here after
   * projecting; the orchestrator reads it for the ValidationPanel (V1.115 T2).
   */
  danglingTargets: string[];
  /**
   * Local edge modifications (draft transition edges created by the author via
   * `onConnect`) that live outside the daemon-persisted graph. The adapter
   * merges them with the projected edges so drafts are visible on the canvas
   * without a second projection path (V1.115 T2).
   */
  localEdges: Edge<StrategyEdgeData>[];
}

export type StrategyCanvasAdapter = CanvasSurfaceAdapter<StrategySurfaceGraph, StrategyNodeData, StrategyEdgeData>;

/**
 * Strategy canvas adapter — projects the daemon preset graph into React Flow
 * nodes/edges and renders surface-specific chrome (inspectors, alt-view,
 * conflict modal, a11y summary).
 *
 * V1.115 P0 T2 (W001): `projectGraph` performs the real projection via
 * `buildStrategyGraph(parsed)` — it is no longer a passthrough. The projection
 * runs once inside `useCanvasSurface`'s `useMemo`, matching the old timing
 * (the query fn previously called `buildStrategyGraph`; now the adapter does).
 *
 * The returned adapter is stable; it reads mutable values from the supplied
 * context ref so the orchestrator can update state without invalidating the
 * hook's memoized graph projection.
 */
export function createStrategyCanvasAdapter(
  ctxRef: MutableRefObject<StrategyCanvasAdapterContext>,
): StrategyCanvasAdapter {
  return {
    surfaceKind: 'strategy',
    nodeTypes: strategyNodeTypes,
    edgeTypes: undefined,
    layoutOptions: { direction: 'TB' },

    projectGraph(graph) {
      const projected = buildStrategyGraph(graph.parsed);

      // Surface the dangling-targets byproduct via ctxRef so the orchestrator
      // can feed the ValidationPanel without extending the projectGraph return
      // shape (adapter interface stability constraint).
      ctxRef.current.danglingTargets = projected.danglingTargets;

      // Apply live session overlay — mark the current node with its session
      // status so strategy nodes can render the active indicator (previously
      // owned by useStrategyCanvas's node-state sync effect; moved here so the
      // adapter owns the full projection pipeline).
      let nodes = projected.nodes;
      const session = graph.activeSession;
      if (session) {
        const currentTask = session.current_task_id;
        const sessionStatus = session.status;
        nodes = nodes.map((n) => {
          const data = n.data as StrategyNodeData;
          const isCurrent =
            currentTask !== undefined &&
            (n.id === currentTask ||
              data.stateId === currentTask ||
              n.id.startsWith(`${currentTask}::`));
          return isCurrent
            ? { ...n, data: { ...data, status: sessionStatus ?? '__current__' } }
            : n;
        });
      }

      // Merge local edge modifications (draft transition edges created by the
      // author via onConnect) that are not yet persisted to the daemon graph.
      // Drafts have no matching projected edge id, so we append them.
      const localEdges = ctxRef.current.localEdges;
      const baseIds = new Set(projected.edges.map((e) => e.id));
      const drafts = localEdges.filter((e) => !baseIds.has(e.id));

      return {
        nodes,
        edges: [...projected.edges, ...drafts],
      };
    },

    adaptConflict(error) {
      if (!isStrategyConflictError(error)) return null;
      const currentRevision =
        typeof error.details === 'object' && error.details !== null
          ? (error.details as { current_revision?: number }).current_revision ?? 0
          : 0;
      const selectedState = ctxRef.current.selectedState;
      if (!selectedState) return null;
      return {
        open: true,
        currentRevision,
        draft: getConflictDraft(ctxRef.current.form),
        canonicalState: selectedState,
        promptTemplateRef: ctxRef.current.promptTemplateRef,
        changedFields: getChangedFields(ctxRef.current.form, originalFormOf(selectedState)),
        onUseCurrent: ctxRef.current.onUseCurrent,
        onReapply: ctxRef.current.onReapply,
        onDismiss: ctxRef.current.onDismiss,
      } satisfies ConflictModalProps;
    },

    renderInspector(_node) {
      const ctx = ctxRef.current;
      const selectedState = ctx.selectedState;
      if (!selectedState) return null;

      return (
        <>
          <StateInspector
            presetId={ctx.presetId}
            selectedState={selectedState}
            form={ctx.form}
            onChange={ctx.onChange}
            workingRevisionRef={ctx.workingRevisionRef}
            saveTrigger={ctx.saveTriggers.state}
            saveStatus={ctx.saveStatuses.state}
            onSaveStatus={(s) => ctx.onSaveStatus('state', s)}
            onConflict={ctx.handleConflict}
          />
          <div onFocusCapture={() => ctx.setActiveSection('transition')}>
            <EdgeInspector
              presetId={ctx.presetId}
              selectedState={selectedState}
              form={ctx.form}
              onChange={ctx.onChange}
              workingRevisionRef={ctx.workingRevisionRef}
              saveTrigger={ctx.saveTriggers.transition}
              saveStatus={ctx.saveStatuses.transition}
              onSaveStatus={(s) => ctx.onSaveStatus('transition', s)}
              onConflict={ctx.handleConflict}
            />
          </div>
          {ctx.promptTemplateRef ? (
            <div onFocusCapture={() => ctx.setActiveSection('prompt')}>
              <PromptInspector
                presetId={ctx.presetId}
                selectedState={selectedState}
                form={ctx.form}
                onChange={ctx.onChange}
                workingRevisionRef={ctx.workingRevisionRef}
                promptTemplateRef={ctx.promptTemplateRef}
                saveTrigger={ctx.saveTriggers.prompt}
                saveStatus={ctx.saveStatuses.prompt}
                onSaveStatus={(s) => ctx.onSaveStatus('prompt', s)}
                onConflict={ctx.handleConflict}
              />
            </div>
          ) : null}
        </>
      );
    },

    renderAltView() {
      const parsed = ctxRef.current.parsed;
      if (!parsed) return null;
      return <StrategyAltView parsed={parsed} statusByState={{}} />;
    },

    summarizeGraph(graph) {
      const states = graph.parsed.manifest.states;
      const count = states.length;
      let edgeCount = 0;
      for (const s of states) {
        if (typeof s.next === 'string') {
          edgeCount++;
        } else if (s.next && typeof s.next === 'object') {
          edgeCount += (s.next.rules?.length ?? 0) + (s.next.default ? 1 : 0);
        }
      }
      const sel = ctxRef.current.selectedNode ? ` Selected: ${ctxRef.current.selectedNode.id}.` : '';
      const live = graph.activeSession
        ? ` Current node: ${graph.activeSession.current_task_id ?? 'none'}. Session status: ${graph.activeSession.status}.`
        : ' No active session.';
      return `Strategy graph: ${count} states, ${edgeCount} transitions.${live}${sel}`;
    },
  };
}
