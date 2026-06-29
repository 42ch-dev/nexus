/**
 * World KB relationship table tests (V1.74 A6).
 *
 * Covers: render rows, sortable columns, selection (click + keyboard),
 * "New Relationship" button, delete confirmation, symmetric rows skipped.
 */
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { WorldKbRelationshipTable } from '../world-kb-relationship-table';
import type { WorldKbEntityProjection, WorldKbRelationshipProjection } from '@42ch/nexus-contracts';

const entity = (id: string, name: string): WorldKbEntityProjection =>
  ({
    key_block_id: id,
    canonical_name: name,
    block_type: 'character',
    status: 'confirmed',
    version: 1,
    source_anchor_count: 0,
    updated_at: '2026-06-29T00:00:00Z',
  }) as WorldKbEntityProjection;

const rel = (overrides: Partial<WorldKbRelationshipProjection> = {}): WorldKbRelationshipProjection =>
  ({
    relationship_id: 'r-1',
    source_entity_id: 'a',
    target_entity_id: 'b',
    relation_type: 'references',
    symmetric: false,
    confidence: 0.8,
    source_anchor_ids: ['sa-1'],
    version: 1,
    projection_direction: 'stored',
    updated_at: '2026-06-29T00:00:00Z',
    ...overrides,
  }) as WorldKbRelationshipProjection;

const entities = [entity('a', 'Aria'), entity('b', 'Beta')];

const confirmMock = vi.fn(() => false);
vi.stubGlobal('confirm', confirmMock);

afterEach(() => {
  confirmMock.mockReset();
  confirmMock.mockReturnValue(false);
});

describe('WorldKbRelationshipTable', () => {
  it('renders stored relationship rows only', () => {
    render(
      <WorldKbRelationshipTable
        relationships={[rel(), rel({ projection_direction: 'symmetric_reverse', relationship_id: 'r-1' })]}
        entities={entities}
        selectedId={null}
        onSelect={vi.fn()}
        onCreate={vi.fn()}
      />,
    );
    expect(screen.getAllByRole('row').length).toBe(2); // header + 1 stored
  });

  it('sorts by type ascending/descending', async () => {
    const user = userEvent.setup();
    render(
      <WorldKbRelationshipTable
        relationships={[
          rel({ relationship_id: 'r-2', relation_type: 'allied_with', source_entity_id: 'a', target_entity_id: 'b' }),
          rel({ relationship_id: 'r-1', relation_type: 'references', source_entity_id: 'a', target_entity_id: 'b' }),
        ]}
        entities={entities}
        selectedId={null}
        onSelect={vi.fn()}
        onCreate={vi.fn()}
      />,
    );
    const typeHeader = screen.getByRole('button', { name: /Type/i });
    const rows = () => screen.getAllByRole('row');
    // default asc by type → Allied With first
    expect(rows()[1]?.textContent).toContain('Allied With');
    await user.click(typeHeader); // desc
    expect(rows()[1]?.textContent).toContain('References');
  });

  it('selects a row by click and keyboard', async () => {
    const user = userEvent.setup();
    const onSelect = vi.fn();
    render(
      <WorldKbRelationshipTable
        relationships={[rel()]}
        entities={entities}
        selectedId={null}
        onSelect={onSelect}
        onCreate={vi.fn()}
      />,
    );
    await user.click(screen.getByText('Aria'));
    expect(onSelect).toHaveBeenCalled();
    const row = screen.getByText('Beta').closest('tr')!;
    row.focus();
    await user.keyboard('{Enter}');
    expect(onSelect).toHaveBeenCalledTimes(2);
  });

  it('fires onCreate from New Relationship button', async () => {
    const user = userEvent.setup();
    const onCreate = vi.fn();
    render(
      <WorldKbRelationshipTable
        relationships={[]}
        entities={entities}
        selectedId={null}
        onSelect={vi.fn()}
        onCreate={onCreate}
      />,
    );
    await user.click(screen.getByRole('button', { name: /New Relationship/i }));
    expect(onCreate).toHaveBeenCalled();
  });

  it('confirms before delete', async () => {
    const user = userEvent.setup();
    confirmMock.mockReturnValueOnce(true);
    const onDelete = vi.fn();
    render(
      <WorldKbRelationshipTable
        relationships={[rel()]}
        entities={entities}
        selectedId={null}
        onSelect={vi.fn()}
        onCreate={vi.fn()}
        onDelete={onDelete}
      />,
    );
    await user.click(screen.getByRole('button', { name: /Delete relationship/i }));
    expect(confirmMock).toHaveBeenCalled();
    expect(onDelete).toHaveBeenCalled();
  });
});
