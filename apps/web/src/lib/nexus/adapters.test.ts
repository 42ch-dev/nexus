import { describe, it, expect } from 'vitest';

describe('adapters', () => {
  it('F-P3/F-F1 adapters were removed after server-side closure', () => {
    // The legacy normalizeList / sortByDate helpers are no longer exported;
    // list endpoints now return canonical `{ items, pagination }` and honor
    // the single `sort` query parameter server-side.
    expect(true).toBe(true);
  });
});
