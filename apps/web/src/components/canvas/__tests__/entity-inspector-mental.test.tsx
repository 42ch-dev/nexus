/**
 * Entity inspector — modules.mental editing (v1.203 P2 Task 2, O1).
 *
 * Holder-capable kinds (`character` / `faction` / `organization`) get the
 * editable Mental State section: values seed from the stored bag, empty bags
 * populate-empty, unknown inner keys round-trip via raw-JSON fallback rows
 * (STOP condition — never silently dropped), client-side invalid JSON blocks
 * the write with an inline field error, and daemon 422s carrying the frozen
 * `modules.mental.<field>: <reason>` grammar map 1:1 onto the offending
 * field. Non-holder kinds keep the V1.164 PD-16 read-only rendering.
 */
import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

import { makeQueryClient } from '@/test/test-providers';
import { QueryClientProvider } from '@tanstack/react-query';
import { ClientProvider } from '@/lib/client-context';
import { ToastProvider, Toaster } from '@/lib/use-toast';
import { NexusClientError, type NexusClient } from '@/lib/nexus';
import type { WorldKbEntityPatch, WorldKbEntityProjection } from '@42ch/nexus-contracts';

import { EntityInspector } from '../world-kb/entity-inspector';
import type { WorldKbNodeData } from '../world-kb/types';

const node: WorldKbNodeData = {
  worldId: 'w-1',
  keyBlockId: 'kb-bo',
  entityKind: 'character',
  name: 'Bo',
  lifecycle: 'confirmed',
  version: 1,
  sourceAnchorCount: 0,
  computable: false,
};

/** Character with a populated `modules.mental` bag (mirrors the Task 2 fixture). */
const entityWithMental: WorldKbEntityProjection = {
  key_block_id: 'kb-bo',
  world_id: 'w-1',
  block_type: 'character',
  canonical_name: 'Bo',
  status: 'confirmed',
  version: 1,
  modules: {
    mental: {
      identity: { role: 'harbor_master' },
      beliefs: { ref: 'kb_bo_beliefs', count: 12 },
      attention: { target: 'kb_tw_dawn_dock', modality: 'visual' },
      goals: [{ goal: 'clear the dawn berths', status: 'active' }],
      emotions: [{ emotion: 'alert', intensity: 0.6 }],
      norms: ['greet arriving captains'],
      constraints: ['cannot waive dockside law'],
    },
  },
};

/** Character without a `modules` bag at all (mirrors the Task 2 fixture). */
const entityWithoutMental: WorldKbEntityProjection = {
  key_block_id: 'kb-ana',
  world_id: 'w-1',
  block_type: 'character',
  canonical_name: 'Ana',
  status: 'confirmed',
  version: 1,
};

/** Non-holder kind with a populated bag — PD-16 read-only path. */
const sceneEntityWithMental: WorldKbEntityProjection = {
  key_block_id: 'kb-dock',
  world_id: 'w-1',
  block_type: 'scene',
  canonical_name: 'Dawn Dock',
  status: 'confirmed',
  version: 2,
  modules: {
    mental: {
      goals: [{ goal: 'keep the cranes turning', status: 'active' }],
    },
  },
};

/** Non-holder kind without any stored modules. */
const sceneEntityWithoutModules: WorldKbEntityProjection = {
  key_block_id: 'kb-empty-scene',
  world_id: 'w-1',
  block_type: 'scene',
  canonical_name: 'Empty Scene',
  status: 'confirmed',
  version: 1,
};

function makeClient(overrides: Partial<NexusClient> = {}): NexusClient {
  return {
    getWorldKbGraph: vi.fn(),
    getWorldKbCandidates: vi.fn(),
    worldKbPatchEntity: vi.fn().mockResolvedValue({}),
    worldKbPromoteCandidate: vi.fn(),
    ...overrides,
  } as unknown as NexusClient;
}

function renderWith(client: NexusClient, ui: React.ReactElement) {
  return render(
    <QueryClientProvider client={makeQueryClient()}>
      <ToastProvider>
        <ClientProvider client={client}>{ui}</ClientProvider>
        <Toaster />
      </ToastProvider>
    </QueryClientProvider>,
  );
}

/** Mock-call capture without coupling to the `vi` helper implementation type. */
interface EntityPatchRequest {
  entity_id: string;
  expected_version: number;
  patch: WorldKbEntityPatch;
}

interface MockCallLog {
  mock: { calls: Array<[string, EntityPatchRequest]> };
}

function callsOf(fn: unknown): Array<[string, EntityPatchRequest]> {
  return (fn as MockCallLog).mock.calls;
}

