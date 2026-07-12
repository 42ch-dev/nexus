import type { TimeBucket } from '@/components/soul/soul-stats';
import { useTranslation } from 'react-i18next';

const BAND_PALETTE = [
  'var(--color-soul-viz-drift-band-fill)',
  'var(--color-soul-viz-drift-band-fill-2)',
  'var(--color-soul-viz-drift-band-fill-3)',
  'var(--color-soul-viz-drift-band-fill-4)',
  'var(--color-soul-viz-drift-band-fill-5)',
  'var(--color-soul-viz-drift-band-fill-6)',
];

export function TemporalDrift({ buckets }: { buckets: TimeBucket[] }) {
  const { t } = useTranslation('memory');
  if (buckets.length < 2) return null;

  const maxNew = Math.max(1, ...buckets.map((b) => b.newCount));
  const legendKeywords = collectLegendKeywords(buckets, 6);
  const total = buckets[buckets.length - 1]!.cumulative;

  return (
    <div data-testid="soul-temporal-drift" className="flex flex-col gap-3">
      <div className="flex items-baseline justify-between">
        <p className="text-copy-13 text-gray-700">
          {t('soul.capturedOverTime', { count: total })}
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
              <span className="absolute -top-0 left-0 right-0 text-center text-label-12 tabular-nums text-gray-900">
                {b.cumulative}
              </span>
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
              <span className="mt-1 text-center text-label-12 tabular-nums text-gray-700">
                {b.label}
              </span>
            </div>
          );
        })}
      </div>
      <p className="text-copy-13 text-gray-700">
        {t('soul.bandHint')}
      </p>
    </div>
  );
}

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
