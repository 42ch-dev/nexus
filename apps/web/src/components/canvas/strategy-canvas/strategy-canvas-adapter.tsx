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
  /**
   * Static trajectory of the open candidate, when a simulation has run. It is
   * part of the graph payload so a new trajectory re-projects the nodes.
   */
  trajectory: SimulationTrajectory | null;
  /**
   * The document being rendered: the applied IR document the status read
   * returned, or the candidate the author is composing. Projected by
   * `buildStrategyGraphFromIr` — there is no YAML fallback.
   */
  document: WorkflowDocument;
  /** Pattern registry listing; supplies declared slots for slot nodes. */
  patterns?: PatternSummary[];
  /** Slot node ids the author expanded into groups. */
  expandedSlots?: ReadonlySet<string>;
  /** Authored nest-into-slot containment chrome (child slot id → parent slot id). */
  containment?: ReadonlyMap<string, string>;
}

/**
 * Mutable context supplied by the orchestrator so the adapter can render
 * inspectors / alt-view / conflict-modal without closing over stale values.
 *
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
  /**
   * Projection byproduct — the adapter writes `danglingTargets` here after
   * projecting; the orchestrator reads it for the ValidationPanel (V1.115 T2).
   */
  danglingTargets: string[];
  /**
   * Compose-path (flag on) slot interactions. Absent when the flag is off, so
   * the slot inspector and expand toggle never render there.
   */
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
 * Strategy canvas adapter — projects the workflow IR document into React Flow
 * nodes/edges and renders surface-specific chrome (inspectors, alt-view, a11y
 * summary).
 *
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
      // One projection, from IR, always. The Canvas has no YAML payload to fall
      // back to (P6 condition 2).
      const projected = buildStrategyGraphFromIr(graph.document, {
        expandedSlots: graph.expandedSlots,
        patterns: graph.patterns,
        containment: graph.containment,
      });

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

      // Simulation overlay (T3b/T5): the trajectory names the nodes the static
      // walk visited — a wired pattern slot is named, and the overlay says so
      // on the slot itself.
      if (graph.trajectory) {
        const visited = new Set(graph.trajectory.nodes_visited);
        nodes = nodes.map((node) =>
          node.type === 'strategy-pattern-slot' && visited.has(node.id)
            ? { ...node, data: { ...node.data, simulated: true } }
            : node,
        );
      }

      // Compose-path slot expand/collapse toggle, patched onto slot node data.
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
      // Compose path: a selected pattern slot renders the slot inspector
      // (bind / replace / unnest). Slot nodes have no PresetState, so this
      // branch precedes the YAML-state inspector entirely.
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
