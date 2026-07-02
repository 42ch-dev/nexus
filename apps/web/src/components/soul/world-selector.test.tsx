/**
 * WorldSelector tests (V1.81 SP-2 — web-ui.md §26.1).
 *
 * Covers the pure `deriveWorldOptions` derivation (grouping, counts, the
 * omitted-worlds invariant) and the rendered control: "All worlds" default,
 * world options with fragment counts, and selection re-scoping.
 */
import { fireEvent, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import {
  WorldSelector,
  deriveWorldOptions,
  worldOptionLabel,
} from '@/components/soul/world-selector';
import { renderInApp } from '@/test/test-providers';
import type { MemoryFragmentInfo } from '@42ch/nexus-contracts';

function frag(over: Partial<MemoryFragmentInfo> = {}): MemoryFragmentInfo {
  return { fragment_id: 'f', summary: 's', ...over };
}

describe('deriveWorldOptions', () => {
  it('groups fragments by world_id and counts them', () => {
    const options = deriveWorldOptions([
      frag({ world_id: 'eryndor' }),
      frag({ world_id: 'eryndor' }),
      frag({ world_id: 'solara' }),
    ]);
    expect(options).toEqual([
      { worldId: 'eryndor', fragmentCount: 2 },
      { worldId: 'solara', fragmentCount: 1 },
    ]);
  });

  it('omits Creator-core-only fragments (null/empty world_id)', () => {
    const options = deriveWorldOptions([
      frag({ world_id: null }),
      frag({ world_id: '' }),
      frag({ world_id: '   ' }),
      frag({ world_id: 'eryndor' }),
    ]);
    // Only the world-backed fragment produces an option; Creator-core-only
    // fragments contribute to "All worlds" but never become a world option.
    expect(options).toEqual([{ worldId: 'eryndor', fragmentCount: 1 }]);
  });

  it('omits worlds with zero fragments (no dead-end empty options)', () => {
    // A world never enters the option set unless a fragment carries its id —
    // so a zero-activity world is structurally impossible here. This test pins
    // the invariant: empty input yields no options.
    expect(deriveWorldOptions([])).toEqual([]);
  });

  it('sorts options by world_id for stable order', () => {
    const options = deriveWorldOptions([
      frag({ world_id: 'zeta' }),
      frag({ world_id: 'alpha' }),
      frag({ world_id: 'mid' }),
    ]);
    expect(options.map((o) => o.worldId)).toEqual(['alpha', 'mid', 'zeta']);
  });
});

describe('worldOptionLabel', () => {
  it('uses singular "fragment" for a count of 1', () => {
    expect(worldOptionLabel({ worldId: 'solara', fragmentCount: 1 })).toBe(
      'solara (1 fragment)',
    );
  });

  it('uses plural "fragments" for counts > 1', () => {
    expect(worldOptionLabel({ worldId: 'eryndor', fragmentCount: 42 })).toBe(
      'eryndor (42 fragments)',
    );
  });
});

describe('WorldSelector', () => {
  it('defaults to "All worlds" and frames the whole Creator SOUL', () => {
    renderInApp(
      <WorldSelector
        options={[{ worldId: 'eryndor', fragmentCount: 42 }]}
        selectedWorld={null}
        onSelect={() => {}}
      />,
    );
    const select = screen.getByTestId('soul-world-selector') as HTMLSelectElement;
    expect(select.value).toBe('');
    expect(screen.getByText('your whole Creator SOUL')).toBeInTheDocument();
    expect(screen.getByText('eryndor (42 fragments)')).toBeInTheDocument();
  });

  it('selecting a world re-scopes the projection and reframes the label', () => {
    const onSelect = vi.fn();
    renderInApp(
      <WorldSelector
        options={[{ worldId: 'eryndor', fragmentCount: 2 }]}
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
        options={[{ worldId: 'eryndor', fragmentCount: 2 }]}
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

  it('disables when there are no world options', () => {
    renderInApp(
      <WorldSelector options={[]} selectedWorld={null} onSelect={() => {}} />,
    );
    expect(
      (screen.getByTestId('soul-world-selector') as HTMLSelectElement).disabled,
    ).toBe(true);
  });
});
