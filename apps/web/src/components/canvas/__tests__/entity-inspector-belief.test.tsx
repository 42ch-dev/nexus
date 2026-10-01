/**
 * Entity inspector — modules.belief editing (v1.203 P2 Task 2, O2).
 *
 * Holder-capable kinds get the editable "Belief Propositions" section beside
 * the mental editor: rows seed from the stored array, authors can add /
 * edit / remove rows (holder, proposition, order, truth, access,
 * representation, content_type, source, context), and submit writes the
 * complete first-level `modules.belief` value (whole-value upsert,
 * AR-4/PD-12 — clearing every row writes `[]`, whose read semantics are
 * "absent"). Closed-label fields use selects mirroring the daemon's
 * `validate_closed_belief_labels` spaces; daemon 422s carrying
 * `modules.belief.<index>.<field>: <reason>` map onto the offending row
 * field. Non-holder kinds render the stored rows read-only (PD-16 parity).
 */
import { render, screen, waitFor, within, fireEvent } from '@testing-library/react';
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

/** Character with two stored belief propositions. */
const entityWithBeliefs: WorldKbEntityProjection = {
  key_block_id: 'kb-bo',
  world_id: 'w-1',
  block_type: 'character',
  canonical_name: 'Bo',
  status: 'confirmed',
  version: 1,
  modules: {
    belief: [
      {
        holder: 'chr_bo',
        proposition: 'the dawn ferry is late',
        order: 0,
        truth: 'True',
        access: 'Public',
        representation: 'Explicit',
        content_type: 'Action/Event',
        source: 'Perception',
        context: 'Neutral',
      },
      {
        holder: 'chr_mara',
        proposition: 'Bo suspects the tide tables',
        order: 1,
        truth: 'Unknown',
        access: 'Private',
      },
    ],
  },
};

/** Character without any stored modules. */
const entityWithoutBeliefs: WorldKbEntityProjection = {
  key_block_id: 'kb-ana',
  world_id: 'w-1',
  block_type: 'character',
  canonical_name: 'Ana',
  status: 'confirmed',
  version: 1,
};

