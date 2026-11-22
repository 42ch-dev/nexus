/**
 * note: The editing surface's view of the document the Canvas renders.
 *
 * The Canvas reads the applied document from `getStrategyApplied` (or the
 * candidate it is composing) and never parses preset YAML. The inspectors, the
 * branch table, the edge-create dialog and draft-edge authoring still speak the
 * `preset.yaml` state shape, so this maps a workflow IR document onto it.
 * Presentation only: no YAML is read, produced or written here.
 */
import type { WorkflowDocument } from '@42ch/nexus-contracts';

import type { ParsedPreset, PresetEnterTask, PresetState } from './preset-yaml';

/** note: Composition chrome and inner steps are not states of the machine. */
const NON_STATE_KINDS: Record<string, true> = {
  inner_step: true,
  pattern_slot: true,
  nest_group: true,
};

type IrNode = WorkflowDocument['nodes'][number];
type IrEdge = WorkflowDocument['edges'][number];

function enterOf(node: IrNode): PresetEnterTask[] {
  const actions: PresetEnterTask[] = (node.enter ?? []).map((action) => ({
    kind: action.kind,
    name: action.name ?? action.tool_name ?? undefined,
    args: action.args as Record<string, unknown> | undefined,
  }));
  // note: A node with no enter action still names its own kind, so the list view's
  // kind column reads `judge` or `manual_wait` instead of `unknown`.
  return actions.length > 0 ? actions : [{ kind: node.kind }];
}

/** note: One outgoing edge is a linear target; several become the conditional form. */
function nextOf(outgoing: IrEdge[]): PresetState['next'] {
  if (outgoing.length === 0) return undefined;
  if (outgoing.length === 1) {
    const [edge] = outgoing;
    // note: A lone fallback edge is unconditional; a lone guarded edge keeps its guard.
    if (edge.kind === 'default' || (edge.when ?? edge.label) === undefined) return edge.to;
  }
  const fallback = outgoing.find((edge) => edge.kind === 'default');
  return {
    kind: 'conditional',
    rules: outgoing
      .filter((edge) => edge.id !== fallback?.id)
      .map((edge) => ({ when: edge.when ?? edge.label ?? undefined, to: edge.to })),
    default: fallback?.to,
  };
}

export function parsedPresetFromIr(document: WorkflowDocument): ParsedPreset {
  // note: `depends_on` is an inner-step dependency carried as an edge, never a
  // state transition.
  const transitions = document.edges.filter((edge) => edge.kind !== 'depends_on');
  const states: PresetState[] = document.nodes
    .filter((node) => NON_STATE_KINDS[node.kind] !== true)
    .map((node) => ({
      id: node.id,
      description: node.ui?.description ?? undefined,
      enter: enterOf(node),
      exit_when: node.exit_when ? { kind: node.exit_when.kind } : undefined,
      next: nextOf(transitions.filter((edge) => edge.from === node.id)),
      converge: node.converge ? { strategy: node.converge.strategy } : undefined,
      terminal: node.kind === 'terminal' || document.terminal.includes(node.id),
      context_update: node.context_update?.template
        ? { template_file: node.context_update.template }
        : undefined,
    }));

  return {
    manifest: {
      preset: { id: document.id, version: document.version, initial: document.entry },
      states,
    },
    revision: document.revision,
    // note: The wire document is already parsed and validated; nothing to report.
    problems: [],
  };
}
