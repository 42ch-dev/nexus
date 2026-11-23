/**
 *
 * Two entrance cards — the choice IS the entrance, which selects the rail
 * registry (`appsForEntrance`) used after the wizard finishes.
 */
import { useTranslation } from 'react-i18next';

import type { EntranceId } from '@/components/layout/entrance-registry';
import { Button } from '@/components/ui/button';
import { cn } from '@/lib/utils';

interface IdentityOption {
  id: EntranceId;
  emoji: string;
  copyKey: 'contentCreator' | 'developer';
  recommended?: boolean;
}

const OPTIONS: readonly IdentityOption[] = [
  {
    id: 'content-creator',
    emoji: '✍️',
    copyKey: 'contentCreator',
    recommended: true,
  },
  {
    id: 'developer',
    emoji: '🛠️',
    copyKey: 'developer',
  },
];

interface StepIdentityProps {
  value: EntranceId | null;
  onChange: (value: EntranceId) => void;
  onNext: () => void;
}

export function StepIdentity({ value, onChange, onNext }: StepIdentityProps) {
  const { t } = useTranslation('setup');
  return (
    <div className="flex flex-col items-center">
      <div className="text-label-12 text-gray-700">{t('wizard.identity.eyebrow')}</div>
      <h1 className="mt-4 font-display text-display-32 text-gray-1000">{t('wizard.identity.title')}</h1>
      <p className="mt-3 max-w-[560px] text-center text-copy-14 text-gray-900">
        {t('wizard.identity.description')}
      </p>

      <div
        role="radiogroup"
        aria-label={t('wizard.identity.optionsLabel')}
        className="mt-10 flex flex-col gap-6 sm:flex-row sm:gap-8"
      >
        {OPTIONS.map((option) => {
          const selected = value === option.id;
          return (
            <button
              key={option.id}
              type="button"
              role="radio"
              aria-checked={selected}
              onClick={() => onChange(option.id)}
              data-testid={`identity-option-${option.id}`}
              className={cn(
                'relative flex w-[320px] flex-col items-center rounded-card border-2 bg-background-100 px-7 pb-8 pt-10 text-center transition-colors duration-state ease-standard motion-reduce:transition-none',
                selected
                  ? 'border-blue-700'
                  : 'border-gray-alpha-400 hover:border-blue-700',
              )}
            >
              {option.recommended && (
                <span className="absolute -top-3 rounded-pill bg-blue-700 px-3.5 py-1 text-label-12 font-semibold text-brand-white dark:text-brand-deep-blue">
                  {t('wizard.identity.recommended')}
                </span>
              )}
              <span aria-hidden="true" className="text-4xl">
                {option.emoji}
              </span>
              <span className="mt-5 text-heading-20 font-heading text-gray-1000">{t(`wizard.identity.option.${option.copyKey}.title`)}</span>
              <span className="mt-2.5 text-copy-13 text-gray-900">{t(`wizard.identity.option.${option.copyKey}.description`)}</span>
            </button>
          );
        })}
      </div>

      <div className="mt-12 flex w-[480px] justify-end">
        <Button variant="primary" disabled={value === null} onClick={onNext}>
          {t('wizard.nav.next')}
        </Button>
      </div>
    </div>
  );
}
