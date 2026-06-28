/**
 * World KB non-spatial alternate view tests (V1.73 P0 A6/A8/A9).
 *
 * Verifies sortable columns, keyboard row activation (Enter opens inspector),
 * and that every row is keyboard-focusable — the accessibility invariant that
 * the alt view is a complete equivalent of the canvas graph.
 */
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

import { WorldKbAltView } from '../world-kb-alt-view';
import type { WorldKbNodeData } from '../types';

function node(overrides: Partial<WorldKbNodeData>): WorldKbNodeData {
  return {
    worldId: 'w-1',
    entityKind: 'character',
    name: 'Entity',
    lifecycle: 'confirmed',
    version: 1,
    sourceAnchorCount: 0,
    computable: false,
    ...overrides,
  };
}

const nodes: WorldKbNodeData[] = [
  node({ keyBlockId: 'b', name: 'Beta', entityKind: 'scene', sourceAnchorCount: 3, updatedAt: '2026-06-20T00:00:00Z' }),
  node({ keyBlockId: 'a', name: 'Aria', entityKind: 'character', sourceAnchorCount: 1, updatedAt: '2026-06-28T00:00:00Z' }),
  node({ candidateId: 'c', name: 'Cand', entityKind: 'character', lifecycle: 'pending', sourceAnchorCount: 0, updatedAt: '2026-06-29T00:00:00Z' }),
];

describe('WorldKbAltView', () => {
  it('renders all rows with name + lifecycle', () => {
    render(<WorldKbAltView nodes={nodes} selectedId={null} onSelect={vi.fn()} />);
    expect(screen.getByText('Aria')).toBeInTheDocument();
    expect(screen.getByText('Beta')).toBeInTheDocument();
    expect(screen.getByText('Cand')).toBeInTheDocument();
    expect(screen.getAllByText(/pending/i).length).toBeGreaterThan(0);
  });

  it('sorts by name ascending then descending', async () => {
    const user = userEvent.setup();
    render(<WorldKbAltView nodes={nodes} selectedId={null} onSelect={vi.fn()} />);
    const nameHeader = screen.getByRole('button', { name: /Name/i });

    // Default asc by name → Aria first.
    await user.click(nameHeader); // toggle to desc
    const rows = screen.getAllByRole('row');
    // Header is row 0; first data row name:
    const firstDataCell = rows[1]?.textContent ?? '';
    expect(firstDataCell).toContain('Cand'); // desc → Cand first
  });

  it('activates a row via keyboard (Enter) and via click', async () => {
    const user = userEvent.setup();
    const onSelect = vi.fn();
    render(<WorldKbAltView nodes={nodes} selectedId={null} onSelect={onSelect} />);

    // Click path.
    await user.click(screen.getByText('Aria'));
    expect(onSelect).toHaveBeenCalled();
    const clicked = onSelect.mock.calls[onSelect.mock.calls.length - 1][0] as WorldKbNodeData;
    expect(clicked.keyBlockId).toBe('a');

    // Keyboard path: focus a row, press Enter.
    const betaRow = screen.getByText('Beta').closest('tr')!;
    betaRow.focus();
    await user.keyboard('{Enter}');
    expect(onSelect.mock.calls[onSelect.mock.calls.length - 1][0]).toMatchObject({ keyBlockId: 'b' });
  });

  it('marks aria-sort on the active column', () => {
    render(<WorldKbAltView nodes={nodes} selectedId={null} onSelect={vi.fn()} />);
    const nameHeader = screen.getByRole('button', { name: /^Name/i }).closest('th');
    expect(nameHeader?.getAttribute('aria-sort')).toBe('ascending');
  });
});
