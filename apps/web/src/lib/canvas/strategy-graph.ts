/**
 * Strategy graph adapter — read projection from a parsed preset into React Flow
 * `nodes`/`edges` (canvas-strategy-surface.md Draft §3.2 mapping table).
 *
 * Mapping (orchestration-engine.md §3 graph-of-graphs → @xyflow/react):
 *   • Outer state-machine state        → top-level node
 *   • `inner_graph` enter state        → group node (parentId children + extent:parent)
 *   • Converge merge-point state       → join node
 *   • terminal state                   → terminal node
 *   • linear `next`                    → edge (transitionKind "next")
 *   • conditional `next.rules[].to`    → branch edges; `next.default` → default edge
 *   • `inner_graphs.<n>.depends_on`    → depends_on edges inside the group
 *
 * UI label is **Strategy**; persisted identifiers remain `preset`/runtime names
 * (Draft §4.2 — no CLI/schema rename in V1.70). This adapter is a read
 * projection only — `wire_contracts_changed: FALSE`.
 *
 * simplify: layout is a naive BFS layering, not a full graph-layout engine.
 * Adequate for α (presets have ≤~10 outer states). Upgrade path: swap
 * `layoutGraph` for dagre/elkjs when node counts grow or user dragging needs
 * stable ports — the node/edge output shape stays unchanged.
 */
import type { Edge, Node } from '@xyflow/react';

import {
  innerGraphIdOf,
  stateKind,
  type ConvergeStrategy,
  type ParsedPreset,
  type PresetState,
} from './preset-yaml';

/** Data carried by a Strategy node (Draft §3.4 StrategyNodeData). */
export interface StrategyNodeData extends Record<string, unknown> {
  stateId: string;
  label: string;
  stateKind: string;
  presetId: string;
  innerGraphId?: string;
  convergeStrategy?: ConvergeStrategy;
  isTerminal: boolean;
  isInitial: boolean;
  isGroup: boolean;
  /** Live overlay status (filled by the overlay layer, not the adapter). */
  status?: string;
  description?: string;
}

/** Data carried by a Strategy edge (Draft §3.4 StrategyEdgeData). */
export interface StrategyEdgeData extends Record<string, unknown> {
  transitionKind: 'next' | 'branch' | 'default' | 'converge' | 'depends_on';
  condition?: string;
}

export interface StrategyGraph {
  nodes: Node<StrategyNodeData>[];
  edges: Edge<StrategyEdgeData>[];
  /** States whose `next` target is unknown (reported to the validation panel). */
  danglingTargets: string[];
}

const LAYER_HEIGHT = 140;
const NODE_SPACING_X = 240;
const GROUP_PADDING = 24;
const INNER_NODE_SPACING_Y = 90;

/**
 * Build the Strategy graph projection from a parsed preset.
 */
