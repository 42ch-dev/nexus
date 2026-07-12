/**
 * World KB canvas header (V1.74 A10 split).
 *
 * Title, entry count, last-fetched staleness, view toggle, and refresh action.
 */
import { useTranslation } from 'react-i18next';
import { List, RefreshCw, Workflow } from 'lucide-react';

import { Button } from '@/components/ui/button';

interface WorldKbHeaderProps {
  entryCount: number;
  lastFetched: string;
  showList: boolean;
  onToggleView: () => void;
  onRefresh: () => void;
  refreshing: boolean;
}

export function WorldKbHeader({
  entryCount,
  lastFetched,
  showList,
  onToggleView,
  onRefresh,
  refreshing,
}: WorldKbHeaderProps) {
  const { t } = useTranslation('canvas');
  return (
    <div className="flex flex-wrap items-center justify-between gap-2">
      <div>
        <div className="flex items-center gap-2">
          <h2 className="text-heading-20 font-heading text-gray-1000">{t('worldKb.header.title')}</h2>
          {entryCount === 0 ? (
            <span className="rounded-pill bg-gray-alpha-100 px-2 py-0.5 text-label-12 text-gray-700">
              {t('worldKb.header.noEntries')}
            </span>
          ) : (
            <span className="rounded-pill bg-gray-alpha-100 px-2 py-0.5 text-label-12 text-gray-700">
              {t('worldKb.header.entryCount', { count: entryCount })} · {t('worldKb.header.fetched', { time: lastFetched })}
            </span>
          )}
        </div>
        <p className="text-copy-13 text-gray-700">
          {entryCount === 0
            ? t('worldKb.header.emptyDescription')
            : t('worldKb.header.description')}
        </p>
      </div>
      <div className="flex items-center gap-2">
        <Button
          type="button"
          variant="tertiary"
          size="small"
          onClick={onToggleView}
          aria-pressed={showList}
        >
          {showList ? (
            <>
              <Workflow className="h-4 w-4" aria-hidden /> {t('worldKb.header.showGraph')}
            </>
          ) : (
            <>
              <List className="h-4 w-4" aria-hidden /> {t('worldKb.header.showList')}
            </>
          )}
        </Button>
        <Button type="button" variant="secondary" size="small" onClick={onRefresh} disabled={refreshing}>
          <RefreshCw className={`h-4 w-4 ${refreshing ? 'animate-spin' : ''}`} aria-hidden />
          {t('worldKb.header.refresh')}
        </Button>
      </div>
    </div>
  );
}
