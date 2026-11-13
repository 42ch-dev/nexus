/**
 * Strategy canvas adapter — compose branch (v239-p3-canvas-compose-ui T3b/T5).
 *
 * With a candidate document in the payload the adapter projects from IR; a
 * trajectory that names a wired slot marks the slot node simulated — the
 * simulation overlay names the slot. With the flag off (no `document`
 * payload) the YAML projection runs untouched.
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
      presetId: 'user/demo',
      form: { label: '', description: '', nextTarget: '', promptBody: '' },
      saveTriggers: { state: 0, transition: 0, prompt: 0 },
      saveStatuses: {},
      workingRevisionRef: { current: 0 },
      handleConflict: () => {},
      onChange: () => {},
      onSaveStatus: () => {},
      setActiveSection: () => {},
      selectedState: undefined,
      promptTemplateRef: undefined,
      selectedNode: null,
      parsed: undefined,
      onUseCurrent: () => {},
      onReapply: () => {},
      onDismiss: () => {},
      danglingTargets: [],
      localEdges: [],
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
      parsed: { manifest: { preset: { id: 'user/demo' }, states: [] }, problems: [] },
      activeSession: null,
      trajectory: simulationTrajectory({ nodes_visited: ['collect-sources', 'done', 'draft', 'routing'] }),
      document,
    });

    const slot = nodes.find((node) => node.id === 'collect-sources');
    expect(slot?.type).toBe('strategy-pattern-slot');
    expect(slot?.data.simulated).toBe(true);
    // A node the walk did not visit stays unmarked.
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
      parsed: { manifest: { preset: { id: 'user/demo' }, states: [] }, problems: [] },
      activeSession: null,
      trajectory: null,
      document: composedCandidate(),
    });
    const slot = nodes.find((node) => node.id === 'collect-sources');
    (slot?.data.onToggleExpand as () => void)();
    expect(toggled).toEqual(['collect-sources']);
  });

  it('keeps the YAML projection when no candidate document is present (flag off)', () => {
    const adapter = createStrategyCanvasAdapter(ctxRef());
    const { nodes } = adapter.projectGraph({
      revision: 1,
      parsed: {
        manifest: {
          preset: { id: 'user/demo' },
          states: [{ id: 'draft', next: 'done', terminal: false }],
        },
        problems: [],
      },
      activeSession: null,
      trajectory: null,
    });
    expect(nodes.some((node) => node.type === 'strategy-pattern-slot')).toBe(false);
    expect(nodes.map((node) => node.id)).toContain('draft');
  });
});
