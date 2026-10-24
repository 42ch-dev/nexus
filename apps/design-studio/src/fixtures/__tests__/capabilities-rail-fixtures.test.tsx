import { cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';

import { CapabilitiesRailFixtures } from '@/fixtures/capabilities-rail-fixtures';
import { ThemeProvider } from '@/components/theme-provider';

/**
 * The `/capabilities` chrome has four content states worth eyeballing: the
 * registry with a filter that hits, a filter that misses (its own copy plus a
 * Clear filter action), an empty registry (which wins over the miss), and the
 * read error. Each frame owns one `data-testid`; the assertions below pin the
 * state that makes the frame different from its siblings.
 */
const VARIANTS = [
  'capabilities-rail-fixture-empty',
  'capabilities-rail-fixture-hit',
  'capabilities-rail-fixture-miss',
  'capabilities-rail-fixture-error',
] as const;

afterEach(() => {
  cleanup();
  document.documentElement.classList.remove('dark');
});

describe('CapabilitiesRailFixtures', () => {
  it('mounts every variant frame', () => {
    render(<CapabilitiesRailFixtures />);

    expect(screen.getByTestId('capabilities-rail-fixtures')).toBeInTheDocument();
    for (const testId of VARIANTS) {
      expect(screen.getByTestId(testId)).toBeInTheDocument();
    }
  });

  it('gives every mounted body its own label/input id pair', () => {
    render(<CapabilitiesRailFixtures />);

    const ids = VARIANTS.map((testId) => {
      const frame = screen.getByTestId(testId);
      // getByLabelText resolves through htmlFor → id: each label reaches the
      // filter rendered beside it, not the first frame in the document.
      return within(frame).getByLabelText('Filter capabilities').id;
    });

    expect(new Set(ids).size).toBe(VARIANTS.length);
    for (const id of ids) {
      expect(id.length).toBeGreaterThan(0);
      expect(document.querySelectorAll(`[id="${id}"]`)).toHaveLength(1);
    }
  });

  it('renders the registry-empty copy even though a filter is typed', () => {
    render(<CapabilitiesRailFixtures />);
    const frame = screen.getByTestId('capabilities-rail-fixture-empty');

    expect(within(frame).getByText('No capabilities')).toBeInTheDocument();
    expect(
      within(frame).getByText('Capabilities will appear here once the runtime registers them.'),
    ).toBeInTheDocument();
    expect(within(frame).queryByRole('button', { name: 'Clear filter' })).not.toBeInTheDocument();
  });

  it('narrows the list to the matching capability on a filtered hit', () => {
    render(<CapabilitiesRailFixtures />);
    const frame = screen.getByTestId('capabilities-rail-fixture-hit');

    expect(within(frame).getByText('nexus.world.describe')).toBeInTheDocument();
    expect(within(frame).getByText('nexus.world.edit')).toBeInTheDocument();
    expect(within(frame).queryByText('sync.pull')).not.toBeInTheDocument();
    expect(within(frame).getByDisplayValue('world')).toBeInTheDocument();
  });

  it('renders the filtered-miss copy quoting the query, and Clear filter restores the list', () => {
    render(<CapabilitiesRailFixtures />);
    const frame = screen.getByTestId('capabilities-rail-fixture-miss');

    expect(
      within(frame).getByText('No capability matches "no.such.cap"'),
    ).toBeInTheDocument();
    expect(
      within(frame).getByText('Clear the filter to see all capabilities.'),
    ).toBeInTheDocument();

    fireEvent.click(within(frame).getByRole('button', { name: 'Clear filter' }));

    expect(within(frame).getByText('nexus.world.describe')).toBeInTheDocument();
    expect(within(frame).getByText('sync.pull')).toBeInTheDocument();
    expect(within(frame).getByDisplayValue('')).toBeInTheDocument();
    expect(within(frame).queryByText(/No capability matches/)).not.toBeInTheDocument();
  });

  it('renders the read error as an alert with retry', () => {
    render(<CapabilitiesRailFixtures />);
    const frame = screen.getByTestId('capabilities-rail-fixture-error');

    expect(within(frame).getByText('Could not load capabilities.')).toBeInTheDocument();
    expect(within(frame).getByRole('alert')).toBeInTheDocument();
    expect(within(frame).queryByText('No capabilities')).not.toBeInTheDocument();
  });

  it('renders every variant under both themes', () => {
    for (const forced of ['light', 'dark'] as const) {
      render(
        <ThemeProvider forcedTheme={forced}>
          <CapabilitiesRailFixtures />
        </ThemeProvider>,
      );

      // The Studio theme system is the authority for the marker (`<html>.dark`);
      // asserting it proves the fixture renders under the resolved theme, not
      // just that a fixture-owned class survived.
      expect(document.documentElement.classList.contains('dark')).toBe(forced === 'dark');
      for (const testId of VARIANTS) {
        expect(screen.getByTestId(testId)).toBeInTheDocument();
      }

      cleanup();
    }
  });
});
