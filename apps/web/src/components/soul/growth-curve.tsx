import type { MemoryFragmentInfo } from '@42ch/nexus-contracts';

import { EmptyState } from '@/components/ui/states';
import {
  GROWTH_LOW_DATA_MAX_FRAGMENT,
  growthDensityFor,
  growthSeries,
  type GrowthPoint,
} from '@/components/soul/soul-stats';

/**
 * Growth-curve (V1.81 SP-3 — web-ui.md §26.1).
 *
 * Cumulative fragment count over time as a simple line/area chart, independent
 * of the temporal-drift timeline (which answers "how has my focus shifted?" —
 * this answers "how much have I accumulated?"). Respects the world projection:
 * the parent passes the world-scoped fragment subset.
 *
 * Three density states (plan §2.3, reusing the V1.79 `densityFor` branching
 * pattern with growth-specific thresholds from soul-stats):
 *  - `empty`    — 0 fragments: forward-looking illustration, no chart.
 *  - `low-data` — 1–9 fragments (and <5 days): a simple cumulative line.
 *  - `rich`     — ≥10 fragments OR ≥5 distinct days: full curve with axis
 *                 labels and a summary stat.
 *
 * Unlike the narrative card (which gates entirely below the quality threshold),
 * the growth-curve always renders a chart for non-empty states — it degrades to
 * a simpler form rather than hiding. The stroke uses the DESIGN.md
 * `soul-growth-curve-stroke` token (`--color-soul-growth-curve-stroke`).
 */
export function GrowthCurve({ fragments }: { fragments: MemoryFragmentInfo[] }) {
  const series = growthSeries(fragments);
  const density = growthDensityFor({
    fragmentCount: fragments.length,
    distinctDays: series.distinctDays,
  });

  if (density === 'empty') {
    return (
      <div data-testid="soul-growth-empty">
        <EmptyState
          title="Your SOUL begins here"
          description="Every review session adds a fragment to your creative growth."
        />
      </div>
    );
  }

  if (density === 'low-data') {
    return (
      <div data-testid="soul-growth-low-data" className="flex flex-col gap-3">
        <p className="text-copy-13 text-gray-700">
          Your SOUL is taking shape. Keep writing to see your growth curve emerge.
        </p>
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
          {total} fragment{total === 1 ? '' : 's'} over {series.distinctDays} day{series.distinctDays === 1 ? '' : 's'}
        </p>
      </div>
      <GrowthLineChart points={series.points} rich />
    </div>
  );
}

/**
 * Lightweight cumulative line chart (no charting dependency). Renders an SVG
 * polyline scaled to the bucket dimensions, with a baseline area fill. `rich`
 * adds axis labels (first/last day) so the timespan is legible; `low-data`
 * renders the line + a quiet inline summary to avoid over-labeling a thin span.
 */
function GrowthLineChart({ points, rich }: { points: GrowthPoint[]; rich: boolean }) {
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
        aria-label="Cumulative fragment growth over time"
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
          {points.length} day{points.length === 1 ? '' : 's'} of growth so far.
        </p>
      )}
    </div>
  );
}

// Re-export the low-data ceiling so tests can assert the density boundary
// without importing the constant from two places.
export { GROWTH_LOW_DATA_MAX_FRAGMENT };
