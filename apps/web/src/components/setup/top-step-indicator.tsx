import { useTranslation } from 'react-i18next';

import { cn } from '@/lib/utils';

export type WizardStep = 'entrance' | 'agent' | 'workspace' | 'done';

type StepDef = { id: WizardStep; key: string };

const STEP_DEFS: StepDef[] = [
  // V1.170 P1 (AR-17): Entrance is step 1 — cheap and daemon-independent, so
  // the remaining steps can frame themselves for the chosen layout.
  { id: 'entrance', key: 'setup:progress.entrance' },
  { id: 'agent', key: 'setup:progress.agent' },
  { id: 'workspace', key: 'setup:progress.workspace' },
  { id: 'done', key: 'setup:progress.done' },
];

type StepStatus = 'complete' | 'active' | 'pending';

function stepStatus(currentStep: WizardStep, index: number): StepStatus {
  const currentIndex = STEP_DEFS.findIndex((s) => s.id === currentStep);
  if (index < currentIndex) return 'complete';
  if (index === currentIndex) return 'active';
  return 'pending';
}

/**
 * Top horizontal Steps (V1.105 N1) — replaces the left step rail.
 * Visual SSOT: apps/design-studio setup-wizard-chrome-fixtures TopStepIndicator.
 */
export function TopStepIndicator({ currentStep }: { currentStep: WizardStep }) {
  const { t } = useTranslation('setup');
  return (
    <nav aria-label={t('progress.label')} className="w-full shrink-0" data-testid="top-step-indicator">
      <ol className="flex w-full items-center justify-between gap-2">
        {STEP_DEFS.map((s, index) => {
          const status = stepStatus(currentStep, index);
          return (
            <li
              key={s.id}
              className="relative flex min-w-0 flex-1 flex-col items-center gap-2"
              aria-current={status === 'active' ? 'step' : undefined}
              data-step-id={s.id}
              data-step-status={status}
            >
              {index < STEP_DEFS.length - 1 && (
                <div
                  className="absolute top-[calc(var(--color-setup-wizard-step-circle-size)/2)] left-[calc(50%+var(--color-setup-wizard-step-circle-size)/2+4px)] right-[calc(-50%+var(--color-setup-wizard-step-circle-size)/2+4px)] h-px bg-setup-wizard-step-connector"
                  aria-hidden
                  data-testid="step-connector"
                />
              )}
              <span
                className={cn(
                  'z-10 flex h-setup-wizard-step-circle-size w-setup-wizard-step-circle-size items-center justify-center rounded-full text-button-14 font-button transition-colors duration-state ease-standard motion-reduce:transition-none',
                  status === 'active' &&
                    'bg-setup-wizard-step-circle-active-bg text-setup-wizard-step-circle-active-text',
                  status === 'complete' &&
                    'bg-setup-wizard-step-circle-complete-bg text-setup-wizard-step-circle-complete-text',
                  status === 'pending' &&
                    'bg-setup-wizard-step-circle-pending-bg text-setup-wizard-step-circle-pending-text',
                )}
              >
                {index + 1}
              </span>
              <span
                className={cn(
                  'truncate text-center text-setup-wizard-step-label-typography',
                  status === 'pending'
                    ? 'text-setup-wizard-step-label-pending-color'
                    : 'text-setup-wizard-step-label-active-color',
                )}
              >
                {t(s.key)}
              </span>
            </li>
          );
        })}
      </ol>
    </nav>
  );
}
