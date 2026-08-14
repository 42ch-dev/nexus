/**
 * Entity inspector — modules.mental read-only display (V1.164 P3 Task 3).
 *
 * Locks AC-V1164-12/15 + PD-16 for the App character inspector: a
 * KnowledgeEntry with a populated `modules.mental` bag renders the
 * collapsible "Mental State" section with every populated nine-field key
 * (bold label + JSON value rows, read-only — no input controls). Missing /
 * null `modules.mental` omits the section entirely — no empty panel, no
 * placeholder rows. Copy resolves through the existing web i18n (canvas
 * namespace, `worldKb.entityInspector.mentalSection.*`).
 */
import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

import { makeQueryClient } from '@/test/test-providers';
import { QueryClientProvider } from '@tanstack/react-query';
import { ClientProvider } from '@/lib/client-context';
import { ToastProvider, Toaster } from '@/lib/use-toast';
import type { NexusClient } from '@/lib/nexus';
import type { WorldKbEntityProjection } from '@42ch/nexus-contracts';

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

/** Character with an explicit null `modules.mental` (PD-16 null degradation). */
const entityWithNullMental: WorldKbEntityProjection = {
  ...entityWithoutMental,
  key_block_id: 'kb-null',
  canonical_name: 'Nullbag',
  modules: { mental: null },
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

describe('EntityInspector — modules.mental section (V1.164 P3 Task 3)', () => {
  it('renders the mental section with beliefs, goals, emotions when modules.mental is populated', () => {
    renderWith(
      makeClient(),
      <EntityInspector worldId="w-1" node={node} entity={entityWithMental} onConflict={vi.fn()} />,
    );

    const section = screen.getByTestId('mental-state-section');
    expect(section).toBeInTheDocument();
    expect(screen.getByText('Mental State')).toBeInTheDocument();

    // The AC proof: at minimum beliefs / goals / emotions render as
    // bold label + value rows.
    expect(within(section).getByText('Beliefs')).toBeInTheDocument();
    expect(within(section).getByText('Goals')).toBeInTheDocument();
    expect(within(section).getByText('Emotions')).toBeInTheDocument();

    // Values render as pretty JSON (read-only display of structured data).
    expect(within(section).getByText(/kb_bo_beliefs/)).toBeInTheDocument();
    expect(within(section).getByText(/clear the dawn berths/)).toBeInTheDocument();
    expect(within(section).getByText(/"alert"/)).toBeInTheDocument();
  });

  it('renders additional populated nine-field keys in the same section (PD-16)', () => {
    renderWith(
      makeClient(),
      <EntityInspector worldId="w-1" node={node} entity={entityWithMental} onConflict={vi.fn()} />,
    );

    const section = screen.getByTestId('mental-state-section');
    expect(within(section).getByText('Identity')).toBeInTheDocument();
    expect(within(section).getByText('Attention')).toBeInTheDocument();
    expect(within(section).getByText('Norms')).toBeInTheDocument();
    expect(within(section).getByText('Constraints')).toBeInTheDocument();
  });

  it('omits the mental section when modules.mental is absent (PD-16)', () => {
    renderWith(
      makeClient(),
      <EntityInspector worldId="w-1" node={node} entity={entityWithoutMental} onConflict={vi.fn()} />,
    );

    expect(screen.queryByTestId('mental-state-section')).not.toBeInTheDocument();
    expect(screen.queryByText('Mental State')).not.toBeInTheDocument();
    expect(screen.queryByText('Beliefs')).not.toBeInTheDocument();
  });

  it('omits the mental section when modules.mental is null (PD-16)', () => {
    renderWith(
      makeClient(),
      <EntityInspector worldId="w-1" node={node} entity={entityWithNullMental} onConflict={vi.fn()} />,
    );

    expect(screen.queryByTestId('mental-state-section')).not.toBeInTheDocument();
    expect(screen.queryByText('Mental State')).not.toBeInTheDocument();
  });

  it('collapses and re-expands the mental section via the header toggle', async () => {
    const user = userEvent.setup();
    renderWith(
      makeClient(),
      <EntityInspector worldId="w-1" node={node} entity={entityWithMental} onConflict={vi.fn()} />,
    );

    const section = screen.getByTestId('mental-state-section');
    const toggle = screen.getByRole('button', { name: 'Mental State' });
    expect(toggle).toHaveAttribute('aria-expanded', 'true');
    expect(within(section).getByText('Beliefs')).toBeInTheDocument();

    // Collapse — the field rows hide, the section header stays.
    await user.click(toggle);
    expect(toggle).toHaveAttribute('aria-expanded', 'false');
    expect(within(section).queryByText('Beliefs')).not.toBeInTheDocument();

    // Re-expand — rows return.
    await user.click(toggle);
    expect(toggle).toHaveAttribute('aria-expanded', 'true');
    expect(within(section).getByText('Beliefs')).toBeInTheDocument();
  });
});