export function buildStrategyGraph(parsed: ParsedPreset): StrategyGraph {
  const { manifest } = parsed;
  const states = manifest.states;
  const byId = new Map<string, PresetState>();
  for (const s of states) byId.set(s.id, s);

  const initialId = manifest.preset.initial;
  const terminalId = manifest.preset.terminal;

  // ── Layers (BFS from initial) ────────────────────────────────────────────
  const layerOf = new Map<string, number>();
  if (initialId && byId.has(initialId)) {
    const queue: string[] = [initialId];
    layerOf.set(initialId, 0);
    while (queue.length > 0) {
      const id = queue.shift()!;
      const layer = layerOf.get(id)!;
      for (const target of nextTargets(byId.get(id)!)) {
        if (!layerOf.has(target)) {
          layerOf.set(target, layer + 1);
          queue.push(target);
        }
      }
    }
  }
  // Orphan states (unreachable from initial) get placed after the last layer.
  let maxLayer = -1;
  for (const l of layerOf.values()) if (l > maxLayer) maxLayer = l;
  for (const s of states) if (!layerOf.has(s.id)) layerOf.set(s.id, maxLayer + 1);

  // Bucket states per layer to spread them horizontally.
  const byLayer = new Map<number, string[]>();
  for (const [id, layer] of layerOf) {
    const bucket = byLayer.get(layer) ?? [];
    bucket.push(id);
    byLayer.set(layer, bucket);
  }
  const xOf = new Map<string, number>();
  for (const [, bucket] of byLayer) {
    bucket.sort();
    const total = (bucket.length - 1) * NODE_SPACING_X;
    bucket.forEach((id, i) => xOf.set(id, i * NODE_SPACING_X - total / 2));
  }

  const danglingTargets: string[] = [];
  const nodes: Node<StrategyNodeData>[] = [];
  const edges: Edge<StrategyEdgeData>[] = [];

  // ── Outer state nodes (+ group children for inner-graph states) ──────────
  for (const state of states) {
    const layer = layerOf.get(state.id) ?? 0;
    const x = xOf.get(state.id) ?? 0;
    const y = layer * LAYER_HEIGHT;
    const kind = stateKind(state);
    const graphId = innerGraphIdOf(state);
    const isGroup = graphId !== undefined && manifest.inner_graphs?.[graphId] !== undefined;
    const isInitial = state.id === initialId;
    const isTerminal = state.terminal === true || state.id === terminalId;

    const nodeType = isTerminal
      ? 'strategy-terminal'
      : state.converge
        ? 'strategy-join'
        : isGroup
          ? 'strategy-group'
          : 'strategy-state';

    nodes.push({
      id: state.id,
      type: nodeType,
      position: { x, y },
      data: {
        stateId: state.id,
        label: state.id,
        stateKind: kind,
        presetId: manifest.preset.id,
        innerGraphId: graphId,
        convergeStrategy: state.converge?.strategy,
        isTerminal,
        isInitial,
        isGroup,
        description: state.description,
      },
      draggable: true,
      selectable: true,
      focusable: true,
    });

    // Inner-graph child nodes live inside the group (parentId + extent:parent).
    if (isGroup && graphId) {
      const graph = manifest.inner_graphs![graphId];
      graph.nodes.forEach((inner, i) => {
        nodes.push({
          id: `${state.id}::${inner.id}`,
          type: 'strategy-inner',
          position: { x: GROUP_PADDING, y: GROUP_PADDING + 40 + i * INNER_NODE_SPACING_Y },
          data: {
            stateId: inner.id,
            label: inner.id,
            stateKind: inner.kind,
            presetId: manifest.preset.id,
            innerGraphId: graphId,
            isTerminal: false,
            isInitial: false,
            isGroup: false,
          },
          parentId: state.id,
          extent: 'parent',
          draggable: true,
          selectable: true,
          focusable: true,
        });
      });
    }
  }

  // ── Outer transition edges ───────────────────────────────────────────────
  for (const state of states) {
    const source = state.id;
    const next = state.next;
    if (typeof next === 'string') {
      pushOuterEdge(edges, source, next, { transitionKind: 'next' }, byId, danglingTargets);
    } else if (next && typeof next === 'object') {
      for (const rule of next.rules ?? []) {
        pushOuterEdge(
          edges,
          source,
          rule.to,
          { transitionKind: 'branch', condition: rule.when },
          byId,
          danglingTargets,
        );
      }
      if (next.default) {
        pushOuterEdge(
          edges,
          source,
          next.default,
          { transitionKind: 'default' },
          byId,
          danglingTargets,
        );
      }
    }
  }

  // ── Inner-graph depends_on edges ─────────────────────────────────────────
  for (const state of states) {
    const graphId = innerGraphIdOf(state);
    if (!graphId) continue;
    const graph = manifest.inner_graphs?.[graphId];
    if (!graph) continue;
    for (const inner of graph.nodes) {
      const innerNodeId = `${state.id}::${inner.id}`;
      for (const dep of inner.depends_on ?? []) {
        const depNodeId = `${state.id}::${dep}`;
        if (graph.nodes.some((n) => n.id === dep)) {
          edges.push({
            id: `e-${innerNodeId}-dep-${depNodeId}`,
            source: depNodeId,
            target: innerNodeId,
            type: 'strategy-edge',
            data: { transitionKind: 'depends_on' },
            selectable: true,
            focusable: true,
          });
        }
      }
    }
  }

  return { nodes, edges, danglingTargets };
}

function pushOuterEdge(
  edges: Edge<StrategyEdgeData>[],
  source: string,
  target: string,
  data: StrategyEdgeData,
  byId: Map<string, PresetState>,
  dangling: string[],
): void {
  if (!byId.has(target)) {
    dangling.push(`${source} → ${target}`);
    return;
  }
  // Multiple branches from the same source → same target (same transitionKind)
  // would otherwise collide on id. Append the current edges.length as a
  // disambiguator so React Flow keeps every conditional rule visible.
  edges.push({
    id: `e-${source}-${target}-${data.transitionKind}-${edges.length}`,
    source,
    target,
    type: 'strategy-edge',
    label: data.condition,
    data,
    selectable: true,
    focusable: true,
  });
}

/** Resolve all `next` target state ids from a state (linear + conditional). */
function nextTargets(state: PresetState): string[] {
  const next = state.next;
  if (typeof next === 'string') return [next];
  if (next && typeof next === 'object') {
    const targets: string[] = [];
    for (const rule of next.rules ?? []) targets.push(rule.to);
    if (next.default) targets.push(next.default);
    return targets;
  }
  return [];
}
