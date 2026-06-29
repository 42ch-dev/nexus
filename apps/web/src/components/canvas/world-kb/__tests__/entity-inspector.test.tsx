/**
 * World KB entity inspector tests — submit + inline validation + conflict handoff
 * (V1.73 P0 A6/A9).
 *
 * Verifies that a dirty field submits `worldKbPatchEntity` with the node's per-row
 * version as `expected_version`, that invalid JSON surfaces an inline error, and
 * that a 409 conflict hands off to the parent canvas (onConflict).
 */
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

import { makeQueryClient } from '@/test/test-providers';
import { QueryClientProvider } from '@tanstack/react-query';
import { ClientProvider } from '@/lib/client-context';
import { ToastProvider, Toaster } from '@/lib/use-toast';
import { NexusClientError, type NexusClient } from '@/lib/nexus';
import type { WorldKbEntityProjection } from '@42ch/nexus-contracts';

import { EntityInspector } from '../entity-inspector';
import type { WorldKbNodeData } from '../types';

const node: WorldKbNodeData = {
  worldId: 'w-1',
  keyBlockId: 'kb-1',
  entityKind: 'character',
  name: 'Aria Stormwind',
  lifecycle: 'confirmed',
  version: 3,
  sourceAnchorCount: 1,
  computable: false,
};

const entity: WorldKbEntityProjection = {
  key_block_id: 'kb-1',
  world_id: 'w-1',
  block_type: 'character',
  canonical_name: 'Aria Stormwind',
  status: 'confirmed',
  version: 3,
};

function makeClient(overrides: Partial<NexusClient> = {}): NexusClient {
  return {
    getWorldKbGraph: vi.fn(),
    getWorldKbCandidates: vi.fn(),
    worldKbPatchEntity: vi.fn().mockResolvedValue({
      entity,
      version: 4,
      validation_summary: { errors: [], warnings: [] },
    }),
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

describe('EntityInspector', () => {
  it('submits a patch with expected_version = node version when title changes', async () => {
    const user = userEvent.setup();
    const client = makeClient();
    const { findByDisplayValue, findByRole } = renderWith(
      client,
      <EntityInspector worldId="w-1" node={node} entity={entity} onConflict={vi.fn()} />,
    );

    const titleInput = await findByDisplayValue('Aria Stormwind');
    await user.type(titleInput, ' (v2)');

    const save = await findByRole('button', { name: /Save entity/i });
    await user.click(save);

    await waitFor(() => expect(client.worldKbPatchEntity).toHaveBeenCalled());
    const call = (client.worldKbPatchEntity as ReturnType<typeof vi.fn>).mock.calls[0];
    expect(call[0]).toBe('w-1');
    expect(call[1]).toMatchObject({
      entity_id: 'kb-1',
      expected_version: 3,
      patch: expect.objectContaining({ title: expect.stringContaining('(v2)') }),
    });
  });

  it('surfaces an inline error when body JSON is invalid', async () => {
    const user = userEvent.setup();
    const client = makeClient();
    const { findByLabelText, findByText, findByRole } = renderWith(
      client,
      <EntityInspector worldId="w-1" node={node} entity={entity} onConflict={vi.fn()} />,
    );

    const body = await findByLabelText(/Body/i);
    // Type a non-JSON string (a bare word is not valid JSON → JSON.parse throws).
    await user.type(body, 'not valid json');

    (await findByRole('button', { name: /Save entity/i })).click();
    expect(await findByText(/Body must be valid JSON/i)).toBeInTheDocument();
    expect(client.worldKbPatchEntity).not.toHaveBeenCalled();
  });

  it('hands a 409 conflict to the parent via onConflict', async () => {
    const user = userEvent.setup();
    const client = makeClient({
      worldKbPatchEntity: vi.fn().mockRejectedValue(
        new NexusClientError(409, 'world_kb_conflict', 'stale', {
          current_version: 7,
          entity_id: 'kb-1',
          conflicting_path: 'title',
          recovery_hint: 'r',
        }),
      ),
    });
    const onConflict = vi.fn();
    const { findByDisplayValue, findByRole } = renderWith(
      client,
      <EntityInspector worldId="w-1" node={node} entity={entity} onConflict={onConflict} />,
    );

    const title = await findByDisplayValue('Aria Stormwind');
    await user.type(title, '!');
    (await findByRole('button', { name: /Save entity/i })).click();

    await waitFor(() => expect(onConflict).toHaveBeenCalled());
    expect(onConflict.mock.calls[0][0]).toMatchObject({
      currentVersion: 7,
      entityId: 'kb-1',
      conflictingPath: 'title',
    });
  });

  // Regression for V1.73 greploop final (greptile P1): the entity patch hook
  // must surface non-conflict / non-validation failures (500/403/network) as a
  // toast instead of silently swallowing them — mirroring the promotion path's
  // non-conflict guard (extended to also skip 422 validation).
  it('surfaces a non-conflict error (500) as a toast, not silence', async () => {
    const user = userEvent.setup();
    const client = makeClient({
      worldKbPatchEntity: vi.fn().mockRejectedValue(
        new NexusClientError(500, 'internal_error', 'Boom — server failure'),
      ),
    });
    const onConflict = vi.fn();
    const { findByDisplayValue, findByRole } = renderWith(
      client,
      <EntityInspector worldId="w-1" node={node} entity={entity} onConflict={onConflict} />,
    );

    const title = await findByDisplayValue('Aria Stormwind');
    await user.type(title, '!');
    (await findByRole('button', { name: /Save entity/i })).click();

    // The hook's global onError toasts non-conflict/non-validation failures.
    expect(await screen.findByRole('alert')).toBeInTheDocument();
    expect(await screen.findByText('Could not save entity')).toBeInTheDocument();
    // A 500 is neither a conflict nor validation — no modal handoff.
    await waitFor(() => expect(client.worldKbPatchEntity).toHaveBeenCalled());
    expect(onConflict).not.toHaveBeenCalled();
  });
});
