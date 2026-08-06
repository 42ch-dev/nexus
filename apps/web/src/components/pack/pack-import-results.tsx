/**
 * Pack import results — summary counts + collapsible per-atom details
 * (V1.152 P1 T2, DF-77). Pure presentation.
 *
 * Renders the `PackImportResponse` returned by `useImportPack`: the two
 * `AtomCounts` blocks (entries / relations) and a native `<details>`
 * disclosure with the per-atom `details` list (kind / id / outcome /
 * optional reason).
 *
 * A11y (WCAG 2.1 AA, brief T4): the headline summary is mirrored into a
 * visually-hidden `role="status"` live region that mounts with the results,
 * so screen readers announce one concise sentence instead of the full table.
 * The visible blocks sit outside the live region (no re-announcement noise
 * while the author reads); the `<details>` disclosure is natively keyboard
 * accessible (enter/space toggles) and each row labels kind + outcome with
 * visible text, not color alone.
 */
import { useId } from 'react';
import { useTranslation } from 'react-i18next';

import type { PackImportResponse } from '@42ch/nexus-contracts';

export interface PackImportResultsProps {
  summary: PackImportResponse;
}

type AtomCounts = PackImportResponse['entries'];
type ImportDetail = PackImportResponse['details'][number];

const OUTCOME_KEYS = ['created', 'skipped', 'rejected', 'renamed', 'overwritten'] as const;
type OutcomeKey = (typeof OUTCOME_KEYS)[number];

function AtomCountsBlock({ label, counts }: { label: string; counts: AtomCounts }) {
  const { t } = useTranslation('pack');
  return (
    <div className="flex flex-col gap-1.5" data-testid="pack-atom-counts">
      <h4 className="text-label-14 font-semibold text-gray-900">{label}</h4>
      <dl className="grid grid-cols-2 gap-x-4 gap-y-1 sm:grid-cols-5">
        {OUTCOME_KEYS.map((key: OutcomeKey) => (
          <div key={key} className="flex items-baseline gap-1.5">
            <dt className="text-copy-13 text-gray-700">{t(`results.outcomes.${key}`)}</dt>
            <dd className="text-copy-13 font-medium text-gray-1000" data-testid={`pack-count-${key}`}>
              {counts[key]}
            </dd>
          </div>
        ))}
      </dl>
    </div>
  );
}

export function PackImportResults({ summary }: PackImportResultsProps) {
  const { t } = useTranslation('pack');
  const titleId = useId();

  // Compact "3 created, 1 overwritten" sentence per atom group for the live
  // region; only non-zero outcomes are listed so the announcement stays short.
  const summarize = (counts: AtomCounts): string => {
    const parts = OUTCOME_KEYS.filter((key) => counts[key] > 0).map((key) =>
      t(`results.liveCount.${key}`, { count: counts[key] }),
    );
    return parts.length > 0 ? parts.join(', ') : t('results.nothing');
  };

  return (
    <section
      aria-labelledby={titleId}
      className="flex flex-col gap-2"
      data-testid="pack-import-results"
    >
      <h3 id={titleId} className="text-heading-16 font-heading text-gray-1000">
        {t('results.title')}
      </h3>
      <div
        className="sr-only"
        role="status"
        aria-live="polite"
        aria-atomic="true"
        data-testid="pack-results-live"
      >
        {t('results.liveSummary', {
          entries: summarize(summary.entries),
          relations: summarize(summary.relations),
        })}
      </div>
      <div className="grid gap-4 sm:grid-cols-2">
        <AtomCountsBlock label={t('results.entries')} counts={summary.entries} />
        <AtomCountsBlock label={t('results.relations')} counts={summary.relations} />
      </div>
      <details className="group" data-testid="pack-import-details">
        <summary className="inline-flex cursor-pointer list-none items-center gap-1.5 self-start rounded-control px-2 py-1 text-button-12 text-gray-1000 hover:bg-gray-alpha-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-700 focus-visible:ring-offset-2 [&::-webkit-details-marker]:hidden">
          <span aria-hidden className="inline-block transition-transform group-open:rotate-90">
            ▸
          </span>
          {summary.details.length > 0
            ? t('results.detailsToggleWithCount', { count: summary.details.length })
            : t('results.detailsToggle')}
        </summary>
        {summary.details.length === 0 ? (
          <p className="mt-2 text-copy-13 text-gray-700">{t('results.detailsEmpty')}</p>
        ) : (
          <ul className="mt-2 flex flex-col gap-1.5">
            {summary.details.map((detail: ImportDetail) => (
              <li
                key={`${detail.kind}-${detail.id}`}
                className="flex flex-col gap-0.5 rounded-card border border-gray-alpha-300 px-3 py-2"
                data-testid="pack-detail-row"
              >
                <div className="flex items-center gap-2">
                  <span className="shrink-0 rounded-pill bg-gray-alpha-100 px-2 py-0.5 text-label-12 text-gray-700">
                    {t(`results.kind.${detail.kind}`)}
                  </span>
                  <span className="min-w-0 flex-1 truncate font-mono text-copy-13 text-gray-900">
                    {detail.id}
                  </span>
                  <span className="shrink-0 rounded-pill bg-blue-1000/10 px-2 py-0.5 text-label-12 text-blue-1000 dark:bg-blue-700/10 dark:text-blue-700">
                    {t(`results.outcomes.${detail.outcome}`)}
                  </span>
                </div>
                {detail.reason ? (
                  <p className="text-copy-13 text-gray-700">{detail.reason}</p>
                ) : null}
              </li>
            ))}
          </ul>
        )}
      </details>
    </section>
  );
}
