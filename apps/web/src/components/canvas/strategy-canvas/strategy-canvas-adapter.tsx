import type { MutableRefObject } from 'react';
import type { Node } from '@xyflow/react';

import type { CanvasSurfaceAdapter } from '../canvas-surface-adapter';
import type { ConflictModalProps } from '../conflict-modal';
import { StrategyAltView } from '../strategy-alt-view';
import { strategyNodeTypes } from '../strategy-nodes';
import type { StrategyEdgeData, StrategyGraph, StrategyNodeData } from '@/lib/canvas/strategy-graph';
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

/** Graph payload consumed by the Strategy surface adapter. */
export interface StrategySurfaceGraph {
  revision: number;
  graph: StrategyGraph;
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
}

export type StrategyCanvasAdapter = CanvasSurfaceAdapter<StrategySurfaceGraph, StrategyNodeData, StrategyEdgeData>;

/**
 * Strategy canvas adapter — projects the daemon preset graph into React Flow
 * nodes/edges and renders surface-specific chrome (inspectors, alt-view,
 * conflict modal, a11y summary).
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
      return {
        nodes: graph.graph.nodes,
        edges: graph.graph.edges,
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
      const count = graph.graph.nodes.length;
      const edgeCount = graph.graph.edges.length;
      const sel = ctxRef.current.selectedNode ? ` Selected: ${ctxRef.current.selectedNode.id}.` : '';
      const live = graph.activeSession
        ? ` Current node: ${graph.activeSession.current_task_id ?? 'none'}. Session status: ${graph.activeSession.status}.`
        : ' No active session.';
      return `Strategy graph: ${count} states, ${edgeCount} transitions.${live}${sel}`;
    },
  };
}
