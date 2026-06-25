/**
 * Tests for the F-P3 + F-F1 + cursor adapters (compass §5 #2, §6 residuals).
 *
 * The daemon's list endpoints use heterogeneous array keys (`works`,
 * `sessions`, `schedules`, `capabilities`) until the F-P3 structural rename to
 * `items` lands in V1.66+. The TanStack Query layer normalizes them into a
 * common `{ items, pagination }` shape so screen components consume one type.
 * Findings already uses `items` (new endpoint, convention §4).
 *
 * F-F1 (sort) is deferred server-side; the UI sorts small un-paginated lists
 * client-side. Cursor-paginated lists (Works, Findings) are NOT sorted
 * client-side — the server order is authoritative.
 */
import { describe, expect, it } from 'vitest';

import {
  normalizeList,
  sortByDate,
  type NormalizedList,
} from './adapters';

describe('normalizeList (F-P3 adapter)', () => {
  it('extracts the `works` array from a ListWorksResponse', () => {
    const res = { works: [{ work_id: 'w1' }, { work_id: 'w2' }], pagination: { limit: 20, has_more: false } };
    const out: NormalizedList = normalizeList(res, 'works');
    expect(out.items).toEqual([{ work_id: 'w1' }, { work_id: 'w2' }]);
    expect(out.pagination).toEqual({ limit: 20, has_more: false });
  });

  it('extracts the `items` array as-is (findings already canonical)', () => {
    const res = { items: [{ finding_id: 'f1' }], pagination: { limit: 20, has_more: true, next_cursor: 'c1' } };
    const out = normalizeList(res, 'items');
    expect(out.items).toEqual([{ finding_id: 'f1' }]);
    expect(out.pagination?.next_cursor).toBe('c1');
  });

  it('extracts `sessions` and returns no pagination when the response omits it', () => {
    const res = { sessions: [{ session_id: 's1' }] };
    const out = normalizeList(res, 'sessions');
    expect(out.items).toEqual([{ session_id: 's1' }]);
    expect(out.pagination).toBeUndefined();
  });

  it('extracts `schedules` and `capabilities`', () => {
    expect(normalizeList({ schedules: [{ schedule_id: 'sc1' }] }, 'schedules').items).toEqual([
      { schedule_id: 'sc1' },
    ]);
    expect(normalizeList({ capabilities: [{ name: 'nexus.foo' }] }, 'capabilities').items).toEqual([
      { name: 'nexus.foo' },
    ]);
  });

  it('returns an empty items array when the key is missing', () => {
    const out = normalizeList({}, 'works');
    expect(out.items).toEqual([]);
  });

  it('is tolerant of an already-normalized { items } payload (idempotent)', () => {
    // A response that already conforms to { items, pagination } passes through.
    const res = { items: [{ id: 'x' }] };
    const out = normalizeList(res, 'items');
    expect(out.items).toEqual([{ id: 'x' }]);
  });
});

describe('sortByDate (F-F1 client-side sort)', () => {
  it('sorts ISO date strings newest-first', () => {
    const rows = [
      { id: 'a', updated_at: '2026-06-01T00:00:00Z' },
      { id: 'b', updated_at: '2026-06-24T00:00:00Z' },
      { id: 'c', updated_at: '2026-06-10T00:00:00Z' },
    ];
    const sorted = sortByDate(rows, (r) => r.updated_at);
    expect(sorted.map((r) => r.id)).toEqual(['b', 'c', 'a']);
  });

  it('does not mutate the input array', () => {
    const rows = [
      { id: 'a', t: '2026-06-01T00:00:00Z' },
      { id: 'b', t: '2026-06-24T00:00:00Z' },
    ];
    const snapshot = [...rows];
    sortByDate(rows, (r) => r.t);
    expect(rows).toEqual(snapshot);
  });

  it('pushes rows with a missing date to the end (stable, no throw)', () => {
    const rows = [
      { id: 'a', t: undefined as string | undefined },
      { id: 'b', t: '2026-06-24T00:00:00Z' },
      { id: 'c', t: undefined as string | undefined },
    ];
    const sorted = sortByDate(rows, (r) => r.t);
    // Dated row first, then the two undated (relative order preserved).
    expect(sorted.map((r) => r.id)).toEqual(['b', 'a', 'c']);
  });

  it('returns an empty array unchanged', () => {
    expect(sortByDate([], () => '')).toEqual([]);
  });
});
