/**
 * World KB relationship inspector tests (V1.74 A6).
 *
 * Covers: create vs edit mode, source/target prefilling, validation,
 * custom label requirement, submit mutation payload, remove action, and
 * conflict callback.
 */
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { RelationshipInspector } from '../relationship-inspector';
import { NexusClientError } from '@/lib/nexus/errors';
import type {
  WorldKbEntityProjection,
  WorldKbRelationshipProjection,
  WorldKbSourceAnchorProjection,
} from '@42ch/nexus-contracts';

const mutate = vi.fn();

vi.mock('@/lib/canvas/use-world-kb-data', () => ({
  usePatchWorldKbRelationship: () => ({ mutate, isPending: false }),
  isWorldKbConflictError: (error: unknown) =>
    error instanceof NexusClientError && error.status === 409 && error.code === 'world_kb_conflict',
  isWorldKbValidationError: (error: unknown) =>
    error instanceof NexusClientError && error.status === 422 && error.code === 'world_kb_validation_failed',
}));

function entity(id: string, name: string): WorldKbEntityProjection {
  return {
    key_block_id: id,
    canonical_name: name,
    block_type: 'character',
    status: 'confirmed',
    version: 1,
    source_anchor_count: 0,
    updated_at: '2026-06-29T00:00:00Z',
  } as WorldKbEntityProjection;
}

function relationship(overrides: Partial<WorldKbRelationshipProjection> = {}): WorldKbRelationshipProjection {
  return {
    relationship_id: 'r-1',
    source_entity_id: 'a',
    target_entity_id: 'b',
    relation_type: 'references',
    symmetric: false,
    confidence: 0.8,
    source_anchor_ids: ['sa-1'],
    version: 3,
    projection_direction: 'stored',
    updated_at: '2026-06-29T00:00:00Z',
    ...overrides,
  } as WorldKbRelationshipProjection;
}

const entities = [entity('a', 'Aria'), entity('b', 'Beta'), entity('c', 'Cora')];
const anchors: WorldKbSourceAnchorProjection[] = [];

afterEach(() => {
  mutate.mockReset();
});

describe('RelationshipInspector', () => {
  it('renders create mode with prefilled source and target', () => {
    render(
      <RelationshipInspector
        worldId="w-1"
        initialSourceEntityId="a"
        initialTargetEntityId="b"
        entities={entities}
        anchors={anchors}
      />,
    );
    expect(screen.getByRole('heading', { name: /New Relationship/i })).toBeInTheDocument();
    expect(screen.getByLabelText(/Source entity/i)).toHaveValue('a');
    expect(screen.getByLabelText(/Target entity/i)).toHaveValue('b');
  });

  it('renders edit mode and disables source/target selects', () => {
    render(
      <RelationshipInspector
        worldId="w-1"
        relationship={relationship()}
        entities={entities}
        anchors={anchors}
      />,
    );
    expect(screen.getByRole('heading', { name: /Edit Relationship/i })).toBeInTheDocument();
    expect(screen.getByLabelText(/Source entity/i)).toBeDisabled();
    expect(screen.getByLabelText(/Target entity/i)).toBeDisabled();
  });

  it('validates missing source/target/confidence and self-loop', async () => {
    const user = userEvent.setup();
    render(
      <RelationshipInspector
        worldId="w-1"
        initialSourceEntityId="a"
        initialTargetEntityId="a"
        entities={entities}
        anchors={anchors}
      />,
    );
    await user.click(screen.getByRole('button', { name: /Add relationship/i }));
    expect(screen.getByText(/Source and target must be different entities/i)).toBeInTheDocument();
    expect(mutate).not.toHaveBeenCalled();
  });

  it('requires custom label for custom relation type', async () => {
    const user = userEvent.setup();
    render(
      <RelationshipInspector
        worldId="w-1"
        initialSourceEntityId="a"
        initialTargetEntityId="b"
        entities={entities}
        anchors={anchors}
      />,
    );
    await user.selectOptions(screen.getByLabelText(/Relation type/i), 'custom');
    await user.click(screen.getByRole('button', { name: /Add relationship/i }));
    expect(screen.getByText(/Custom label is required/i)).toBeInTheDocument();
  });

  it('submits add payload with trimmed custom label', async () => {
    const user = userEvent.setup();
    render(
      <RelationshipInspector
        worldId="w-1"
        initialSourceEntityId="a"
        initialTargetEntityId="b"
        entities={entities}
        anchors={anchors}
      />,
    );
    await user.selectOptions(screen.getByLabelText(/Relation type/i), 'custom');
    await user.type(screen.getByLabelText(/Custom label/i), '  Childhood Friend  ');
    await user.click(screen.getByRole('button', { name: /Add relationship/i }));
    await waitFor(() => expect(mutate).toHaveBeenCalled());
    const request = mutate.mock.calls[0][0];
    expect(request.action).toBe('add');
    expect(request.relationship.custom_label).toBe('Childhood Friend');
  });

  it('submits update payload with expected_version', async () => {
    const user = userEvent.setup();
    render(
      <RelationshipInspector
        worldId="w-1"
        relationship={relationship()}
        entities={entities}
        anchors={anchors}
      />,
    );
    await user.click(screen.getByRole('button', { name: /Save changes/i }));
    await waitFor(() => expect(mutate).toHaveBeenCalled());
    const request = mutate.mock.calls[0][0];
    expect(request.action).toBe('update');
    expect(request.expected_version).toBe(3);
    expect(request.relationship_id).toBe('r-1');
  });

  it('calls onConflict when edit returns 409', async () => {
    const user = userEvent.setup();
    const onConflict = vi.fn();
    render(
      <RelationshipInspector
        worldId="w-1"
        relationship={relationship()}
        entities={entities}
        anchors={anchors}
        onConflict={onConflict}
      />,
    );
    mutate.mockImplementationOnce((_req: unknown, opts: { onError?: (e: unknown) => void }) => {
      opts?.onError?.(
        new NexusClientError(409, 'world_kb_conflict', 'stale version', {
          current_version: 7,
          conflicting_path: 'version',
          recovery_hint: '',
        }),
      );
    });
    await user.click(screen.getByRole('button', { name: /Save changes/i }));
    await waitFor(() => expect(onConflict).toHaveBeenCalled());
    expect(onConflict.mock.calls[0][0].currentVersion).toBe(7);
  });

  it('removes relationship in edit mode', async () => {
    const user = userEvent.setup();
    const onSaved = vi.fn();
    render(
      <RelationshipInspector
        worldId="w-1"
        relationship={relationship()}
        entities={entities}
        anchors={anchors}
        onSaved={onSaved}
      />,
    );
    await user.click(screen.getByRole('button', { name: /Remove relationship/i }));
    await waitFor(() => expect(mutate).toHaveBeenCalled());
    const request = mutate.mock.calls[0][0];
    expect(request.action).toBe('remove');
    expect(request.expected_version).toBe(3);
  });
});
