import type { TimeBucket } from '@/components/soul/soul-stats';

/**
 * Temporal drift timeline (V1.79 P1 — SOUL §C).
 *
 * A horizontal stepped chart showing how the creator's keyword composition has
 * shifted over time, with the cumulative fragment count folded in (growth is
 * part of the timeline, NOT a separate view). Each step is a time bucket; the
 * stacked bands inside encode the share of the top themes captured in that
 * window. Reading left → right answers "which themes rose, which faded, and how
 * much memory have I accumulated?".
 *
 * Honesty rules (plan §F anti-patterns):
 *  - Never renders a single-point forced chart: the parent only renders this for
 *    the `rich` density state with >=2 time buckets. With one bucket it falls
 *    back to the frequency list.
 *  - Sparse data renders honest step heights — no interpolation, no fake curves.
 *  - A legend maps the top-N band colors to keywords; the cumulative count sits
 *    above each bucket boundary so growth is legible without a second axis.
 *
 * Palette (R-V179P1-QC1-002): the band fills are an ordered token set in the
 * `soul-viz-drift-band` family (DESIGN.md / DESIGN.dark.md), bridged to the
 * `--color-soul-viz-drift-band-fill*` CSS vars in src/index.css. Slot 0 maps to
 * `--color-soul-viz-drift-band-fill` and slots 1..5 to `-fill-2`..`-fill-6`.
 * No RGBA value is hardcoded here; adding a 7th band requires extending the
 * token family + the CSS bridge (the chart caps at the palette length).
 */
const BAND_PALETTE = [
  'var(--color-soul-viz-drift-band-fill)',
  'var(--color-soul-viz-drift-band-fill-2)',
  'var(--color-soul-viz-drift-band-fill-3)',
  'var(--color-soul-viz-drift-band-fill-4)',
  'var(--color-soul-viz-drift-band-fill-5)',
  'var(--color-soul-viz-drift-band-fill-6)',
];

export function TemporalDrift({ buckets }: { buckets: TimeBucket[] }) {
  if (buckets.length < 2) return null;

  const maxNew = Math.max(1, ...buckets.map((b) => b.newCount));
  const legendKeywords = collectLegendKeywords(buckets, 6);
  const total = buckets[buckets.length - 1]!.cumulative;

  return (
    <div data-testid="soul-temporal-drift" className="flex flex-col gap-3">
      <div className="flex items-baseline justify-between">
        <p className="text-copy-13 text-gray-700">
          {total} fragment{total === 1 ? '' : 's'} captured over time
        </p>
        <ul className="flex flex-wrap items-center justify-end gap-x-3 gap-y-1">
          {legendKeywords.map((kw, i) => (
            <li key={kw} className="flex items-center gap-1.5 text-label-12 text-gray-900">
              <span
                aria-hidden
                className="inline-block h-2.5 w-2.5 rounded-pill"
                style={{
                  backgroundColor: BAND_PALETTE[i % BAND_PALETTE.length],
                  boxShadow: `inset 0 0 0 1px var(--color-soul-viz-drift-band-step-stroke)`,
                }}
              />
              {kw}
            </li>
          ))}
        </ul>
      </div>

      <div className="relative flex h-40 items-end gap-1.5">
        {buckets.map((b) => {
          const heightPct = Math.round((b.newCount / maxNew) * 100);
          return (
            <div
              key={b.index}
              className="group relative flex h-full flex-1 flex-col justify-end"
              title={`${b.label}: +${b.newCount} (cumulative ${b.cumulative})`}
              data-testid="soul-drift-bucket"
            >
              {/* Cumulative count label above each bucket (growth fold-in). */}
              <span className="absolute -top-0 left-0 right-0 text-center text-label-12 tabular-nums text-gray-900">
                {b.cumulative}
              </span>
              {/* Stacked bands: top keyword composition of the NEW fragments. */}
              <div
                className="flex w-full flex-col-reverse overflow-hidden rounded-control"
                style={{
                  height: `${heightPct}%`,
                  minHeight: b.newCount > 0 ? '4px' : '0',
                  boxShadow: `inset 0 0 0 1px var(--color-soul-viz-drift-band-step-stroke)`,
                }}
              >
                {b.newCount > 0 &&
                  b.keywords.slice(0, BAND_PALETTE.length).map(({ keyword, count }, i) => {
                    const share = (count / b.newCount) * 100;
                    const inLegend = legendKeywords.indexOf(keyword);
                    const colorIdx = inLegend === -1 ? i : inLegend;
                    return (
                      <span
                        key={keyword}
                        className="block w-full"
                        style={{
                          height: `${share}%`,
                          backgroundColor: BAND_PALETTE[colorIdx % BAND_PALETTE.length],
                        }}
                        title={`${keyword}: ${count}`}
                      />
                    );
                  })}
              </div>
              {/* soul-viz-timeline-axis label (DESIGN.md token). */}
              <span className="mt-1 text-center text-label-12 tabular-nums text-gray-700">
                {b.label}
              </span>
            </div>
          );
        })}
      </div>
      <p className="text-copy-13 text-gray-700">
        Band heights show which themes each window captured; the count above each
        bar is your cumulative memory size.
      </p>
    </div>
  );
}

/**
 * Pick the overall top-N keywords (by total mentions across all buckets) so the
 * legend colors stay stable and meaningful across the whole timeline. A keyword
 * that only appears in one bucket but is globally rare still gets a stable slot
 * if it ranks in the top N overall.
 */
function collectLegendKeywords(buckets: TimeBucket[], topN: number): string[] {
  const totals = new Map<string, number>();
  for (const b of buckets) {
    for (const { keyword, count } of b.keywords) {
      totals.set(keyword, (totals.get(keyword) ?? 0) + count);
    }
  }
  return [...totals.entries()]
    .sort((a, c) => c[1] - a[1] || a[0].localeCompare(c[0]))
    .slice(0, topN)
    .map(([kw]) => kw);
}
