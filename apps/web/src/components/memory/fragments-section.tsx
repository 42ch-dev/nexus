/**
 * FragmentsSection — extracted from MemoryPage (R-V179P1-QC1-001).
 *
 * Read-only long-term-memory fragment browser with an optional keyword filter.
 * Controlled: the page shell lifts the keyword so the SOUL viz click-to-filter
 * can drive this section (V1.79 P1 §D integration). Owns its fragments query +
 * empty/error/loading states; the page shell owns section composition.
 */
import { Label } from '@/components/ui/label';
import { EmptyState, ErrorState, LoadingState } from '@/components/ui/states';
import { useMemoryFragments } from '@/api/queries';
import { useTranslation } from 'react-i18next';

export function FragmentsSection({
  creatorId,
  keyword,
  onKeywordChange,
}: {
  creatorId: string;
  keyword: string;
  onKeywordChange: (next: string) => void;
}) {
  const { t } = useTranslation('memory');
  const trimmed = keyword.trim();
  const fragments = useMemoryFragments(creatorId, trimmed ? { keyword: trimmed } : undefined);
  const rows = fragments.data?.fragments ?? [];

  return (
    <section data-testid="memory-fragments-section">
      <div className="mb-3 flex flex-wrap items-center justify-between gap-2">
        <h2 className="text-heading-16 text-gray-1000">{t('fragments.title')}</h2>
        <p className="text-copy-13 text-gray-700">
          {t('fragments.description')}
        </p>
      </div>
      <div className="mb-4 flex flex-col gap-1.5">
        <Label htmlFor="memory-fragment-filter">{t('fragments.filterLabel')}</Label>
        <input
          id="memory-fragment-filter"
          type="search"
          value={keyword}
          onChange={(e) => onKeywordChange(e.target.value)}
          placeholder={t('fragments.filterPlaceholder')}
          className="h-10 w-full max-w-[320px] rounded-control border border-gray-alpha-400 bg-background-100 px-3 text-copy-14 text-gray-1000 placeholder:text-gray-700"
        />
      </div>
      {fragments.isError ? (
        <ErrorState description={t('fragments.error')} onRetry={() => fragments.refetch()} />
      ) : fragments.isLoading ? (
        <LoadingState label={t('fragments.loading')} />
      ) : rows.length === 0 ? (
        <EmptyState
          title={t('fragments.emptyTitle')}
          description={trimmed ? t('fragments.emptyFiltered') : t('fragments.emptyDefault')}
        />
      ) : (
        <ul className="flex flex-col divide-y divide-gray-alpha-400 rounded-control border border-gray-alpha-400">
          {rows.map((f) => (
            <li
              key={f.fragment_id}
              className="flex flex-col gap-1 px-3 py-2.5"
              data-testid="memory-fragment-row"
            >
              <div className="flex items-center gap-2">
                <span className="text-copy-13-mono text-gray-800">{f.fragment_id}</span>
              </div>
              <p className="whitespace-pre-wrap break-words text-copy-14 leading-[1.5] text-gray-1000">
                {f.summary}
              </p>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
