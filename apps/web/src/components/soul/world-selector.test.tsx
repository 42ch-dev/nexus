/**
 * WorldSelector tests (V1.82 SP-2 — web-ui.md §26.1).
 *
 * Covers the pure `countFragmentsByWorld` helper and the rendered control:
 * "All worlds" default, world options rendered with titles + fragment counts,
 * zero-fragment worlds included, and selection re-scoping.
 */
import { fireEvent, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { WorldSelector, countFragmentsByWorld } from '@/components/soul/world-selector';
import { renderInApp } from '@/test/test-providers';
import type { World } from '@42ch/nexus-contracts';

function world(over: Partial<World> = {}): World {
  return {
    schema_version: 1,
    world_id: 'w-1',
    owner_creator_id: 'c1',
    title: 'World One',
    slug: 'world-one',
    status: 'active',
    visibility: 'private',
    time_policy: 'manual',
    created_at: '2026-07-01T00:00:00Z',
    ...over,
  };
}

describe('countFragmentsByWorld', () => {
  it('counts fragments by world_id', () => {
    const counts = countFragmentsByWorld([
      { world_id: 'eryndor' },
      { world_id: 'eryndor' },
      { world_id: 'solara' },
    ]);
    expect(counts).toEqual({ eryndor: 2, solara: 1 });
  });

  it('ignores Creator-core-only fragments (null/empty world_id)', () => {
    const counts = countFragmentsByWorld([
      { world_id: null },
      { world_id: '' },
      { world_id: '   ' },
      { world_id: 'eryndor' },
    ]);
    expect(counts).toEqual({ eryndor: 1 });
  });

  it('returns an empty record for empty input', () => {
    expect(countFragmentsByWorld([])).toEqual({});
  });
});

describe('WorldSelector', () => {
  it('defaults to "All worlds" and frames the whole Creator SOUL', () => {
    renderInApp(
      <WorldSelector
        worlds={[world({ world_id: 'eryndor', title: 'Eryndor' })]}
        fragmentCounts={{ eryndor: 42 }}
        selectedWorld={null}
        onSelect={() => {}}
      />,
    );
    const select = screen.getByTestId('soul-world-selector') as HTMLSelectElement;
    expect(select.value).toBe('');
    expect(screen.getByText('your whole Creator SOUL')).toBeInTheDocument();
    expect(screen.getByText('Eryndor (42 fragments)')).toBeInTheDocument();
  });

  it('renders world titles, not raw world_id', () => {
    renderInApp(
      <WorldSelector
        worlds={[world({ world_id: 'w-eryndor', title: 'The Realms of Eryndor' })]}
        fragmentCounts={{ 'w-eryndor': 5 }}
        selectedWorld={null}
        onSelect={() => {}}
      />,
    );
    expect(screen.getByText('The Realms of Eryndor (5 fragments)')).toBeInTheDocument();
    expect(screen.queryByText('w-eryndor')).not.toBeInTheDocument();
  });

  it('includes Work-backed worlds with zero fragments', () => {
    renderInApp(
      <WorldSelector
        worlds={[world({ world_id: 'solara', title: 'Solara' })]}
        fragmentCounts={{}}
        selectedWorld={null}
        onSelect={() => {}}
      />,
    );
    expect(screen.getByText('Solara (no fragments)')).toBeInTheDocument();
  });

  it('sorts options by title', () => {
    renderInApp(
      <WorldSelector
        worlds={[
          world({ world_id: 'zeta', title: 'Zeta' }),
          world({ world_id: 'alpha', title: 'Alpha' }),
        ]}
        fragmentCounts={{ zeta: 1, alpha: 2 }}
        selectedWorld={null}
        onSelect={() => {}}
      />,
    );
    const options = screen.getAllByRole('option').slice(1); // skip "All worlds"
    expect(options.map((o) => o.textContent)).toEqual(['Alpha (2 fragments)', 'Zeta (1 fragment)']);
  });

  it('selecting a world re-scopes the projection and reframes the label', () => {
    const onSelect = vi.fn();
    renderInApp(
      <WorldSelector
        worlds={[world({ world_id: 'eryndor', title: 'Eryndor' })]}
        fragmentCounts={{ eryndor: 2 }}
        selectedWorld={null}
        onSelect={onSelect}
      />,
    );
    fireEvent.change(screen.getByTestId('soul-world-selector'), {
      target: { value: 'eryndor' },
    });
    expect(onSelect).toHaveBeenCalledWith('eryndor');
  });

  it('restoring "All worlds" passes the null sentinel', () => {
    const onSelect = vi.fn();
    renderInApp(
      <WorldSelector
        worlds={[world({ world_id: 'eryndor', title: 'Eryndor' })]}
        fragmentCounts={{ eryndor: 2 }}
        selectedWorld="eryndor"
        onSelect={onSelect}
      />,
    );
    expect(screen.getByText('a subset of your Creator SOUL')).toBeInTheDocument();
    fireEvent.change(screen.getByTestId('soul-world-selector'), {
      target: { value: '' },
    });
    expect(onSelect).toHaveBeenCalledWith(null);
  });

  it('shows an honest empty helper when there are no worlds', () => {
    renderInApp(
      <WorldSelector worlds={[]} fragmentCounts={{}} selectedWorld={null} onSelect={() => {}} />,
    );
    expect(
      (screen.getByTestId('soul-world-selector') as HTMLSelectElement).disabled,
    ).toBe(true);
    expect(screen.getByText('no worlds in this workspace')).toBeInTheDocument();
  });
});
