/**
 * GrowthCurve tests (V1.81 SP-3 — web-ui.md §26.1).
 *
 * Covers the three density states (empty / low-data / rich) with the
 * documented thresholds and copy, plus the pure `growthDensityFor` /
 * `growthSeries` helpers in soul-stats. The growth-curve always renders a chart
 * for non-empty states (it degrades to a simpler form rather than gating).
 */
import { screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { GrowthCurve } from '@/components/soul/growth-curve';
import {
  GROWTH_LOW_DATA_MAX_FRAGMENT,
  GROWTH_RICH_DAY_THRESHOLD,
  GROWTH_RICH_FRAGMENT_THRESHOLD,
  growthDensityFor,
  growthSeries,
} from '@/components/soul/soul-stats';
import { renderInApp } from '@/test/test-providers';
import type { MemoryFragmentInfo } from '@42ch/nexus-contracts';

function frag(over: Partial<MemoryFragmentInfo> = {}): MemoryFragmentInfo {
  return { fragment_id: 'f', summary: 's', ...over };
}

function stamped(id: string, day: number): MemoryFragmentInfo {
  return frag({
    fragment_id: id,
    created_at: `2026-06-${String(day).padStart(2, '0')}T00:00:00Z`,
  });
}

describe('growthDensityFor', () => {
  it('classifies empty when there are zero fragments', () => {
    expect(
      growthDensityFor({ fragmentCount: 0, distinctDays: 0 }),
    ).toBe('empty');
  });

  it('classifies low-data for 1–9 fragments under the day threshold', () => {
    expect(
      growthDensityFor({ fragmentCount: 1, distinctDays: 1 }),
    ).toBe('low-data');
    expect(
      growthDensityFor({
        fragmentCount: GROWTH_LOW_DATA_MAX_FRAGMENT,
        distinctDays: 2,
      }),
    ).toBe('low-data');
  });

  it('classifies rich at ≥10 fragments', () => {
    expect(
      growthDensityFor({
        fragmentCount: GROWTH_RICH_FRAGMENT_THRESHOLD,
        distinctDays: 1,
      }),
    ).toBe('rich');
  });

  it('classifies rich at ≥5 distinct days even with few fragments', () => {
    expect(
      growthDensityFor({
        fragmentCount: 4,
        distinctDays: GROWTH_RICH_DAY_THRESHOLD,
      }),
    ).toBe('rich');
  });
});

describe('growthSeries', () => {
  it('builds a cumulative point per distinct day', () => {
    const series = growthSeries([
      stamped('a', 1),
      stamped('b', 1),
      stamped('c', 3),
    ]);
    expect(series.distinctDays).toBe(2);
    expect(series.points).toEqual([
      { label: expect.any(String), cumulative: 2 },
      { label: expect.any(String), cumulative: 3 },
    ]);
  });

  it('drops fragments without a parseable timestamp', () => {
    const series = growthSeries([stamped('a', 1), frag({ created_at: undefined })]);
    expect(series.distinctDays).toBe(1);
    expect(series.points).toHaveLength(1);
  });
});

describe('GrowthCurve', () => {
  it('renders the forward-looking empty state for zero fragments', () => {
    renderInApp(<GrowthCurve fragments={[]} />);
    expect(screen.getByTestId('soul-growth-empty')).toBeInTheDocument();
    expect(screen.getByText(/your soul begins here/i)).toBeInTheDocument();
    // No chart in the empty state.
    expect(screen.queryByRole('img')).not.toBeInTheDocument();
  });

  it('renders the low-data line chart with the taking-shape copy', () => {
    renderInApp(<GrowthCurve fragments={[stamped('a', 1), stamped('b', 2)]} />);
    expect(screen.getByTestId('soul-growth-low-data')).toBeInTheDocument();
    expect(screen.getByText(/your soul is taking shape/i)).toBeInTheDocument();
    expect(screen.getByRole('img', { name: /cumulative fragment growth/i })).toBeInTheDocument();
  });

  it('renders the rich curve with a summary stat at ≥10 fragments', () => {
    const fragments = Array.from({ length: 12 }, (_, i) => stamped(`f${i}`, 1 + (i % 6)));
    renderInApp(<GrowthCurve fragments={fragments} />);
    expect(screen.getByTestId('soul-growth-rich')).toBeInTheDocument();
    // Summary stat: 12 fragments over N days.
    expect(screen.getByText(/12 fragments over/i)).toBeInTheDocument();
  });
});
