/**
 * Budget block — P1 T2 (DF-76). Read-only.
 *
 * Renders the activation token-budget accounting (chars/4 estimates):
 * `primary_tokens_est`, `hop_tokens_est`, `cap`, `remaining`. Nullable fields
 * (`cap`/`remaining` — null when no activation ran) render as "—". The budget
 * is observed, never modified (AC-I6). No live meter (plan non-goal).
 */
import type { MomentInspectResponse } from '@42ch/nexus-contracts';
import { useTranslation } from 'react-i18next';

export interface BudgetBlockProps {
  budget: MomentInspectResponse['budget'];
}

export function BudgetBlock({ budget }: BudgetBlockProps) {
  const { t } = useTranslation('inspector');
  const none = t('budget.none');

  const rows: { label: string; value: string; testId: string }[] = [
    { label: t('budget.primaryEstLabel'), value: String(budget.primary_tokens_est), testId: 'budget-primary' },
    { label: t('budget.hopEstLabel'), value: String(budget.hop_tokens_est), testId: 'budget-hop' },
    { label: t('budget.capLabel'), value: budget.cap === null ? none : String(budget.cap), testId: 'budget-cap' },
    {
      label: t('budget.remainingLabel'),
      value: budget.remaining === null ? none : String(budget.remaining),
      testId: 'budget-remaining',
    },
  ];

  return (
    <section aria-labelledby="inspector-budget-title" data-testid="budget-block">
      <h3 id="inspector-budget-title" className="text-heading-16 font-heading text-gray-1000">
        {t('budget.title')}
      </h3>
      <p className="text-copy-13 text-gray-700">{t('budget.description')}</p>
      <dl className="mt-2 flex flex-col gap-1.5 text-copy-13">
        {rows.map((row) => (
          <div key={row.testId} className="flex items-center justify-between gap-4">
            <dt className="text-gray-900">{row.label}</dt>
            <dd className="tabular-nums text-gray-1000" data-testid={row.testId}>
              {row.value}
            </dd>
          </div>
        ))}
      </dl>
    </section>
  );
}
