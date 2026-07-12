import type { MemoryFragmentInfo } from '@42ch/nexus-contracts';
import { useTranslation } from 'react-i18next';

import { EmptyState } from '@/components/ui/states';
import {
  GROWTH_LOW_DATA_MAX_FRAGMENT,
  growthDensityFor,
  growthSeries,
  type GrowthPoint,
} from '@/components/soul/soul-stats';

export function GrowthCurve({ fragments }: { fragments: MemoryFragmentInfo[] }) {
  const { t } = useTranslation('memory');
  const series = growthSeries(fragments);
  const density = growthDensityFor({
    fragmentCount: fragments.length,
    distinctDays: series.distinctDays,
  });

  if (density === 'empty') {
    return (
      <div data-testid="soul-growth-empty">
        <EmptyState
          title={t('growth.emptyTitle')}
          description={t('growth.emptyDescription')}
        />
      </div>
    );
  }

  if (density === 'low-data') {
    return (
      <div data-testid="soul-growth-low-data" className="flex flex-col gap-3">
        <p className="text-copy-13 text-gray-700">{t('growth.lowData')}</p>
        <GrowthLineChart points={series.points} rich={false} />
      </div>
    );
  }

  // rich
  const total = series.points[series.points.length - 1]?.cumulative ?? fragments.length;
  return (
    <div data-testid="soul-growth-rich" className="flex flex-col gap-3">
      <div className="flex items-baseline justify-between">
        <p className="text-copy-13 text-gray-700">
          {t('growth.summary', { count: total, days: series.distinctDays })}
        </p>
      </div>
      <GrowthLineChart points={series.points} rich />
    </div>
  );
}

function GrowthLineChart({ points, rich }: { points: GrowthPoint[]; rich: boolean }) {
  const { t } = useTranslation('memory');
  if (points.length === 0) return null;
  const width = 100;
  const height = 40;
  const max = points[points.length - 1]!.cumulative;
  if (max <= 0) return null;

  const stepX = points.length > 1 ? width / (points.length - 1) : 0;
  const coords = points.map((p, i) => {
    const x = points.length > 1 ? i * stepX : width / 2;
    const y = height - (p.cumulative / max) * height;
    return [x, y] as const;
  });
  const line = coords.map(([x, y]) => `${x.toFixed(2)},${y.toFixed(2)}`).join(' ');
  const area = `0,${height} ${line} ${width},${height}`;

  return (
    <div className="flex flex-col gap-1">
      <svg
        viewBox={`0 0 ${width} ${height}`}
        preserveAspectRatio="none"
        className="h-24 w-full"
        role="img"
        aria-label={t('growth.chartAriaLabel')}
      >
        <polygon points={area} fill="var(--color-soul-growth-curve-stroke)" opacity={0.14} />
        <polyline
          points={line}
          fill="none"
          stroke="var(--color-soul-growth-curve-stroke)"
          strokeWidth={1.5}
          strokeLinejoin="round"
          strokeLinecap="round"
        />
      </svg>
      {rich ? (
        <div className="flex justify-between text-label-12 tabular-nums text-gray-700">
          <span>{points[0]!.label}</span>
          <span>{points[points.length - 1]!.label}</span>
        </div>
      ) : (
        <p className="text-label-12 text-gray-700">
          {t('growth.daysOfGrowth', { count: points.length })}
        </p>
      )}
    </div>
  );
}

export { GROWTH_LOW_DATA_MAX_FRAGMENT };
