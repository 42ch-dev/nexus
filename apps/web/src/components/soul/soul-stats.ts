/**
 * SOUL personality visualization — pure aggregation helpers.
 *
 * V1.79 P1: these functions turn the read-only `MemoryFragmentInfo` list
 * (carrying `keywords` + `created_at` as of the additive DTO extension) into the
 * shapes the keyword-frequency and temporal-drift surfaces render. They are pure
 * so the density-state branching (empty / low-data / rich) and the chart math can
 * be unit-tested without a DOM.
 *
 * Scope: per-creator (the fragments list is already `creator_id`-scoped by the
 * daemon). World filtering is a future enhancement (plan §Out of scope).
 */

import type { MemoryFragmentInfo } from '@42ch/nexus-contracts';

/**
 * Density state drives which viz treatment renders (plan §F).
 *
 * - `empty`    — zero fragments: empathetic forward-looking empty state, no chart.
 * - `low-data` — 1..=LOW_DATA_MAX fragments: a simple frequency list (a forced
 *                cluster chart with one node would look broken).
 * - `rich`     — more than LOW_DATA_MAX: full clusters + temporal drift timeline.
 */
export const LOW_DATA_MAX = 20;

export type SoulDensity = 'empty' | 'low-data' | 'rich';

/** Resolve the density state from the live fragment count. */
export function densityFor(count: number): SoulDensity {
  if (count <= 0) return 'empty';
  if (count <= LOW_DATA_MAX) return 'low-data';
  return 'rich';
}

/**
 * Flatten a fragment's optional `keywords` into a guaranteed `string[]`.
 * Malformed/absent data degrades to an empty list (the daemon already decodes
 * the JSON array server-side; this is a defensive client guard).
 */
export function fragmentKeywords(f: MemoryFragmentInfo): string[] {
  if (!Array.isArray(f.keywords)) return [];
  return f.keywords.filter(
    (k): k is string => typeof k === 'string' && k.trim().length > 0,
  );
}

export interface KeywordCount {
  keyword: string;
  count: number;
}

/**
 * Aggregate keyword frequency across a creator's fragments, sorted by descending
 * count (ties broken alphabetically for deterministic output). A keyword that
 * appears on N fragments counts N times — this is "how often the theme shows up
 * in the captured memory", the frequency the cluster node size encodes.
 */
export function aggregateKeywordFrequency(
  fragments: MemoryFragmentInfo[],
): KeywordCount[] {
  const counts = new Map<string, number>();
  for (const f of fragments) {
    for (const kw of fragmentKeywords(f)) {
      counts.set(kw, (counts.get(kw) ?? 0) + 1);
    }
  }
  return [...counts.entries()]
    .map(([keyword, count]) => ({ keyword, count }))
    .sort((a, b) => (b.count - a.count) || a.keyword.localeCompare(b.keyword));
}

export interface TimeBucket {
  /** 0-based index of this bucket within the timeline. */
  index: number;
  /** Short human label for the bucket's start boundary (e.g. "Jun 1"). */
  label: string;
  /** Number of NEW fragments captured in this bucket. */
  newCount: number;
  /** Cumulative fragment count through the END of this bucket (growth fold-in). */
  cumulative: number;
  /** Keyword composition of the NEW fragments in this bucket, sorted desc. */
  keywords: KeywordCount[];
}

/**
 * Bucket fragments by `created_at` into `targetBuckets` equal-time intervals for
 * the temporal-drift timeline. Fragments with an unparseable timestamp are
 * dropped from the timeline (they still count toward the keyword-frequency view).
 *
 * The timeline is honest about sparse data: if there are fewer distinct moments
 * than `targetBuckets`, the actual bucket count shrinks so no empty bucket is
 * fabricated. Each bucket carries the cumulative count (growth folded in) plus
 * the keyword composition of the NEW fragments captured in that window — that
 * composition shift over time is the "drift" insight.
 *
 * Returns `[]` when there is no parseable temporal data (the caller renders the
 * low-data fallback instead of a broken single-point chart).
 */
export function bucketByTime(
  fragments: MemoryFragmentInfo[],
  targetBuckets = 6,
): TimeBucket[] {
  const stamped = fragments
    .map((f) => ({ f, ms: safeParseMs(f.created_at) }))
    .filter((s): s is { f: MemoryFragmentInfo; ms: number } => s.ms !== null)
    .sort((a, b) => a.ms - b.ms);
  if (stamped.length === 0) return [];

  const minMs = stamped[0]!.ms;
  const maxMs = stamped[stamped.length - 1]!.ms;
  if (maxMs < minMs) return [];

  // Collapse to a single bucket when all fragments share one moment or the span
  // is too small to divide meaningfully — avoids div-by-zero and empty buckets.
  const span = maxMs - minMs;
  const bucketCount = span === 0 ? 1 : Math.max(1, Math.min(targetBuckets, distinctDays(stamped)));
  const step = span === 0 ? 1 : span / bucketCount;

  const buckets: TimeBucket[] = Array.from({ length: bucketCount }, (_, index) => ({
    index,
    label: bucketLabel(minMs + index * step),
    newCount: 0,
    cumulative: 0,
    keywords: [],
  }));

  const perBucketKeywords: Map<string, number>[] = buckets.map(() => new Map());
  for (const { f, ms } of stamped) {
    const idx = span === 0 ? 0 : Math.min(bucketCount - 1, Math.floor((ms - minMs) / step));
    const b = buckets[idx]!;
    b.newCount += 1;
    const kwMap = perBucketKeywords[idx]!;
    for (const kw of fragmentKeywords(f)) {
      kwMap.set(kw, (kwMap.get(kw) ?? 0) + 1);
    }
  }

  let running = 0;
  buckets.forEach((b, i) => {
    running += b.newCount;
    b.cumulative = running;
    b.keywords = [...perBucketKeywords[i]!.entries()]
      .map(([keyword, count]) => ({ keyword, count }))
      .sort((a, c) => c.count - a.count || a.keyword.localeCompare(c.keyword));
  });
  return buckets;
}