/** Non-holder kind with populated beliefs — PD-16 read-only path. */
const sceneEntityWithBeliefs: WorldKbEntityProjection = {
  key_block_id: 'kb-dock',
  world_id: 'w-1',
  block_type: 'scene',
  canonical_name: 'Dawn Dock',
  status: 'confirmed',
  version: 2,
  modules: {
    belief: [{ holder: 'world', proposition: 'the dock tolls doubled', order: 0 }],
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

describe('EntityInspector — modules.belief editing (v1.203 P2 O2)', () => {
  it('renders the belief section with rows seeded from the stored array on a holder kind', () => {
    renderWith(
      makeClient(),
      <EntityInspector worldId="w-1" node={node} entity={entityWithBeliefs} onConflict={vi.fn()} />,
    );

    const section = screen.getByTestId('belief-section');
    expect(within(section).getByRole('button', { name: 'Belief Propositions' })).toHaveAttribute(
      'aria-expanded',
      'true',
    );

    // Rows seed independently (duplicate labels → within(row) scoping).
    const rows = within(section).getAllByRole('group');
    expect(rows).toHaveLength(2);
    // Row 1 seeds with every populated field.
    expect(within(rows[0]).getByLabelText('Holder')).toHaveDisplayValue('chr_bo');
    expect(within(rows[0]).getByLabelText('Proposition')).toHaveDisplayValue(
      'the dawn ferry is late',
    );
    expect(within(rows[0]).getByLabelText('Order')).toHaveDisplayValue('0');
    expect(within(rows[0]).getByLabelText('Truth Status')).toHaveDisplayValue('True');
    expect(within(rows[0]).getByLabelText('Knowledge Access')).toHaveDisplayValue('Public');
    // Row 2 seeds independently.
    expect(within(rows[1]).getByLabelText('Holder')).toHaveDisplayValue('chr_mara');
    expect(within(rows[1]).getByLabelText('Truth Status')).toHaveDisplayValue('Unknown');
    // Closed-label selects only offer the handbook spaces (plus empty).
    expect(within(rows[1]).getByLabelText('Representation')).toHaveValue('');
  });

  it('renders read-only belief rows for populated non-holder kinds (PD-16 parity)', () => {
    renderWith(
      makeClient(),
      <EntityInspector
        worldId="w-1"
        node={{ ...node, keyBlockId: 'kb-dock' }}
        entity={sceneEntityWithBeliefs}
        onConflict={vi.fn()}
      />,
    );

    const section = screen.getByTestId('belief-section');
    expect(within(section).getByText('Belief 1')).toBeInTheDocument();
    expect(within(section).getByText(/the dock tolls doubled/)).toBeInTheDocument();
    expect(within(section).queryByRole('textbox')).not.toBeInTheDocument();
    expect(within(section).queryByRole('button', { name: 'Add belief' })).not.toBeInTheDocument();
  });

  it('omits the belief section on non-holder kinds when nothing is stored', () => {
    renderWith(
      makeClient(),
      <EntityInspector
        worldId="w-1"
        node={{ ...node, keyBlockId: 'kb-empty-scene' }}
        entity={sceneEntityWithoutModules}
        onConflict={vi.fn()}
      />,
    );

    expect(screen.queryByTestId('belief-section')).not.toBeInTheDocument();
  });

  it('offers populate-empty on holder kinds: an empty bag renders the section with only the add affordance', () => {
    renderWith(
      makeClient(),
      <EntityInspector worldId="w-1" node={node} entity={entityWithoutBeliefs} onConflict={vi.fn()} />,
    );

    const section = screen.getByTestId('belief-section');
    expect(within(section).getByRole('button', { name: 'Add belief' })).toBeInTheDocument();
    expect(within(section).queryByRole('group')).not.toBeInTheDocument();
  });

  it('edits a stored row and submits the complete first-level modules.belief value', async () => {
    const user = userEvent.setup();
    const client = makeClient();
    renderWith(
      client,
      <EntityInspector worldId="w-1" node={node} entity={entityWithBeliefs} onConflict={vi.fn()} />,
    );

    const proposition = screen.getByDisplayValue('the dawn ferry is late');
    await user.clear(proposition);
    await user.type(proposition, 'the dawn ferry is cancelled');

    await user.click(screen.getByRole('button', { name: /^Save$/i }));

    await waitFor(() => expect(client.worldKbPatchEntity).toHaveBeenCalled());
    const call = callsOf(client.worldKbPatchEntity)[0];
    expect(call[0]).toBe('w-1');
    expect(call[1]).toMatchObject({
      entity_id: 'kb-bo',
      expected_version: 1,
      patch: {
        modules: {
          belief: [
            {
              holder: 'chr_bo',
              proposition: 'the dawn ferry is cancelled',
              order: 0,
              truth: 'True',
              access: 'Public',
              representation: 'Explicit',
              content_type: 'Action/Event',
              source: 'Perception',
              context: 'Neutral',
            },
            {
              holder: 'chr_mara',
              proposition: 'Bo suspects the tide tables',
              order: 1,
              truth: 'Unknown',
              access: 'Private',
            },
          ],
        },
      },
    });
    // Governance untouched — no `audience` key is emitted.
    expect(call[1].patch).not.toHaveProperty('audience');
    // Whole-value per-dialect gating: mental was not edited, so it is absent.
    expect(call[1].patch.modules).not.toHaveProperty('mental');
  });

  it('adds a new row and submits it alongside the untouched rows', async () => {
    const user = userEvent.setup();
    const client = makeClient();
    renderWith(
      client,
      <EntityInspector worldId="w-1" node={node} entity={entityWithBeliefs} onConflict={vi.fn()} />,
    );

    await user.click(screen.getByRole('button', { name: 'Add belief' }));
    const section = screen.getByTestId('belief-section');
    const rows = within(section).getAllByRole('group');
    expect(rows).toHaveLength(3);

    await user.type(within(rows[2]).getByLabelText('Holder'), 'chr_bo');
    await user.type(within(rows[2]).getByLabelText('Proposition'), 'the tide tables are wrong');
    await user.type(within(rows[2]).getByLabelText('Order'), '2');
    await user.selectOptions(within(rows[2]).getByLabelText('Truth Status'), 'False');

    await user.click(screen.getByRole('button', { name: /^Save$/i }));

    await waitFor(() => expect(client.worldKbPatchEntity).toHaveBeenCalled());
    const belief = callsOf(client.worldKbPatchEntity)[0][1].patch!.modules!.belief as Array<
      Record<string, unknown>
    >;
    expect(belief).toHaveLength(3);
    expect(belief[2]).toEqual({
      holder: 'chr_bo',
      proposition: 'the tide tables are wrong',
      order: 2,
      truth: 'False',
    });
  });

  it('removes a row: the submitted array drops it', async () => {
    const user = userEvent.setup();
    const client = makeClient();
    renderWith(
      client,
      <EntityInspector worldId="w-1" node={node} entity={entityWithBeliefs} onConflict={vi.fn()} />,
    );

    await user.click(screen.getByRole('button', { name: 'Remove belief 2' }));

    await user.click(screen.getByRole('button', { name: /^Save$/i }));

    await waitFor(() => expect(client.worldKbPatchEntity).toHaveBeenCalled());
    const belief = callsOf(client.worldKbPatchEntity)[0][1].patch!.modules!.belief as unknown[];
    expect(belief).toHaveLength(1);
    expect(belief[0]).toMatchObject({ holder: 'chr_bo' });
  });

  it('clear-to-empty: removing every row writes modules.belief: [] (absent on read-back)', async () => {
    const user = userEvent.setup();
    const client = makeClient();
    renderWith(
      client,
      <EntityInspector worldId="w-1" node={node} entity={entityWithBeliefs} onConflict={vi.fn()} />,
    );

    await user.click(screen.getByRole('button', { name: 'Remove belief 2' }));
    await user.click(screen.getByRole('button', { name: 'Remove belief 1' }));

    await user.click(screen.getByRole('button', { name: /^Save$/i }));

    await waitFor(() => expect(client.worldKbPatchEntity).toHaveBeenCalled());
    expect(callsOf(client.worldKbPatchEntity)[0][1].patch!.modules!.belief).toEqual([]);
  });

  it('read-back: a cleared-to-empty bag renders the section absent after re-read', async () => {
    const user = userEvent.setup();
    const client = makeClient();
    const clearedEntity: WorldKbEntityProjection = {
      ...entityWithBeliefs,
      version: 2,
      modules: { belief: [] },
    };
    const { rerender } = renderWith(
      client,
      <EntityInspector worldId="w-1" node={node} entity={entityWithBeliefs} onConflict={vi.fn()} />,
    );

    await user.click(screen.getByRole('button', { name: 'Remove belief 2' }));
    await user.click(screen.getByRole('button', { name: 'Remove belief 1' }));
    await user.click(screen.getByRole('button', { name: /^Save$/i }));
    await waitFor(() => expect(client.worldKbPatchEntity).toHaveBeenCalled());

    rerender(
      <QueryClientProvider client={makeQueryClient()}>
        <ToastProvider>
          <ClientProvider client={client}>
            <EntityInspector
              worldId="w-1"
              node={{ ...node, version: 2 }}
              entity={clearedEntity}
              onConflict={vi.fn()}
            />
          </ClientProvider>
          <Toaster />
        </ToastProvider>
      </QueryClientProvider>,
    );

    // Empty container = "absent": the editable section still offers
    // populate-empty on holder kinds, but no rows render.
    const section = screen.getByTestId('belief-section');
    expect(within(section).queryByRole('group')).not.toBeInTheDocument();
  });

  it('blocks the write when a row order is not a whole number', async () => {
    const user = userEvent.setup();
    const client = makeClient();
    renderWith(
      client,
      <EntityInspector worldId="w-1" node={node} entity={entityWithBeliefs} onConflict={vi.fn()} />,
    );

    const section = screen.getByTestId('belief-section');
    const order = within(section).getAllByLabelText('Order')[0];
    await user.clear(order);
    await user.type(order, '1.5');

    await user.click(screen.getByRole('button', { name: /^Save$/i }));

    expect(await screen.findByText(/must be a whole number/i)).toBeInTheDocument();
    expect(client.worldKbPatchEntity).not.toHaveBeenCalled();
  });

  it('maps a daemon 422 modules.belief.<index>.<field> entry onto the offending row field', async () => {
    const user = userEvent.setup();
    const client = makeClient({
      worldKbPatchEntity: vi.fn().mockRejectedValue(
        new NexusClientError(422, 'world_kb_validation_failed', 'validation failed', {
          validation_summary: {
            errors: [
              'modules.belief.1.truth: must be one of the handbook closed labels (got "Maybe")',
            ],
          },
        }),
      ),
    });
    renderWith(
      client,
      <EntityInspector worldId="w-1" node={node} entity={entityWithBeliefs} onConflict={vi.fn()} />,
    );

    // Make the form dirty (second row proposition edit).
    const proposition = screen.getByDisplayValue('Bo suspects the tide tables');
    await user.type(proposition, ' today');

    await user.click(screen.getByRole('button', { name: /^Save$/i }));

    const section = await screen.findByTestId('belief-section');
    const rows = within(section).getAllByRole('group');
    expect(
      within(rows[1]).getByText(/must be one of the handbook closed labels \(got "Maybe"\)/),
    ).toBeInTheDocument();
  });

  it('leaves non-`modules.` 422 entries at section level verbatim', async () => {
    const user = userEvent.setup();
    const client = makeClient({
      worldKbPatchEntity: vi.fn().mockRejectedValue(
        new NexusClientError(422, 'world_kb_validation_failed', 'validation failed', {
          validation_summary: { errors: ['title: must not be empty'] },
        }),
      ),
    });
    renderWith(
      client,
      <EntityInspector worldId="w-1" node={node} entity={entityWithBeliefs} onConflict={vi.fn()} />,
    );

    const proposition = screen.getByDisplayValue('the dawn ferry is late');
    await user.type(proposition, '!');

    await user.click(screen.getByRole('button', { name: /^Save$/i }));

    expect(await screen.findByText('title: must not be empty')).toBeInTheDocument();
  });
});

describe('EntityInspector — fix round 1 (L2-T2-001/002/003/004)', () => {
  /** Stored rows with content the flat form cannot represent. */
  const richEntity: WorldKbEntityProjection = {
    key_block_id: 'kb-bo',
    world_id: 'w-1',
    block_type: 'character',
    canonical_name: 'Bo',
    status: 'confirmed',
    version: 1,
    modules: {
      belief: [
        {
          holder: 'chr_bo',
          proposition: '  the ferry is late  ',
          order: 0,
          truth: 'True',
          custom_model_state: { nested: ['a', 'b'] },
        },
        { holder: 'chr_mara', proposition: 'untouched row', order: 1, context: null },
      ],
    },
  };

  it('L2-T2-002: editing one known field preserves unknown inner keys, nulls, and exact untouched text', async () => {
    const user = userEvent.setup();
    const client = makeClient();
    renderWith(
      client,
      <EntityInspector worldId="w-1" node={node} entity={richEntity} onConflict={vi.fn()} />,
    );

    const section = screen.getByTestId('belief-section');
    await user.selectOptions(within(section).getAllByLabelText('Truth Status')[0], 'False');

    await user.click(screen.getByRole('button', { name: /^Save$/i }));

    await waitFor(() => expect(client.worldKbPatchEntity).toHaveBeenCalled());
    const belief = callsOf(client.worldKbPatchEntity)[0][1].patch!.modules!.belief as Array<
      Record<string, unknown>
    >;
    // The edited member applies; every untouched member of the same row —
    // including the unknown nested key — round-trips verbatim (no trimming).
    expect(belief[0]).toEqual({
      holder: 'chr_bo',
      proposition: '  the ferry is late  ',
      order: 0,
      truth: 'False',
      custom_model_state: { nested: ['a', 'b'] },
    });
    // The untouched second row is byte-identical, explicit null included.
    expect(belief[1]).toEqual({
      holder: 'chr_mara',
      proposition: 'untouched row',
      order: 1,
      context: null,
    });
  });

  it('L2-T2-001: a mental-only conflicting submit retains dirtyDialects ["mental"]', async () => {
    const user = userEvent.setup();
    const client = makeClient({
      worldKbPatchEntity: vi.fn().mockRejectedValue(
        new NexusClientError(409, 'world_kb_conflict', 'stale', {
          current_version: 7,
          entity_id: 'kb-ana',
          conflicting_path: 'modules',
          recovery_hint: 'r',
        }),
      ),
    });
    const onConflict = vi.fn();
    renderWith(
      client,
      <EntityInspector
        worldId="w-1"
        node={{ ...node, keyBlockId: 'kb-ana' }}
        entity={entityWithoutBeliefs}
        onConflict={onConflict}
      />,
    );

    fireEvent.change(screen.getByLabelText('Goals'), {
      target: { value: '{"focus":"the ferry"}' },
    });
    await user.click(screen.getByRole('button', { name: /^Save$/i }));

    await waitFor(() => expect(onConflict).toHaveBeenCalled());
    expect(onConflict.mock.calls[0][0]).toMatchObject({
      currentVersion: 7,
      dirtyFields: ['modules'],
      dirtyDialects: ['mental'],
    });
  });

  it('L2-T2-001: a belief-only conflicting submit retains dirtyDialects ["belief"] (a previously absent mental may be populated concurrently)', async () => {
    const user = userEvent.setup();
    const client = makeClient({
      worldKbPatchEntity: vi.fn().mockRejectedValue(
        new NexusClientError(409, 'world_kb_conflict', 'stale', {
          current_version: 7,
          entity_id: 'kb-ana',
          conflicting_path: 'modules',
          recovery_hint: 'r',
        }),
      ),
    });
    const onConflict = vi.fn();
    renderWith(
      client,
      <EntityInspector
        worldId="w-1"
        node={{ ...node, keyBlockId: 'kb-ana' }}
        entity={entityWithoutBeliefs}
        onConflict={onConflict}
      />,
    );

    const section = screen.getByTestId('belief-section');
    await user.click(within(section).getByRole('button', { name: 'Add belief' }));
    await user.type(within(section).getByLabelText('Proposition'), 'a brand-new belief');

    await user.click(screen.getByRole('button', { name: /^Save$/i }));

    await waitFor(() => expect(onConflict).toHaveBeenCalled());
    expect(onConflict.mock.calls[0][0].dirtyDialects).toEqual(['belief']);
  });

  it('L2-T2-003: maps holder/order 422 prefixes onto row fields, keeps a reason containing ": ", and leaves out-of-range addresses at section level', async () => {
    const user = userEvent.setup();
    const client = makeClient({
      worldKbPatchEntity: vi.fn().mockRejectedValue(
        new NexusClientError(422, 'world_kb_validation_failed', 'validation failed', {
          validation_summary: {
            errors: [
              'modules.belief.0.holder: must reference a known character: got "chr_x"',
              'modules.belief.1.order: must be a whole number: got 1.5',
              'modules.belief.5.truth: row does not exist',
            ],
          },
        }),
      ),
    });
    renderWith(
      client,
      <EntityInspector worldId="w-1" node={node} entity={entityWithBeliefs} onConflict={vi.fn()} />,
    );

    const proposition = screen.getByDisplayValue('the dawn ferry is late');
    await user.type(proposition, '!');
    await user.click(screen.getByRole('button', { name: /^Save$/i }));

    const section = await screen.findByTestId('belief-section');
    const rows = within(section).getAllByRole('group');
    // Non-closed-label members map 1:1 onto their row fields...
    expect(
      within(rows[0]).getByText('must reference a known character: got "chr_x"'),
    ).toBeInTheDocument();
    expect(within(rows[1]).getByText('must be a whole number: got 1.5')).toBeInTheDocument();
    // ...while a well-formed prefix addressing a row that does not exist stays
    // at the form-level error list, verbatim (outside the collapsible section).
    expect(screen.getByText('modules.belief.5.truth: row does not exist')).toBeInTheDocument();
  });

  it('L2-T2-004: closed-label options render the catalog label while the value stays the frozen handbook string', () => {
    renderWith(
      makeClient(),
      <EntityInspector worldId="w-1" node={node} entity={entityWithBeliefs} onConflict={vi.fn()} />,
    );

    const section = screen.getByTestId('belief-section');
    const contentType = within(section).getAllByLabelText('Content Type')[0];
    const option = within(contentType).getByRole('option', { name: 'Contents/Physical State' });
    expect(option).toHaveValue('Contents/Physical State');
  });
});

describe('EntityInspector — fix round 2 (L2-T2-002 remainder: stored {} rows)', () => {
  /** Holder with a degenerate stored `{}` row before a populated row. */
  const emptyRowEntity: WorldKbEntityProjection = {
    key_block_id: 'kb-bo',
    world_id: 'w-1',
    block_type: 'character',
    canonical_name: 'Bo',
    status: 'confirmed',
    version: 1,
    modules: {
      belief: [{}, { holder: 'chr_mara', proposition: 'the tide tables are wrong', order: 1 }],
    },
  };

  it('L2-T2-002 remainder: a title-only save with stored [{}] omits modules entirely (no deletion semantics)', async () => {
    const user = userEvent.setup();
    const client = makeClient();
    renderWith(
      client,
      <EntityInspector worldId="w-1" node={node} entity={emptyRowEntity} onConflict={vi.fn()} />,
    );

    const title = screen.getByLabelText('Title');
    await user.clear(title);
    await user.type(title, 'Bo the ferryman');

    await user.click(screen.getByRole('button', { name: /^Save$/i }));

    await waitFor(() => expect(client.worldKbPatchEntity).toHaveBeenCalled());
    const call = callsOf(client.worldKbPatchEntity)[0];
    expect(call[1]).toMatchObject({
      entity_id: 'kb-bo',
      expected_version: 1,
      patch: { title: 'Bo the ferryman' },
    });
    // The untouched stored `[{}]` must not turn into `belief: []`.
    expect(call[1].patch).not.toHaveProperty('modules');
  });

  it('L2-T2-002 remainder: editing the populated row preserves the untouched {} at its original index', async () => {
    const user = userEvent.setup();
    const client = makeClient();
    renderWith(
      client,
      <EntityInspector worldId="w-1" node={node} entity={emptyRowEntity} onConflict={vi.fn()} />,
    );

    const section = screen.getByTestId('belief-section');
    const rows = within(section).getAllByRole('group');
    expect(rows).toHaveLength(2);

    const proposition = within(rows[1]).getByLabelText('Proposition');
    await user.clear(proposition);
    await user.type(proposition, 'the tide tables are falsified');

    await user.click(screen.getByRole('button', { name: /^Save$/i }));

    await waitFor(() => expect(client.worldKbPatchEntity).toHaveBeenCalled());
    const belief = callsOf(client.worldKbPatchEntity)[0][1].patch!.modules!.belief as Array<
      Record<string, unknown>
    >;
    expect(belief).toHaveLength(2);
    expect(belief[0]).toEqual({});
    expect(belief[1]).toEqual({
      holder: 'chr_mara',
      proposition: 'the tide tables are falsified',
      order: 1,
    });
  });

  it('L2-T2-002 remainder: a mental-only save with stored [{}] omits belief (no spontaneous dialect dirty)', async () => {
    const user = userEvent.setup();
    const client = makeClient();
    renderWith(
      client,
      <EntityInspector worldId="w-1" node={node} entity={emptyRowEntity} onConflict={vi.fn()} />,
    );

    // Type into a mental raw-JSON field: the change makes mental the only
    // dirty dialect.
    const mentalSection = screen.getByTestId('mental-state-section');
    fireEvent.change(within(mentalSection).getByLabelText('Identity'), {
      target: { value: '{"note": "changed"}' },
    });

    await user.click(screen.getByRole('button', { name: /^Save$/i }));

    await waitFor(() => expect(client.worldKbPatchEntity).toHaveBeenCalled());
    const modules = callsOf(client.worldKbPatchEntity)[0][1].patch!.modules!;
    expect(modules).not.toHaveProperty('belief');
  });
});