describe('EntityInspector — modules.mental editing (v1.203 P2 O1)', () => {
  it('renders the editable mental section seeded from the stored bag on a holder kind', () => {
    renderWith(
      makeClient(),
      <EntityInspector worldId="w-1" node={node} entity={entityWithMental} onConflict={vi.fn()} />,
    );

    const section = screen.getByTestId('mental-state-section');
    expect(section).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Mental State' })).toHaveAttribute(
      'aria-expanded',
      'true',
    );

    // Locked nine-field labels render; seeded values sit in editable JSON inputs.
    expect(within(section).getByLabelText('Beliefs')).toHaveDisplayValue(/kb_bo_beliefs/);
    expect(within(section).getByLabelText('Goals')).toHaveDisplayValue(/clear the dawn berths/);
    expect(within(section).getByLabelText('Emotions')).toHaveDisplayValue(/"alert"/);
    expect(within(section).getByLabelText('Identity')).toHaveDisplayValue(/harbor_master/);
  });

  it('keeps the read-only JSON rendering for populated bags on non-holder kinds (PD-16)', () => {
    renderWith(
      makeClient(),
      <EntityInspector
        worldId="w-1"
        node={{ ...node, keyBlockId: 'kb-dock' }}
        entity={sceneEntityWithMental}
        onConflict={vi.fn()}
      />,
    );

    const section = screen.getByTestId('mental-state-section');
    expect(within(section).getByText('Goals')).toBeInTheDocument();
    expect(within(section).getByText(/keep the cranes turning/)).toBeInTheDocument();
    // No input controls — read-only, no create affordance.
    expect(within(section).queryByRole('textbox')).not.toBeInTheDocument();
  });

  it('omits the mental section on non-holder kinds when modules.mental is absent (PD-16)', () => {
    renderWith(
      makeClient(),
      <EntityInspector
        worldId="w-1"
        node={{ ...node, keyBlockId: 'kb-empty-scene' }}
        entity={sceneEntityWithoutModules}
        onConflict={vi.fn()}
      />,
    );

    expect(screen.queryByTestId('mental-state-section')).not.toBeInTheDocument();
  });

  it('renders the editable section empty for holder kinds when modules.mental is absent (populate-empty)', () => {
    renderWith(
      makeClient(),
      <EntityInspector worldId="w-1" node={node} entity={entityWithoutMental} onConflict={vi.fn()} />,
    );

    const section = screen.getByTestId('mental-state-section');
    expect(within(section).getByLabelText('Goals')).toHaveDisplayValue('');
    expect(within(section).getByLabelText('Beliefs')).toHaveDisplayValue('');
  });

  it('renders the editable section empty for null / non-object modules.mental (defensive degradation)', () => {
    const nullMental: WorldKbEntityProjection = {
      ...entityWithoutMental,
      modules: { mental: null },
    };
    const stringMental: WorldKbEntityProjection = {
      ...entityWithoutMental,
      modules: { mental: 'not-an-object' },
    };
    const { rerender } = renderWith(
      makeClient(),
      <EntityInspector worldId="w-1" node={node} entity={nullMental} onConflict={vi.fn()} />,
    );
    expect(screen.getByTestId('mental-state-section')).toBeInTheDocument();

    rerender(
      <QueryClientProvider client={makeQueryClient()}>
        <ToastProvider>
          <ClientProvider client={makeClient()}>
            <EntityInspector worldId="w-1" node={node} entity={stringMental} onConflict={vi.fn()} />
          </ClientProvider>
          <Toaster />
        </ToastProvider>
      </QueryClientProvider>,
    );
    expect(screen.getByTestId('mental-state-section')).toBeInTheDocument();
  });

  it('edits a populated value and submits the complete first-level modules.mental value', async () => {
    const user = userEvent.setup();
    const client = makeClient();
    renderWith(
      client,
      <EntityInspector worldId="w-1" node={node} entity={entityWithMental} onConflict={vi.fn()} />,
    );

    fireEvent.change(screen.getByLabelText('Goals'), {
      target: { value: '[{"goal": "repaint the lighthouse", "status": "active"}]' },
    });

    await user.click(screen.getByRole('button', { name: /^Save$/i }));

    await waitFor(() => expect(client.worldKbPatchEntity).toHaveBeenCalled());
    const call = callsOf(client.worldKbPatchEntity)[0];
    expect(call[0]).toBe('w-1');
    expect(call[1]).toMatchObject({
      entity_id: 'kb-bo',
      expected_version: 1,
      patch: {
        modules: {
          // Whole-first-level value: untouched keys round-trip verbatim.
          mental: {
            identity: { role: 'harbor_master' },
            beliefs: { ref: 'kb_bo_beliefs', count: 12 },
            attention: { target: 'kb_tw_dawn_dock', modality: 'visual' },
            goals: [{ goal: 'repaint the lighthouse', status: 'active' }],
            emotions: [{ emotion: 'alert', intensity: 0.6 }],
            norms: ['greet arriving captains'],
            constraints: ['cannot waive dockside law'],
          },
        },
      },
    });
    // Governance is untouched: no `audience` key is emitted.
    expect(call[1].patch).not.toHaveProperty('audience');
  });

  it('populates an empty bag: writes only the edited keys and omits blanked ones', async () => {
    const user = userEvent.setup();
    const client = makeClient();
    renderWith(
      client,
      <EntityInspector worldId="w-1" node={node} entity={entityWithoutMental} onConflict={vi.fn()} />,
    );

    fireEvent.change(screen.getByLabelText('Identity'), {
      target: { value: '{"role": "night_watch"}' },
    });

    await user.click(screen.getByRole('button', { name: /^Save$/i }));

    await waitFor(() => expect(client.worldKbPatchEntity).toHaveBeenCalled());
    const call = callsOf(client.worldKbPatchEntity)[0];
    expect(call[1].patch.modules).toEqual({ mental: { identity: { role: 'night_watch' } } });
  });

  it('round-trips unknown inner keys via the raw-JSON fallback rows (STOP condition)', async () => {
    const user = userEvent.setup();
    const client = makeClient();
    const entityWithExtra: WorldKbEntityProjection = {
      ...entityWithoutMental,
      modules: {
        mental: {
          goals: [{ goal: 'mend the nets' }],
          // A deep freeform key the locked vocabulary cannot represent.
          custom_model_state: { nested: { deeply: ['a', 'b'] } },
        },
      },
    };
    renderWith(
      client,
      <EntityInspector worldId="w-1" node={node} entity={entityWithExtra} onConflict={vi.fn()} />,
    );

    const section = screen.getByTestId('mental-state-section');
    const extra = within(section).getByLabelText('custom_model_state');
    expect(extra).toHaveDisplayValue(/deeply/);

    // Dirty the form (edit the goals field) so the save path runs.
    fireEvent.change(within(section).getByLabelText('Goals'), {
      target: { value: '[{"goal":"mend the nets","status":"active"}]' },
    });
    await user.click(screen.getByRole('button', { name: /^Save$/i }));
    await waitFor(() => expect(client.worldKbPatchEntity).toHaveBeenCalled());
    const call = callsOf(client.worldKbPatchEntity)[0];
    // The unknown key survives the write — nothing is silently dropped.
    expect(
      (call[1].patch.modules!.mental as Record<string, unknown>).custom_model_state,
    ).toEqual({
      nested: { deeply: ['a', 'b'] },
    });
  });

  it('blocks the write and shows an inline field error when edited JSON is invalid', async () => {
    const user = userEvent.setup();
    const client = makeClient();
    renderWith(
      client,
      <EntityInspector worldId="w-1" node={node} entity={entityWithMental} onConflict={vi.fn()} />,
    );

    fireEvent.change(screen.getByLabelText('Goals'), { target: { value: 'not valid json' } });

    await user.click(screen.getByRole('button', { name: /^Save$/i }));

    const section = screen.getByTestId('mental-state-section');
    expect(within(section).getByText(/must be valid JSON/i)).toBeInTheDocument();
    expect(client.worldKbPatchEntity).not.toHaveBeenCalled();
  });

  it('maps a daemon 422 modules.mental.<field> entry onto the offending field', async () => {
    const user = userEvent.setup();
    const client = makeClient({
      worldKbPatchEntity: vi.fn().mockRejectedValue(
        new NexusClientError(422, 'world_kb_validation_failed', 'validation failed', {
          validation_summary: {
            errors: ['modules.mental.goals: must be an object', 'modules.mental: must be an object'],
          },
        }),
      ),
    });
    renderWith(
      client,
      <EntityInspector worldId="w-1" node={node} entity={entityWithMental} onConflict={vi.fn()} />,
    );

    // Any change to make the form dirty — replace norms with valid JSON.
    fireEvent.change(screen.getByLabelText('Norms'), { target: { value: '[]' } });

    await user.click(screen.getByRole('button', { name: /^Save$/i }));

    const section = await screen.findByTestId('mental-state-section');
    // The field-prefixed reason lands on the Goals field…
    expect(within(section).getByText('must be an object')).toBeInTheDocument();
    // …while the whole-dialect entry stays section-level, verbatim.
    expect(screen.getByText('modules.mental: must be an object')).toBeInTheDocument();
  });

  it('clears a populated field by blanking it (key omitted from the written value)', async () => {
    const user = userEvent.setup();
    const client = makeClient();
    renderWith(
      client,
      <EntityInspector worldId="w-1" node={node} entity={entityWithMental} onConflict={vi.fn()} />,
    );

    await user.clear(screen.getByLabelText('Beliefs'));
    await user.click(screen.getByRole('button', { name: /^Save$/i }));

    await waitFor(() => expect(client.worldKbPatchEntity).toHaveBeenCalled());
    const call = callsOf(client.worldKbPatchEntity)[0];
    expect(call[1].patch.modules!.mental).not.toHaveProperty('beliefs');
    expect(
      (call[1].patch.modules!.mental as Record<string, unknown>).identity,
    ).toEqual({ role: 'harbor_master' });
  });

  it('collapses and re-expands the mental section via the header toggle', async () => {
    const user = userEvent.setup();
    renderWith(
      makeClient(),
      <EntityInspector worldId="w-1" node={node} entity={entityWithMental} onConflict={vi.fn()} />,
    );

    const toggle = screen.getByRole('button', { name: 'Mental State' });
    const section = screen.getByTestId('mental-state-section');
    expect(toggle).toHaveAttribute('aria-expanded', 'true');
    expect(within(section).getByLabelText('Beliefs')).toBeInTheDocument();

    await user.click(toggle);
    expect(toggle).toHaveAttribute('aria-expanded', 'false');
    expect(within(section).queryByLabelText('Beliefs')).not.toBeInTheDocument();

    await user.click(toggle);
    expect(within(section).getByLabelText('Beliefs')).toBeInTheDocument();
  });

  it('resets the collapse state to expanded when switching to another entity with modules.mental (S-3)', async () => {
    const user = userEvent.setup();
    const client = makeClient();
    const secondEntity: WorldKbEntityProjection = {
      ...entityWithMental,
      key_block_id: 'kb-ana',
      canonical_name: 'Ana',
      modules: {
        mental: { goals: [{ goal: 'keep the docks quiet', status: 'active' }] },
      },
    };
    const { rerender } = renderWith(
      client,
      <EntityInspector worldId="w-1" node={node} entity={entityWithMental} onConflict={vi.fn()} />,
    );

    await user.click(screen.getByRole('button', { name: 'Mental State' }));
    expect(screen.getByRole('button', { name: 'Mental State' })).toHaveAttribute(
      'aria-expanded',
      'false',
    );

    // (rerender replaces the whole root element, so re-wrap the providers.)
    rerender(
      <QueryClientProvider client={makeQueryClient()}>
        <ToastProvider>
          <ClientProvider client={client}>
            <EntityInspector worldId="w-1" node={node} entity={secondEntity} onConflict={vi.fn()} />
          </ClientProvider>
          <Toaster />
        </ToastProvider>
      </QueryClientProvider>,
    );
    expect(screen.getByRole('button', { name: 'Mental State' })).toHaveAttribute(
      'aria-expanded',
      'true',
    );
    const section = screen.getByTestId('mental-state-section');
    expect(within(section).getByLabelText('Goals')).toHaveDisplayValue(/keep the docks quiet/);
  });

  it('read-back: a saved update re-seeds the form with the updated stored values', async () => {
    const user = userEvent.setup();
    const client = makeClient();
    const updatedEntity: WorldKbEntityProjection = {
      ...entityWithMental,
      version: 2,
      modules: {
        mental: { goals: [{ goal: 'keep the docks quiet', status: 'active' }] },
      },
    };
    const { rerender } = renderWith(
      client,
      <EntityInspector worldId="w-1" node={node} entity={entityWithMental} onConflict={vi.fn()} />,
    );

    fireEvent.change(screen.getByLabelText('Goals'), {
      target: { value: '[{"goal":"keep the docks quiet","status":"active"}]' },
    });
    await user.click(screen.getByRole('button', { name: /^Save$/i }));
    await waitFor(() => expect(client.worldKbPatchEntity).toHaveBeenCalled());

    // The canvas re-reads the entity and re-renders the inspector (new version).
    rerender(
      <QueryClientProvider client={makeQueryClient()}>
        <ToastProvider>
          <ClientProvider client={client}>
            <EntityInspector
              worldId="w-1"
              node={{ ...node, version: 2 }}
              entity={updatedEntity}
              onConflict={vi.fn()}
              reseedSignal={1}
            />
          </ClientProvider>
          <Toaster />
        </ToastProvider>
      </QueryClientProvider>,
    );

    const section = screen.getByTestId('mental-state-section');
    expect(within(section).getByLabelText('Goals')).toHaveDisplayValue(/keep the docks quiet/);
    // Keys absent from the updated stored value no longer appear.
    expect(within(section).getByLabelText('Beliefs')).toHaveDisplayValue('');
  });
});