/** Parse an ISO timestamp to epoch-ms, or `null` if unparseable. */
function safeParseMs(iso: string | undefined | null): number | null {
  if (!iso) return null;
  const ms = Date.parse(iso);
  return Number.isNaN(ms) ? null : ms;
}

/** Count distinct calendar days in the stamped set (caps over-bucketing). */
function distinctDays(stamped: { ms: number }[]): number {
  const days = new Set<string>();
  for (const { ms } of stamped) {
    const d = new Date(ms);
    days.add(`${d.getFullYear()}-${d.getMonth()}-${d.getDate()}`);
  }
  return days.size;
}

/** Short locale label for a bucket boundary (e.g. "Jun 14"). */
function bucketLabel(ms: number): string {
  return new Intl.DateTimeFormat(undefined, { month: 'short', day: 'numeric' }).format(ms);
}

// ── Growth-curve helpers (V1.81 SP-3) ────────────────────────────────────────

/**
 * Growth-curve density thresholds (plan §2.3). These are deliberately tighter
 * than the V1.79 keyword/drift `LOW_DATA_MAX` (20): the growth-curve answers
 * "how much have I accumulated?" and degrades to simpler charts rather than
 * gating entirely, so a smaller rich threshold still renders a meaningful
 * cumulative line. The narrative insufficient-data gate is a separate concern
 * (server-side, see `SoulNarrativeResponse` threshold fields).
 */
export const GROWTH_RICH_FRAGMENT_THRESHOLD = 10;
export const GROWTH_RICH_DAY_THRESHOLD = 5;
export const GROWTH_LOW_DATA_MAX_FRAGMENT = 9;

export type GrowthDensity = 'empty' | 'low-data' | 'rich';

export interface GrowthDensityInput {
  /** Total fragment count (stamped + unstamped). */
  fragmentCount: number;
  /** Distinct calendar days among fragments that carry a parseable timestamp. */
  distinctDays: number;
}

/**
 * Resolve the growth-curve density state. Mirrors the V1.79 `densityFor`
 * branching pattern (pure predicate → render branch) but with growth-specific
 * thresholds: `empty` (0 fragments), `low-data` (1–9 fragments), `rich`
 * (≥10 fragments OR ≥5 distinct days). See plan §2.3.
 */
export function growthDensityFor(input: GrowthDensityInput): GrowthDensity {
  if (input.fragmentCount <= 0) return 'empty';
  if (
    input.fragmentCount >= GROWTH_RICH_FRAGMENT_THRESHOLD ||
    input.distinctDays >= GROWTH_RICH_DAY_THRESHOLD
  ) {
    return 'rich';
  }
  return 'low-data';
}

export interface GrowthPoint {
  /** Short human label for the day (e.g. "Jun 1"). */
  label: string;
  /** Cumulative fragment count through the end of this day (growth). */
  cumulative: number;
}

export interface GrowthSeries {
  /** One cumulative point per distinct day, in chronological order. */
  points: GrowthPoint[];
  /** Distinct calendar days among parseable timestamps. */
  distinctDays: number;
}

/**
 * Build the cumulative-growth series: one point per distinct calendar day,
 * carrying the running fragment total. Fragments with an unparseable timestamp
 * are dropped from the timeline (they still count toward density via
 * `fragmentCount`, passed separately by the caller). Returns an empty `points`
 * array when no timestamp parses; the caller renders the `empty`/`low-data`
 * fallback instead of a zero-point chart.
 */
export function growthSeries(fragments: MemoryFragmentInfo[]): GrowthSeries {
  const perDay = new Map<string, { ms: number; added: number }>();
  for (const f of fragments) {
    const ms = safeParseMs(f.created_at);
    if (ms === null) continue;
    const d = new Date(ms);
    const key = `${d.getFullYear()}-${d.getMonth()}-${d.getDate()}`;
    const existing = perDay.get(key);
    if (existing) {
      existing.added += 1;
      if (ms < existing.ms) existing.ms = ms;
    } else {
      perDay.set(key, { ms, added: 1 });
    }
  }
  const sorted = [...perDay.values()].sort((a, b) => a.ms - b.ms);
  let running = 0;
  const points: GrowthPoint[] = sorted.map((d) => {
    running += d.added;
    return { label: bucketLabel(d.ms), cumulative: running };
  });
  return { points, distinctDays: sorted.length };
}
