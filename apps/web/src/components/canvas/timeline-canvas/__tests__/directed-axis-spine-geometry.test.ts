import { describe, expect, it } from 'vitest';

import {
  narrativeSpineTickLayout,
  narrativeSpineTickX,
} from '../directed-axis-spine';

describe('DirectedAxisSpine narrative geometry — 1000-event stress', () => {
  it('keeps 1000 ticks finite, monotonic, and within computed width', () => {
    const totalTicks = 1000;
    const { tickSpacing, totalWidth } = narrativeSpineTickLayout(totalTicks);

    expect(Number.isFinite(tickSpacing)).toBe(true);
    expect(tickSpacing).toBeGreaterThan(0);
    expect(Number.isFinite(totalWidth)).toBe(true);
    expect(totalWidth).toBeGreaterThan(0);

    const first = narrativeSpineTickX(0, totalTicks);
    const last = narrativeSpineTickX(totalTicks - 1, totalTicks);
    expect(first).toBe(20);
    expect(last).toBe(20 + (totalTicks - 1) * tickSpacing);
    expect(last).toBeGreaterThan(first);
    expect(last).toBeLessThanOrEqual(totalWidth);

    const mid = narrativeSpineTickX(500, totalTicks);
    expect(mid).toBeGreaterThan(first);
    expect(mid).toBeLessThan(last);
  });
});
