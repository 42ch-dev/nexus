/**
 * Strategy canvas adapter — IR projection (v239-p3-canvas-compose-ui T3b/T5).
 *
 * The payload carries the document only; the adapter projects it from IR and a
 * trajectory that names a wired slot marks the slot node simulated. There is
 * no YAML branch: the Canvas renders the applied IR document (P6 condition 2).
 */
import { describe, expect, it, vi } from 'vitest';
import { fireEvent, screen, within } from '@testing-library/react';
import type { MutableRefObject } from 'react';

import type { WorkflowDocument } from '@42ch/nexus-contracts';

import type { NexusClient } from '@/lib/nexus';
import type { PresetState } from '@/lib/canvas/preset-yaml';
import { renderInApp } from '@/test/test-providers';
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
      activeSession: null,
      trajectory: null,
      document: composedCandidate(),
    });
    const slot = nodes.find((node) => node.id === 'collect-sources');
    (slot?.data.onToggleExpand as () => void)();
    expect(toggled).toEqual(['collect-sources']);
  });

  it('disables the YAML transition patch while a candidate document is the authority', () => {
    const client = { strategyPatchTransition: vi.fn() } as unknown as NexusClient;
    const adapter = createStrategyCanvasAdapter(
      ctxRef({
        selectedState: { id: 'draft', next: 'done' } as PresetState,
        form: { label: '', description: '', nextTarget: 'revise', promptBody: '' },
        compose: {
          isComposing: false,
          onToggleSlotExpanded: () => {},
          onBindSlot: () => Promise.resolve(),
          onReplaceSubtree: () => Promise.resolve(),
          onUnnest: () => Promise.resolve(),
        },
      }),
    );

    renderInApp(<>{adapter.renderInspector?.(null as never)}</>, { client });

    const section = screen.getByRole('region', { name: 'Transition inspector' });
    // Kept visible with its reason, not silently hidden.
    expect(within(section).getByTestId('transition-structural-disabled')).toBeInTheDocument();
    const save = within(section).getByRole('button', { name: 'Save' });
    expect(save).toBeDisabled();
    fireEvent.click(save);
    expect(client.strategyPatchTransition).not.toHaveBeenCalled();
  });

  it('disables the YAML state patch while a candidate document is the authority', () => {
    const client = { strategyPatchState: vi.fn() } as unknown as NexusClient;
    const adapter = createStrategyCanvasAdapter(
      ctxRef({
        selectedState: { id: 'draft', next: 'done' } as PresetState,
        form: { label: 'Draft (renamed)', description: '', nextTarget: 'done', promptBody: '' },
        compose: {
          isComposing: false,
          onToggleSlotExpanded: () => {},
          onBindSlot: () => Promise.resolve(),
          onReplaceSubtree: () => Promise.resolve(),
          onUnnest: () => Promise.resolve(),
        },
      }),
    );

    renderInApp(<>{adapter.renderInspector?.(null as never)}</>, { client });

    const section = screen.getByRole('region', { name: 'State inspector' });
    // Kept visible with its reason, not silently hidden.
    expect(within(section).getByTestId('state-structural-disabled')).toBeInTheDocument();
    const save = within(section).getByRole('button', { name: 'Save' });
    expect(save).toBeDisabled();
    fireEvent.click(save);
    expect(client.strategyPatchState).not.toHaveBeenCalled();
  });

  it('keeps the prompt editor writable on a compose candidate', () => {
    const client = { strategyPatchPromptTemplate: vi.fn() } as unknown as NexusClient;
    const adapter = createStrategyCanvasAdapter(
      ctxRef({
        appliedView: false,
        selectedState: {
          id: 'draft',
          next: 'done',
          context_update: { template_file: 'prompts/draft.md' },
        } as PresetState,
        promptTemplateRef: 'prompts/draft.md',
        form: { label: '', description: '', nextTarget: '', promptBody: '# edited prompt' },
        compose: {
          isComposing: false,
          onToggleSlotExpanded: () => {},
          onBindSlot: () => Promise.resolve(),
          onReplaceSubtree: () => Promise.resolve(),
          onUnnest: () => Promise.resolve(),
        },
      }),
    );

    renderInApp(<>{adapter.renderInspector?.(null as never)}</>, { client });

    // A prompt body on the candidate is content its author still owns; only the
    // read-only applied view disables it.
    const prompt = screen.getByRole('region', { name: 'Prompt inspector' });
    expect(within(prompt).queryByTestId('prompt-structural-disabled')).toBeNull();
    expect(within(prompt).getByRole('textbox')).not.toBeDisabled();
    expect(within(prompt).getByRole('button', { name: 'Save' })).not.toBeDisabled();
  });

  it('keeps the applied view read-only: every YAML editor refuses its write and names Compose', () => {
    const client = {
      strategyPatchState: vi.fn(),
      strategyPatchTransition: vi.fn(),
      strategyPatchPromptTemplate: vi.fn(),
    } as unknown as NexusClient;
    const adapter = createStrategyCanvasAdapter(
      ctxRef({
        appliedView: true,
        selectedState: {
          id: 'draft',
          next: 'done',
          context_update: { template_file: 'prompts/draft.md' },
        } as PresetState,
        promptTemplateRef: 'prompts/draft.md',
        form: {
          label: 'Draft (renamed)',
          description: '',
          nextTarget: 'revise',
          promptBody: '# edited prompt',
        },
      }),
    );

    renderInApp(<>{adapter.renderInspector?.(null as never)}</>, { client });

    // The applied document is the running strategy. Rendering it must not give
    // any control a way to write the preset file behind it — the very defect
    // this covers is a canvas that displayed one revision and patched another.
    const reason = /author the change in Compose/;
    const state = screen.getByRole('region', { name: 'State inspector' });
    expect(within(state).getByTestId('state-structural-disabled')).toHaveTextContent(reason);
    const stateSave = within(state).getByRole('button', { name: 'Save' });
    expect(stateSave).toBeDisabled();
    fireEvent.click(stateSave);

    const transition = screen.getByRole('region', { name: 'Transition inspector' });
    expect(within(transition).getByTestId('transition-structural-disabled')).toHaveTextContent(reason);
    const transitionSave = within(transition).getByRole('button', { name: 'Save' });
    expect(transitionSave).toBeDisabled();
    fireEvent.click(transitionSave);

    const prompt = screen.getByRole('region', { name: 'Prompt inspector' });
    expect(within(prompt).getByTestId('prompt-structural-disabled')).toHaveTextContent(reason);
    expect(within(prompt).getByRole('textbox')).toBeDisabled();
    const promptSave = within(prompt).getByRole('button', { name: 'Save' });
    expect(promptSave).toBeDisabled();
    fireEvent.click(promptSave);

    expect(client.strategyPatchState).not.toHaveBeenCalled();
    expect(client.strategyPatchTransition).not.toHaveBeenCalled();
    expect(client.strategyPatchPromptTemplate).not.toHaveBeenCalled();
  });

  it('merges a local draft edge into the projection', () => {
    const draft = {
      id: 'draft-edge',
      source: 'draft',
      target: 'done',
      type: 'strategy-edge',
      data: { transitionKind: 'next' as const, isDraft: true },
    };
    const adapter = createStrategyCanvasAdapter(ctxRef({ localEdges: [draft] }));
    const payload = {
      revision: 0,
      activeSession: null,
      trajectory: null,
      document: composedCandidate(),
    };

    // A draft is the author's uncommitted transition, not part of the document.
    expect(adapter.projectGraph(payload).edges.map((e) => e.id)).toContain('draft-edge');
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
