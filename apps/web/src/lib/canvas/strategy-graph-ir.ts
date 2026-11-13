/**
 * Strategy IR projection — the composition path's read projection from
 * `workflow-ir` DTOs to the React Flow graph the Strategy Canvas renders
 * (v239-p3-canvas-compose-ui T1).
 *
 * The YAML projection (`buildStrategyGraph`) stays the compatibility view for
 * legacy bundles; this projection is the only path used when the
 * `strategy_canvas_compose` flag is on — one projection runs, never both,
 * never merged per node (plan risk mitigation).
 *
 * IR node-id conventions (nexus-preset `workflow_ir.rs`):
 *   • outer state              → node id is the state id; `terminal` kind → terminal node
 *   • inner step               → `<inner_graph>/<node_id>`, owned by the outer state whose
 *                                 `enter` carries `inner_graph` with that graph name
 *   • pattern slot             → the slot node of a top-level `nests[]` binding
 *                                 (`nests[].parent_node` names it)
 *   • judge / manual wait      → state-shaped nodes; the trajectory overlay names them
 *
 * `depends_on` is a node field in IR (not an edge): inner steps declare
 * `<graph>/<dep>` ids, so the projection re-materialises those edges.
 *
 * Layout reuses the YAML projection's naive BFS layering (simplify comment in
 * `strategy-graph.ts` applies unchanged). Children carry positions relative
 * to their parent's top-left corner (`parentId` + `extent: 'parent'`) and
 * parents are emitted before children, so React Flow's parents-first group
 * ordering renders and drags three nesting levels without manual absolute
 * positioning (T8, proven by the P3-1 spike).
 */
import type { Edge, Node } from '@xyflow/react';

import type { WorkflowDocument } from '@42ch/nexus-contracts';

import type { StrategyEdgeData, StrategyGraph, StrategyNodeData } from './strategy-graph';

/** A document node — the generated barrel keeps the inlined member types module-local. */
export type IrNode = WorkflowDocument['nodes'][number];

/** Slot declaration + binding state carried by a `pattern_slot` node's data. */
export interface SlotBindingRow {
  name: string;
  required: boolean;
  kind: string;
  /** Bound value from the node's `bind` map; `undefined` while unbound. */
  value: unknown;
}

/** Minimal registry summary shape the projection needs (matches `PatternSummary`). */
export interface SlotPatternSummary {
  id: string | null;
  version: string | null;
  slots: { name: string; kind: string; required: boolean }[];
  effect_class: string | null;
}

export interface StrategyIrProjectionOptions {
  /** Slot node ids the author expanded; an expanded slot renders as a group with children. */
  expandedSlots?: ReadonlySet<string>;
  /** Pattern registry listing; supplies declared slots for the pinned pattern. */
  patterns?: readonly SlotPatternSummary[];
  /**
   * Authored nest-into-slot containment: child slot node id → parent slot node id.
   * The compose wire records no graph position for a nested region (the fragment
   * owns it), so the canvas keeps the spatial choice the author made and re-applies
   * it after every committed op.
   */
  containment?: ReadonlyMap<string, string>;
}

const LAYER_HEIGHT = 140;
const NODE_SPACING_X = 240;
const GROUP_PADDING = 24;
const INNER_NODE_SPACING_Y = 90;

function innerGraphNameOf(node: IrNode): string | undefined {
  return node.enter?.find((action) => action.kind === 'inner_graph')?.name ?? undefined;
}

function slotRowsFor(
  node: IrNode,
  patterns: readonly SlotPatternSummary[],
): SlotBindingRow[] {
  const summary = patterns.find(
    (pattern) => pattern.id === node.pattern?.registry_id && pattern.version === node.pattern?.version,
  );
  const declared = summary?.slots ?? [];
  const rows = declared.map((slot) => ({
    name: slot.name,
    required: slot.required,
    kind: slot.kind,
    value: node.bind?.[slot.name],
  }));
  // A binding the manifest does not declare still renders (reported, never hidden).
  for (const [name, value] of Object.entries(node.bind ?? {})) {
    if (!rows.some((row) => row.name === name)) {
      rows.push({ name, required: false, kind: 'unknown', value });
    }
  }
  return rows;
}

