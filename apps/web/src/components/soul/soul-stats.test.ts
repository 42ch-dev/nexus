/**
 * soul-stats — pure aggregation unit tests (V1.79 P1 SOUL §B/§C).
 *
 * Covers density branching, keyword-frequency aggregation (incl. malformed /
 * non-string keyword data degrading gracefully), and temporal bucketing (sparse
 * data collapses to honest bucket counts; growth folds in cumulatively).
 */
import { describe, expect, it } from 'vitest';

import {
  aggregateKeywordFrequency,
  bucketByTime,
  densityFor,
  fragmentKeywords,
  LOW_DATA_MAX,
} from '@/components/soul/soul-stats';
import type { MemoryFragmentInfo } from '@42ch/nexus-contracts';

function frag(over: Partial<MemoryFragmentInfo> = {}): MemoryFragmentInfo {
  return {
    fragment_id: 'f',
    summary: 's',
    ...over,
  };
}

describe('densityFor', () => {
  it('classifies empty / low-data / rich by the documented thresholds', () => {
    expect(densityFor(0)).toBe('empty');
    expect(densityFor(1)).toBe('low-data');
    expect(densityFor(LOW_DATA_MAX)).toBe('low-data');
    expect(densityFor(LOW_DATA_MAX + 1)).toBe('rich');
  });
});

describe('fragmentKeywords', () => {
  it('returns the keyword array when present', () => {
    expect(fragmentKeywords(frag({ keywords: ['a', 'b'] }))).toEqual(['a', 'b']);
  });

  it('degrades to empty for missing / malformed / non-string items', () => {
    expect(fragmentKeywords(frag())).toEqual([]);
    expect(fragmentKeywords(frag({ keywords: [] }))).toEqual([]);
    // The daemon decodes the JSON server-side, but the client guards against
    // a non-array shape defensively.
    expect(fragmentKeywords(frag({ keywords: undefined }))).toEqual([]);
    expect(
      fragmentKeywords(frag({ keywords: ['ok', '', '  '] as unknown as string[] })),
    ).toEqual(['ok']);
  });
});

describe('aggregateKeywordFrequency', () => {
  it('counts keyword mentions across fragments and sorts by frequency desc', () => {
    const counts = aggregateKeywordFrequency([
      frag({ keywords: ['beta', 'alpha'] }),
      frag({ keywords: ['alpha'] }),
      frag({ keywords: ['gamma', 'alpha'] }),
    ]);
    expect(counts).toEqual([
      { keyword: 'alpha', count: 3 },
      { keyword: 'beta', count: 1 },
      { keyword: 'gamma', count: 1 },
    ]);
  });

  it('breaks ties alphabetically for deterministic output', () => {
    const counts = aggregateKeywordFrequency([
      frag({ keywords: ['zeta', 'alpha'] }),
      frag({ keywords: ['mu'] }),
    ]);
    expect(counts.map((c) => c.keyword)).toEqual(['alpha', 'mu', 'zeta']);
  });

  it('ignores fragments with no keywords', () => {
    expect(aggregateKeywordFrequency([frag(), frag({ keywords: [] })])).toEqual([]);
  });
});

describe('bucketByTime', () => {
  it('returns [] when no fragment has a parseable timestamp', () => {
    expect(bucketByTime([frag(), frag({ created_at: 'not-a-date' })])).toEqual([]);
  });

  it('folds growth in cumulatively and labels buckets', () => {
    const buckets = bucketByTime([
      frag({ created_at: '2026-06-01T00:00:00Z' }),
      frag({ created_at: '2026-06-02T00:00:00Z' }),
      frag({ created_at: '2026-06-03T00:00:00Z' }),
    ]);
    expect(buckets.length).toBeGreaterThanOrEqual(1);
    const last = buckets[buckets.length - 1]!;
    expect(last.cumulative).toBe(3);
    expect(last.newCount).toBeGreaterThanOrEqual(1);
  });

  it('collapses to a single bucket when all fragments share one moment', () => {
    const buckets = bucketByTime([
      frag({ created_at: '2026-06-01T00:00:00Z', keywords: ['x'] }),
      frag({ created_at: '2026-06-01T00:00:00Z', keywords: ['x'] }),
    ]);
    expect(buckets).toHaveLength(1);
    expect(buckets[0]!.cumulative).toBe(2);
    expect(buckets[0]!.newCount).toBe(2);
  });

  it('records per-bucket keyword composition of the NEW fragments', () => {
    const buckets = bucketByTime([
      frag({ created_at: '2026-06-01T00:00:00Z', keywords: ['rising'] }),
      frag({ created_at: '2026-09-01T00:00:00Z', keywords: ['fading', 'newcomer'] }),
    ]);
    const last = buckets[buckets.length - 1]!;
    const kws = last.keywords.map((k) => k.keyword);
    // The later bucket should carry the "newcomer"/"fading" themes, proving the
    // composition shifts across the timeline.
    expect(kws).toContain('fading');
    expect(kws).toContain('newcomer');
  });

  it('caps the bucket count so sparse data does not fabricate empty buckets', () => {
    // 3 distinct days → at most 3 buckets even if targetBuckets asks for more.
    const buckets = bucketByTime(
      [
        frag({ created_at: '2026-06-01T00:00:00Z' }),
        frag({ created_at: '2026-06-02T00:00:00Z' }),
        frag({ created_at: '2026-06-03T00:00:00Z' }),
      ],
      6,
    );
    expect(buckets.length).toBeLessThanOrEqual(3);
    expect(buckets.every((b) => b.newCount >= 1)).toBe(true);
  });
});
