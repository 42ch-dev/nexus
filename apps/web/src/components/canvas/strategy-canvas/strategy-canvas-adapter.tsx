import type { MutableRefObject } from 'react';
import type { Node } from '@xyflow/react';

import type { SimulationTrajectory, WorkflowDocument, PatternSummary } from '@42ch/nexus-contracts';

import type { CanvasSurfaceAdapter } from '../canvas-surface-adapter';
import { StrategyAltView } from '../strategy-alt-view';
import { strategyNodeTypes } from '../strategy-nodes';
import { buildStrategyGraphFromIr } from '@/lib/canvas/strategy-graph-ir';
import type { StrategyEdgeData, StrategyNodeData } from '@/lib/canvas/strategy-graph';
import type { ParsedPreset, PresetState } from '@/lib/canvas/preset-yaml';

import { EdgeInspector } from './inspectors/edge-inspector';
import { PromptInspector } from './inspectors/prompt-inspector';
import { SlotInspector } from './inspectors/slot-inspector';
import { StateInspector } from './inspectors/state-inspector';
import { applyTrajectoryOverlay } from './simulation-overlay';
import { type EditForm, type Section } from './state-machine';

/** Live session overlay data attached to the graph payload. */
export interface ActiveSession {
  current_task_id?: string;
  status: string;
}

/**
 * Graph payload consumed by the Strategy surface adapter.
 *
 * The Canvas runs on the applied IR document (P6 condition 2): `document` is
 * the one projection source and carries no YAML anywhere in the payload.
 */
export interface StrategySurfaceGraph {
  revision: number;
  activeSession: ActiveSession | null | undefined;
  trajectory: SimulationTrajectory | null;
  document: WorkflowDocument;
  /** Pattern registry listing; supplies declared slots for slot nodes. */
  patterns?: PatternSummary[];
  /** Slot node ids the author expanded into groups. */
  expandedSlots?: ReadonlySet<string>;
  /** Authored nest-into-slot containment chrome (child slot id → parent slot id). */
  containment?: ReadonlyMap<string, string>;
}

/**
 * All fields are read at render time from the ref; the adapter object itself is
 * stable and never recreated.
 */
export interface StrategyCanvasAdapterContext {
  form: EditForm;
  onChange: <K extends keyof EditForm>(field: K, value: EditForm[K]) => void;
  setActiveSection: (section: Section) => void;
  selectedState: PresetState | undefined;
  promptTemplateRef: string | undefined;
  selectedNode: Node<StrategyNodeData> | null;
  parsed: ParsedPreset | undefined;
  danglingTargets: string[];
  compose?: {
    isComposing: boolean;
    onToggleSlotExpanded: (slotNodeId: string) => void;
    onBindSlot: (nestId: string, slot: string, value: unknown) => Promise<void>;
    onReplaceSubtree: (nestId: string, pkg: { id: string; version: string }) => Promise<void>;
    onUnnest: (nestId: string) => Promise<void>;
  };
  /** Pattern registry listing for the slot inspector / projection (compose path). */
  patterns?: PatternSummary[];
}

export type StrategyCanvasAdapter = CanvasSurfaceAdapter<StrategySurfaceGraph, StrategyNodeData, StrategyEdgeData>;

/**
 * `projectGraph` performs the projection via `buildStrategyGraphFromIr` inside
 * `useCanvasSurface`'s `useMemo`. The Canvas renders the applied IR document, so
 * there is no YAML projection to choose between (P6 condition 2).
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
      const projected = buildStrategyGraphFromIr(graph.document, {
        expandedSlots: graph.expandedSlots,
        patterns: graph.patterns,
        containment: graph.containment,
      });

      ctxRef.current.danglingTargets = projected.danglingTargets;

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

      if (graph.trajectory) {
        const visited = new Set(graph.trajectory.nodes_visited);
        nodes = nodes.map((node) =>
          node.type === 'strategy-pattern-slot' && visited.has(node.id)
            ? { ...node, data: { ...node.data, simulated: true } }
            : node,
        );
      }

      if (ctxRef.current.compose) {
        const toggle = ctxRef.current.compose.onToggleSlotExpanded;
        nodes = nodes.map((node) =>
          node.type === 'strategy-pattern-slot'
            ? { ...node, data: { ...node.data, onToggleExpand: () => toggle(node.id) } }
            : node,
        );
      }

      return {
        nodes: applyTrajectoryOverlay(nodes, graph.trajectory),
        edges: projected.edges,
      };
    },

    renderInspector(_node) {
      const ctx = ctxRef.current;
      const selectedNode = ctx.selectedNode;
      if (ctx.compose && selectedNode?.type === 'strategy-pattern-slot') {
        return (
          <SlotInspector
            node={selectedNode.data as StrategyNodeData}
            patterns={ctx.patterns ?? []}
            isComposing={ctx.compose.isComposing}
            onBindSlot={ctx.compose.onBindSlot}
            onReplaceSubtree={ctx.compose.onReplaceSubtree}
            onUnnest={ctx.compose.onUnnest}
          />
        );
      }
      const selectedState = ctx.selectedState;
      if (!selectedState) return null;

      return (
        <>
          <StateInspector form={ctx.form} onChange={ctx.onChange} />
          <div onFocusCapture={() => ctx.setActiveSection('transition')}>
            <EdgeInspector selectedState={selectedState} form={ctx.form} onChange={ctx.onChange} />
          </div>
          {ctx.promptTemplateRef ? (
            <div onFocusCapture={() => ctx.setActiveSection('prompt')}>
              <PromptInspector
                form={ctx.form}
                onChange={ctx.onChange}
                promptTemplateRef={ctx.promptTemplateRef}
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
      const { document } = graph;
      const slots = document.nodes.filter((node) => node.kind === 'pattern_slot').length;
      const visited = graph.trajectory
        ? ` Simulated nodes: ${graph.trajectory.nodes_visited.join(', ')}.`
        : ' Not simulated.';
      const sel = ctxRef.current.selectedNode ? ` Selected: ${ctxRef.current.selectedNode.id}.` : '';
      const live = graph.activeSession
        ? ` Current node: ${graph.activeSession.current_task_id ?? 'none'}. Session status: ${graph.activeSession.status}.`
        : ' No active session.';
      return `Strategy preset ${document.id} at revision ${graph.revision}: ${document.nodes.length} nodes, ${document.edges.length} edges, ${slots} pattern slots.${visited}${live}${sel}`;
    },
  };
}
