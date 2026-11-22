/**
 * note: The Canvas's non-graph presentations (D16), all resolved from one
 * `getStrategyApplied` read:
 *
 *   (a) never applied, never approved → Compose is the only action;
 *   (b) approved with no live version → the apply lead, and no Compose;
 *   (c) an apply is recorded but its document no longer verifies → an error.
 *
 * (c) is deliberately not an empty state: rendering it as "not yet applied"
 * would invite an apply that overwrites a ledger the system says is live. It
 * offers neither an import nor an apply action.
 */
import { useTranslation } from 'react-i18next';

import { Button } from '@/components/ui/button';

export type AppliedStateKind = 'never-applied' | 'approved-unapplied' | 'unreadable';

export interface StrategyAppliedStateProps {
  state: AppliedStateKind;
  /** note: (c) only: the refusal's own sentence, naming what no longer verifies. */
  reason?: string;
  /** note: (b) only: the refusal the last apply returned, rendered verbatim. */
  refusal?: string | null;
  onOpenCompose?: () => void;
  onApplyLatest?: () => void;
  onRecheck?: () => void;
}

export function StrategyAppliedState({
  state,
  reason,
  refusal,
  onOpenCompose,
  onApplyLatest,
  onRecheck,
}: StrategyAppliedStateProps) {
  const { t } = useTranslation('canvas');
  // note: literal keys keep the catalogue greppable from the state itself.
  const titles: Record<AppliedStateKind, string> = {
    'never-applied': 'strategy.applied.neverApplied.title',
    'approved-unapplied': 'strategy.applied.approvedUnapplied.title',
    unreadable: 'strategy.applied.unreadable.title',
  };
  const descriptions: Record<AppliedStateKind, string> = {
    'never-applied': 'strategy.applied.neverApplied.description',
    'approved-unapplied': 'strategy.applied.approvedUnapplied.description',
    unreadable: 'strategy.applied.unreadable.description',
  };

  return (
    <section
      data-testid={`applied-state-${state}`}
      aria-labelledby={`applied-state-${state}-title`}
      className="mx-auto flex max-w-md flex-col items-start gap-2 py-16"
    >
      <h2
        id={`applied-state-${state}-title`}
        className="font-display text-display-24 text-gray-1000"
      >
        {t(titles[state])}
      </h2>
      <p className="text-copy-14 text-gray-900">
        {state === 'unreadable' ? t(descriptions[state], { reason }) : t(descriptions[state])}
      </p>
      {refusal ? (
        <p role="alert" className="text-copy-14 text-red-1000">
          {refusal}
        </p>
      ) : null}
      <div className="mt-2">
        {state === 'never-applied' && onOpenCompose ? (
          <Button onClick={onOpenCompose}>{t('strategy.applied.neverApplied.openCompose')}</Button>
        ) : null}
        {state === 'approved-unapplied' && onApplyLatest ? (
          <Button onClick={onApplyLatest}>
            {t('strategy.applied.approvedUnapplied.applyLatest')}
          </Button>
        ) : null}
        {/* note: (c): re-reading is honest recovery and mutates nothing; import and
            apply are what this state must not offer. */}
        {state === 'unreadable' && onRecheck ? (
          <Button variant="secondary" onClick={onRecheck}>
            {t('strategy.applied.unreadable.recheck')}
          </Button>
        ) : null}
      </div>
    </section>
  );
}
