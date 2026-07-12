import { useState } from 'react';
import { useTranslation } from 'react-i18next';

import { EmptyState } from '@/components/ui/states';
import type { KeywordCount } from '@/components/soul/soul-stats';

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
  const { t } = useTranslation('memory');
  const [hovered, setHovered] = useState<string | null>(null);
  const top = counts.slice(0, maxRows);
  const maxCount = top.length > 0 ? top[0]!.count : 0;

  if (top.length === 0) {
    return (
      <EmptyState
        title={t('soul.noThemesTitle')}
        description={t('soul.noThemesDescription')}
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
              title={t('soul.fragmentCount', { count })}
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
          {t('soul.moreThemes', { count: counts.length - maxRows })}
        </li>
      )}
    </ul>
  );
}