/**
 * Build the Strategy graph projection from a workflow IR document.
 */
export function buildStrategyGraphFromIr(
  document: WorkflowDocument,
  options: StrategyIrProjectionOptions = {},
): StrategyGraph {
  const { expandedSlots, patterns = [], containment = new Map() } = options;
  const nodes = document.nodes;
  const byId = new Map(nodes.map((node) => [node.id, node]));

  // ── Layers (BFS from entry over document edges) ──────────────────────────
  const layerOf = new Map<string, number>();
  if (byId.has(document.entry)) {
    const queue: string[] = [document.entry];
    layerOf.set(document.entry, 0);
    while (queue.length > 0) {
      const id = queue.shift()!;
      const layer = layerOf.get(id)!;
      for (const edge of document.edges) {
        if (edge.from !== id || layerOf.has(edge.to) || !byId.has(edge.to)) continue;
        layerOf.set(edge.to, layer + 1);
        queue.push(edge.to);
      }
    }
  }
  let maxLayer = -1;
  for (const layer of layerOf.values()) if (layer > maxLayer) maxLayer = layer;
  for (const node of nodes) {
    if (node.kind !== 'inner_step' && !layerOf.has(node.id)) layerOf.set(node.id, maxLayer + 1);
  }
  const byLayer = new Map<number, string[]>();
  for (const node of nodes) {
    if (node.kind === 'inner_step') continue;
    const layer = layerOf.get(node.id) ?? 0;
    const bucket = byLayer.get(layer) ?? [];
    bucket.push(node.id);
    byLayer.set(layer, bucket);
  }
  const xOf = new Map<string, number>();
  for (const [, bucket] of byLayer) {
    bucket.sort();
    const total = (bucket.length - 1) * NODE_SPACING_X;
    bucket.forEach((id, i) => xOf.set(id, i * NODE_SPACING_X - total / 2));
  }

  const danglingTargets: string[] = [];
  const rfNodes: Node<StrategyNodeData>[] = [];
  const rfEdges: Edge<StrategyEdgeData>[] = [];

  const pushEdge = (
    id: string,
    from: string,
    to: string,
    data: StrategyEdgeData,
    label?: string | null,
  ) => {
    if (!byId.has(to)) {
      danglingTargets.push(`${from} → ${to}`);
      return;
    }
    rfEdges.push({
      id,
      source: from,
      target: to,
      type: 'strategy-edge',
      label: label ?? undefined,
      data,
      selectable: true,
      focusable: true,
    });
  };

  // ── Outer-level nodes (parents before children) ──────────────────────────
  const innerOwners = new Map<string, string>(); // inner graph name → owning outer state id
  for (const node of nodes) {
    const graphName = innerGraphNameOf(node);
    if (graphName) innerOwners.set(graphName, node.id);
  }

  const slotNestId = new Map<string, string>(); // slot node id → nest id
  for (const nest of document.nests) slotNestId.set(nest.parent_node, nest.id);

  /**
   * A contained slot is hidden exactly while no expanded ancestor emits it: a
   * collapsed parent must keep it in the node set (hidden) so the edges naming
   * it keep both endpoints, and an expanded parent emits it as its own child.
   */
  const hiddenSlot = (id: string, seen: Set<string> = new Set()): boolean => {
    const parent = containment.get(id);
    if (parent === undefined) return false;
    if (seen.has(id)) return false; // chrome cycle: treat as top level rather than hide both
    seen.add(id);
    return !(expandedSlots?.has(parent) === true && !hiddenSlot(parent, seen));
  };

  for (const node of nodes) {
    if (node.kind === 'inner_step') continue;
    const contained = node.kind === 'pattern_slot' && containment.has(node.id);
    // An expanded parent emits this slot as its child (parents-first); otherwise
    // it stays here as a hidden node rather than dropping out of the graph.
    const hidden = contained && hiddenSlot(node.id);
    if (contained && !hidden) continue;
    const layer = layerOf.get(node.id) ?? 0;
    const position = { x: xOf.get(node.id) ?? 0, y: layer * LAYER_HEIGHT };
    const isInitial = node.id === document.entry;
    const isTerminal = node.kind === 'terminal' || document.terminal.includes(node.id);
    const isSlot = node.kind === 'pattern_slot';
    const expanded = isSlot && !hidden && expandedSlots?.has(node.id) === true;

    const data: StrategyNodeData = {
      stateId: node.id,
      label: node.ui?.label ?? node.id,
      stateKind: node.kind,
      presetId: document.id,
      isTerminal,
      isInitial,
      isGroup: expanded || innerGraphNameOf(node) !== undefined,
      description: node.ui?.description ?? undefined,
      ...(node.converge ? { convergeStrategy: node.converge.strategy } : {}),
      ...(isSlot
        ? {
            patternId: node.pattern?.registry_id ?? '',
            patternVersion: node.pattern?.version ?? '',
            patternEffectClass: node.effect_class,
            slotRows: slotRowsFor(node, patterns),
            unboundRequired: slotRowsFor(node, patterns)
              .filter((row) => row.required && row.value === undefined)
              .map((row) => row.name),
            nestId: slotNestId.get(node.id),
            expanded: false,
          }
        : {}),
    };

    rfNodes.push({
      id: node.id,
      type: isTerminal
        ? 'strategy-terminal'
        : node.kind === 'join'
          ? 'strategy-join'
          : isSlot
            ? 'strategy-pattern-slot'
            : innerGraphNameOf(node)
              ? 'strategy-group'
              : 'strategy-state',
      position,
      data,
      ...(hidden ? { hidden: true } : {}),
      draggable: true,
      selectable: true,
      focusable: true,
    });

    // Expanded slot: materialise the fragment inline — declared slots as child
    // binding rows and authored nested slots as child slot groups (three
    // levels deep at most, matching the IR nest policy).
    if (isSlot && expanded) {
      const parent = rfNodes[rfNodes.length - 1];
      parent.data = { ...parent.data, isGroup: true, expanded: true };
      pushSlotSubtree(rfNodes, node, options, byId, slotNestId, 1);
    }
  }

  // Inner steps as children of their owning outer state (parents-first: owners emitted above).
  const innerByGraph = new Map<string, IrNode[]>();
  for (const node of nodes) {
    if (node.kind !== 'inner_step') continue;
    const graphName = node.id.split('/')[0];
    const bucket = innerByGraph.get(graphName) ?? [];
    bucket.push(node);
    innerByGraph.set(graphName, bucket);
  }
  for (const [graphName, steps] of innerByGraph) {
    const ownerId = innerOwners.get(graphName);
    if (!ownerId) continue;
    steps.forEach((step, i) => {
      rfNodes.push({
        id: step.id,
        type: 'strategy-inner',
        position: { x: GROUP_PADDING, y: GROUP_PADDING + 40 + i * INNER_NODE_SPACING_Y },
        data: {
          stateId: step.id,
          label: step.id.split('/')[1] ?? step.id,
          stateKind: step.kind,
          presetId: document.id,
          isTerminal: false,
          isInitial: false,
          isGroup: false,
        },
        parentId: ownerId,
        extent: 'parent',
        draggable: true,
        selectable: true,
        focusable: true,
      });
    });
  }

  // ── Document edges ───────────────────────────────────────────────────────
  for (const edge of document.edges) {
    const data: StrategyEdgeData =
      edge.kind === 'linear'
        ? { transitionKind: 'next' }
        : edge.kind === 'default'
          ? { transitionKind: 'default' }
          : edge.kind === 'depends_on'
            ? { transitionKind: 'depends_on' }
            : { transitionKind: 'branch', condition: edge.when ?? edge.label ?? edge.kind };
    pushEdge(`e-${edge.id}`, edge.from, edge.to, data, edge.label ?? edge.when);
  }

  // Inner-step depends_on edges (IR carries them on the node, not as edges).
  for (const node of nodes) {
    if (node.kind !== 'inner_step') continue;
    for (const dep of node.depends_on ?? []) {
      if (!byId.has(dep)) {
        danglingTargets.push(`${node.id} → ${dep}`);
        continue;
      }
      rfEdges.push({
        id: `e-${node.id}-dep-${dep}`,
        source: dep,
        target: node.id,
        type: 'strategy-edge',
        data: { transitionKind: 'depends_on' },
        selectable: true,
        focusable: true,
      });
    }
  }

  return { nodes: rfNodes, edges: rfEdges, danglingTargets };
}

