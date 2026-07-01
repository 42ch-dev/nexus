import { useState } from 'react';

import { EmptyState } from '@/components/ui/states';
import type { KeywordCount } from '@/components/soul/soul-stats';

/**
 * Keyword frequency / cluster visualization (V1.79 P1 — SOUL §B).
 *
 * Renders the creator's top creative themes as a frequency list: each row is a
 * keyword whose horizontal bar encodes how often it shows up across the
 * captured fragments. Node size = frequency (the cluster-node token maps to bar
 * width here). Per DESIGN.md `soul-viz-keyword-cluster-node`, the fill/stroke
 * carry the purple accent; the count surfaces on hover (title) and inline.
 *
 * Interaction:
 *  - Hover a row → the count is announced via `title` (tooltip) and inline text.
 *  - Click a keyword → `onSelectKeyword` fires so the parent can filter the
 *    fragments browser (optional; the callback is `undefined` when the parent
 *    does not wire filtering).
 *
 * Density contract: the parent only renders this for `low-data` and `rich`
 * states (never `empty`). When the fragments exist but none carry keywords, an
 * honest inline empty state explains the gap rather than rendering a blank chart.
 */
export function KeywordFrequency({
  counts,
  selectedKeyword,
  onSelectKeyword,
  maxRows = 12,
}: {
  counts: KeywordCount[];
  selectedKeyword?: string | null;
  onSelectKeyword?: (keyword: string | null) => void;
  maxRows?: number;
}) {
  const [hovered, setHovered] = useState<string | null>(null);
  const top = counts.slice(0, maxRows);
  const maxCount = top.length > 0 ? top[0]!.count : 0;

  if (top.length === 0) {
    return (
      <EmptyState
        title="No themes yet"
        description="Your captured fragments do not carry keyword labels yet. Keep reviewing pending captures — themes will accumulate here."
      />
    );
  }

  const selectable = Boolean(onSelectKeyword);
  return (
    <ul className="flex flex-col gap-2" data-testid="soul-keyword-frequency">
      {top.map(({ keyword, count }) => {
        const pct = maxCount > 0 ? Math.max(6, Math.round((count / maxCount) * 100)) : 0;
        const isSelected = selectedKeyword === keyword;
        const isHovered = hovered === keyword;
        return (
          <li key={keyword}>
            <button
              type="button"
              disabled={!selectable}
              onClick={() => {
                if (!selectable || !onSelectKeyword) return;
                onSelectKeyword(isSelected ? null : keyword);
              }}
              onMouseEnter={() => setHovered(keyword)}
              onMouseLeave={() => setHovered((h) => (h === keyword ? null : h))}
              onFocus={() => setHovered(keyword)}
              onBlur={() => setHovered((h) => (h === keyword ? null : h))}
              title={`${count} fragment${count === 1 ? '' : 's'} mention “${keyword}”`}
              aria-pressed={selectable ? isSelected : undefined}
              className={[
                'group flex w-full items-center gap-3 rounded-control px-2 py-1.5 text-left',
                'transition-colors duration-state ease-standard',
                selectable ? 'cursor-pointer hover:bg-background-200 focus-visible:outline-none' : 'cursor-default',
                isSelected ? 'bg-background-300' : '',
              ].join(' ')}
              data-testid="soul-keyword-row"
            >
              <span className="w-[40%] max-w-[220px] truncate text-copy-14 text-gray-1000">
                {keyword}
              </span>
              <span
                className="relative h-2.5 flex-1 overflow-hidden rounded-pill"
                aria-hidden
              >
                {/* soul-viz-keyword-cluster-node fill/stroke (DESIGN.md token). */}
                <span
                  className="block h-full rounded-pill"
                  style={{
                    width: `${pct}%`,
                    backgroundColor: 'var(--color-soul-viz-keyword-cluster-node-fill)',
                    boxShadow: `inset 0 0 0 1px var(--color-soul-viz-keyword-cluster-node-stroke)`,
                    opacity: isSelected || isHovered ? 1 : 0.85,
                  }}
                />
              </span>
              <span className="w-10 shrink-0 text-right text-label-12 tabular-nums text-gray-700">
                {count}
              </span>
            </button>
          </li>
        );
      })}
      {counts.length > maxRows && (
        <li className="px-2 text-copy-13 text-gray-700">
          +{counts.length - maxRows} more theme{counts.length - maxRows === 1 ? '' : 's'}
        </li>
      )}
    </ul>
  );
}
