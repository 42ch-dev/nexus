/**
 * Simulation fixtures — a minimal candidate IR and its trajectory for the
 * Approve-disclosure and canvas-overlay tests.
 */
import type { SimulationTrajectory, WorkflowDocument } from '@42ch/nexus-contracts';

export function candidateDocument(overrides: Partial<WorkflowDocument> = {}): WorkflowDocument {
  return {
    ir_version: 0,
    id: 'user/demo',
    version: 1,
    revision: 1,
    content_hash: 'hash-1',
    state: 'candidate',
    meta: {
      description: 'Demo candidate',
      kind: 'creator',
      run_intents: ['work_init'],
      requires_capabilities: [],
    },
    entry: 'draft',
    terminal: ['done'],
    nodes: [
      { id: 'draft', kind: 'outer_state', effect_class: 'pure' },
      { id: 'routing', kind: 'judge', effect_class: 'pure', exit_when: { kind: 'llm_judge' } },
      { id: 'pause', kind: 'manual_wait', effect_class: 'pure' },
      { id: 'done', kind: 'terminal', effect_class: 'pure' },
    ],
    edges: [
      { id: 'draft->routing', from: 'draft', to: 'routing', kind: 'linear' },
      { id: 'routing->done', from: 'routing', to: 'done', kind: 'default' },
      { id: 'routing->pause', from: 'routing', to: 'pause', kind: 'conditional' },
    ],
    nests: [],
    assets: [],
    signals: [],
    roles: [],
    effect_summary: 'pure',
    provenance: { source: 'compose' },
    ...overrides,
  };
}

export function simulationTrajectory(
  overrides: Partial<SimulationTrajectory> = {},
): SimulationTrajectory {
  return {
    schema_version: 1,
    ir_content_hash: 'hash-1',
    simulation_level: 'reachability',
    nodes_visited: ['done', 'draft', 'routing'],
    predicted_waits: [{ reason: 'manual', subject: 'pause' }],
    effect_summary: 'write_candidate',
    stubbed_judges: ['routing'],
    ...overrides,
  };
}