interface SlotChild {
  id: string;
  type: string;
  data: StrategyNodeData;
}

/**
 * Children of an expanded slot: declared slot binding rows, then authored
 * nested slots. Nested slots that are themselves expanded recurse (the
 * parents-first push order is preserved: each parent lands before its
 * children), bounded by the IR nest policy's three levels.
 */
function pushSlotSubtree(
  rfNodes: Node<StrategyNodeData>[],
  slot: IrNode,
  options: StrategyIrProjectionOptions,
  byId: Map<string, IrNode>,
  slotNestId: ReadonlyMap<string, string>,
  depth: number,
): void {
  const { patterns = [], containment = new Map(), expandedSlots } = options;
  const children: SlotChild[] = [];
  for (const row of slotRowsFor(slot, patterns)) {
    children.push({
      id: `${slot.id}::slot-row::${row.name}`,
      type: 'strategy-inner',
      data: {
        stateId: row.name,
        label: row.name,
        stateKind: 'slot_binding',
        presetId: slot.id,
        isTerminal: false,
        isInitial: false,
        isGroup: false,
        slotBindingRow: row,
      },
    });
  }
  for (const [childId, parentId] of containment) {
    if (parentId !== slot.id) continue;
    const child = byId.get(childId);
    if (!child || child.kind !== 'pattern_slot') continue;
    const rows = slotRowsFor(child, patterns);
    children.push({
      id: childId,
      type: 'strategy-pattern-slot',
      data: {
        stateId: child.id,
        label: child.ui?.label ?? child.id,
        stateKind: child.kind,
        presetId: slot.id,
        isTerminal: false,
        isInitial: false,
        isGroup: false,
        patternId: child.pattern?.registry_id ?? '',
        patternVersion: child.pattern?.version ?? '',
        patternEffectClass: child.effect_class,
        // The nested slot is editable exactly like a top-level one: its binding
        // op addresses the nest whose `parent_node` names it.
        nestId: slotNestId.get(child.id),
        slotRows: rows,
        unboundRequired: rows.filter((row) => row.required && row.value === undefined).map((row) => row.name),
        expanded: false,
      },
    });
  }
  children.forEach((child, i) => {
    rfNodes.push({
      id: child.id,
      type: child.type,
      position: { x: GROUP_PADDING, y: GROUP_PADDING + 40 + i * INNER_NODE_SPACING_Y },
      data: child.data,
      parentId: slot.id,
      extent: 'parent',
      draggable: true,
      selectable: true,
      focusable: true,
    });
    if (child.type === 'strategy-pattern-slot' && expandedSlots?.has(child.id) && depth < 3) {
      const childIr = byId.get(child.id);
      if (!childIr) return;
      rfNodes[rfNodes.length - 1] = {
        ...rfNodes[rfNodes.length - 1],
        data: { ...rfNodes[rfNodes.length - 1].data, isGroup: true, expanded: true },
      };
      pushSlotSubtree(rfNodes, childIr, options, byId, slotNestId, depth + 1);
    }
  });
}
