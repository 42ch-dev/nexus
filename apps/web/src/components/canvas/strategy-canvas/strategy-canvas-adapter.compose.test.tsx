/**
 *
 * The payload carries the document only; the adapter projects it from IR and a
 * trajectory that names a wired slot marks the slot node simulated. There is
 * no YAML branch: the Canvas renders the applied IR document (P6 condition 2).
 */
import { describe, expect, it } from 'vitest';
import type { MutableRefObject } from 'react';

import type { WorkflowDocument } from '@42ch/nexus-contracts';

import type { StrategyCanvasAdapterContext } from './strategy-canvas-adapter';
import { createStrategyCanvasAdapter } from './strategy-canvas-adapter';
import { candidateDocument, simulationTrajectory } from '@/test/simulation-fixture';

function ctxRef(overrides: Partial<StrategyCanvasAdapterContext> = {}) {
  return {
    current: {
      form: { label: '', description: '', nextTarget: '', promptBody: '' },
      onChange: () => {},
      setActiveSection: () => {},
      selectedNode: null,
      parsed: undefined,
      danglingTargets: [],
      ...overrides,
    },
  } as MutableRefObject<StrategyCanvasAdapterContext>;
}

function composedCandidate(): WorkflowDocument {
  return {
    ...candidateDocument(),
    nodes: [
      ...candidateDocument().nodes,
      {
        id: 'collect-sources',
        kind: 'pattern_slot',
        effect_class: 'external',
        pattern: { registry_id: 'prompt-step', version: '1.0.0' },
      },
    ],
    edges: [
      ...candidateDocument().edges,
      { id: 'draft->collect-sources', from: 'draft', to: 'collect-sources', kind: 'linear' },
    ],
  };
}

describe('createStrategyCanvasAdapter — compose branch', () => {
  it('projects from IR when a candidate document is present and marks a trajectory-named slot simulated', () => {
    const adapter = createStrategyCanvasAdapter(ctxRef());
    const document = composedCandidate();
    const { nodes } = adapter.projectGraph({
      revision: 0,
      activeSession: null,
      trajectory: simulationTrajectory({ nodes_visited: ['collect-sources', 'done', 'draft', 'routing'] }),
      document,
    });

    const slot = nodes.find((node) => node.id === 'collect-sources');
    expect(slot?.type).toBe('strategy-pattern-slot');
    expect(slot?.data.simulated).toBe(true);
    expect(nodes.find((node) => node.id === 'pause')?.data.simulated).toBeUndefined();
  });

  it('patches the expand toggle onto slot nodes only when compose handlers exist', () => {
    const toggled: string[] = [];
    const adapter = createStrategyCanvasAdapter(
      ctxRef({
        compose: {
          isComposing: false,
          onToggleSlotExpanded: (id) => toggled.push(id),
          onBindSlot: () => Promise.resolve(),
          onReplaceSubtree: () => Promise.resolve(),
          onUnnest: () => Promise.resolve(),
        },
      }),
    );
    const { nodes } = adapter.projectGraph({
      revision: 0,
      activeSession: null,
      trajectory: null,
      document: composedCandidate(),
    });
    const slot = nodes.find((node) => node.id === 'collect-sources');
    (slot?.data.onToggleExpand as () => void)();
    expect(toggled).toEqual(['collect-sources']);
  });

  it('projects the document it is given, with no YAML fallback to fall back to', () => {
    const adapter = createStrategyCanvasAdapter(ctxRef());
    const { nodes } = adapter.projectGraph({
      revision: 1,
      activeSession: null,
      trajectory: null,
      document: candidateDocument(),
    });
    expect(nodes.some((node) => node.type === 'strategy-pattern-slot')).toBe(false);
    expect(nodes.map((node) => node.id).sort()).toEqual(['done', 'draft', 'pause', 'routing']);
  });
});
