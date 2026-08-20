import { ChevronLeft } from 'lucide-react';
import { useTranslation } from 'react-i18next';

import { Button } from '@/components/ui/button';
import { type EntranceId } from '@/components/layout/entrance-registry';
import { cn } from '@/lib/utils';
import type { WizardState } from '@/pages/setup-wizard-page';

interface SetupStepEntranceProps {
  state: WizardState;
  onChange: (state: WizardState) => void;
  onNext: () => void;
  /** Hidden on first step (Entrance); omit so Back is not shown. */
  onBack?: () => void;
}

/**
 * Wizard Entrance step (V1.170 P1 — AR-17, product EL §2).
 *
 * Step 1 of Entrance → Agent → Workspace → Done. Two option cards with the
 * locked EL §2 copy; the choice is cheap and daemon-independent, so it comes
 * first and the remaining steps can frame themselves for the chosen layout.
 * The selection is written into `WizardState.entrance` immediately; the
 * wizard's `finish()` persists it (Tauri IPC / localStorage) before marking
 * setup completed.
 */
const OPTIONS: readonly { id: EntranceId; titleKey: string; descriptionKey: string }[] = [
  {
    id: 'content-creator',
    titleKey: 'step.entrance.option.contentCreator.title',
    descriptionKey: 'step.entrance.option.contentCreator.description',
  },
  {
    id: 'developer',
    titleKey: 'step.entrance.option.developer.title',
    descriptionKey: 'step.entrance.option.developer.description',
  },
] as const;

export function SetupStepEntrance({ state, onChange, onNext, onBack }: SetupStepEntranceProps) {
  const { t } = useTranslation('setup');
  const selected = state.entrance;

  function select(id: EntranceId) {
    if (id === selected) return;
    onChange({ ...state, entrance: id });
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-4">
      <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto" data-testid="wizard-step-body">
        <div className="flex flex-col gap-2">
          <h2 className="font-display text-display-24 text-gray-1000">{t('step.entrance.title')}</h2>
          <p className="text-copy-14 text-gray-900">{t('step.entrance.description')}</p>
        </div>

        <div
          className="grid grid-cols-1 gap-4 sm:grid-cols-2"
          role="radiogroup"
          aria-label={t('step.entrance.optionsLabel')}
          data-testid="entrance-options"
        >
          {OPTIONS.map((option) => {
            const isSelected = selected === option.id;
            return (
              <button
                key={option.id}
                type="button"
                role="radio"
                aria-checked={isSelected}
                onClick={() => select(option.id)}
                data-testid={`entrance-option-${option.id}`}
                className={cn(
                  'flex flex-col gap-2 rounded-card border-2 p-4 text-left transition-colors duration-state ease-standard motion-reduce:transition-none',
                  'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-1000 dark:focus-visible:ring-blue-700',
                  isSelected
                    ? 'border-blue-1000 dark:border-blue-700'
                    : 'border-gray-alpha-400 hover:bg-gray-alpha-100',
                )}
              >
                <span className="text-heading-16 font-heading text-gray-1000">
                  {t(option.titleKey)}
                </span>
                <span className="text-copy-14 text-gray-900">{t(option.descriptionKey)}</span>
              </button>
            );
          })}
        </div>
      </div>

      <div
        className="mt-auto flex shrink-0 items-center gap-setup-wizard-surface-cta-container-gap"
        data-testid="wizard-cta-row"
        data-layout="horizontal-adjacent"
      >
        {onBack && (
          <Button variant="tertiary" onClick={onBack} aria-label={t('action.back')} className="px-2">
            <ChevronLeft className="h-4 w-4" aria-hidden="true" />
          </Button>
        )}
        <Button
          variant="primary"
          onClick={onNext}
          className="w-full max-w-setup-wizard-surface-cta-primary-max-width"
        >
          {t('action.continue')}
        </Button>
      </div>
    </div>
  );
}
