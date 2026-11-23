/**
 * Presentation only: the shell owns the finish sequence (creator bootstrap →
 * workspace selection → `createWorld`), so a rejected finish leaves this step
 * mounted with the draft and the product copy. There is no client-side
 * first-run marker — the gate flips from the selected/default workspace state
 * (compass D21/D25).
 */
import { useState } from 'react';
import { useTranslation } from 'react-i18next';

import { Button, Input, Label } from '@/components/ui';
import { errorMessage } from '@/lib/error-message';
import { cn } from '@/lib/utils';
import { useActiveWorkspaceQuery } from '@/lib/workspace-context';

const LINK_CLASS =
  'text-label-14 font-medium text-brand-deep-blue transition-colors duration-state ease-standard hover:opacity-80 dark:text-blue-700 dark:hover:opacity-100';

interface WorldTemplate {
  id: 'blank' | 'fantasy' | 'scifi';
  emoji: string;
  /** Whether the template preselects a catalog name; the field stays editable after picking. */
  prefillsName: boolean;
}

const TEMPLATES: readonly WorldTemplate[] = [
  { id: 'blank', emoji: '📄', prefillsName: false },
  { id: 'fantasy', emoji: '🏰', prefillsName: true },
  { id: 'scifi', emoji: '🚀', prefillsName: true },
];

/** Prototype default: the fantasy template, with its name prefilled. */
const DEFAULT_TEMPLATE = TEMPLATES[1]!;

interface StepFirstWorldProps {
  onBack: () => void;
  /** Runs the finish sequence; a rejection keeps this step mounted with the draft. */
  onFinish: (name: string) => Promise<void>;
}

export function StepFirstWorld({ onBack, onFinish }: StepFirstWorldProps) {
  const { t } = useTranslation('setup');
  const templateName = (template: WorldTemplate) =>
    template.prefillsName ? t(`wizard.firstWorld.template.${template.id}.name`) : '';
  const [templateId, setTemplateId] = useState(DEFAULT_TEMPLATE.id);
  const [name, setName] = useState(() => templateName(DEFAULT_TEMPLATE));
  const [error, setError] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);
  const [showRoot, setShowRoot] = useState(false);
  const activeWorkspace = useActiveWorkspaceQuery();

  const workspaceRoot = activeWorkspace.data?.creative_root ?? null;
  const trimmed = name.trim();

  function pickTemplate(template: WorldTemplate) {
    setTemplateId(template.id);
    setName(templateName(template));
    setError(null);
  }

  async function submit() {
    if (trimmed === '' || submitting) return;
    setSubmitting(true);
    setError(null);
    try {
      await onFinish(trimmed);
    } catch (failure) {
      setError(
        t('wizard.firstWorld.createFailed', {
          reason: errorMessage(failure) || t('wizard.unknownError'),
        }),
      );
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <div className="flex flex-col items-center">
      <div className="text-label-12 text-gray-700">{t('wizard.firstWorld.stepLabel')}</div>
      <h1 className="mt-4 font-display text-display-32 text-gray-1000">{t('wizard.firstWorld.title')}</h1>
      <p className="mt-3 text-copy-14 text-gray-900">
        {t('wizard.firstWorld.description')}
      </p>

      <div
        role="radiogroup"
        aria-label={t('wizard.firstWorld.templatesLabel')}
        className="mt-10 grid w-[720px] max-w-full grid-cols-1 gap-3 sm:grid-cols-3"
      >
        {TEMPLATES.map((template) => {
          const selected = templateId === template.id;
          return (
            <button
              key={template.id}
              type="button"
              role="radio"
              aria-checked={selected}
              data-testid={`world-template-${template.id}`}
              onClick={() => pickTemplate(template)}
              className={cn(
                'flex flex-col items-center rounded-card border-2 bg-background-100 px-4 py-6 text-center transition-colors duration-state ease-standard motion-reduce:transition-none',
                selected ? 'border-blue-700' : 'border-gray-alpha-400 hover:border-blue-700',
              )}
            >
              <span aria-hidden="true" className="text-2xl">
                {template.emoji}
              </span>
              <span className="mt-3 text-heading-16 font-heading text-gray-1000">
                {t(`wizard.firstWorld.template.${template.id}.label`)}
              </span>
              <span className="mt-1 text-copy-13 text-gray-900">{t(`wizard.firstWorld.template.${template.id}.description`)}</span>
            </button>
          );
        })}
      </div>

      <div className="mt-8 flex w-[480px] flex-col gap-1.5">
        <Label htmlFor="onboarding-world-name" className="text-label-14 font-medium text-gray-1000">
          {t('wizard.firstWorld.nameLabel')}
        </Label>
        <Input
          id="onboarding-world-name"
          value={name}
          onChange={(event) => setName(event.target.value)}
          placeholder={t('wizard.firstWorld.namePlaceholder')}
        />
      </div>

      <div className="mt-4 flex w-[480px] flex-col gap-2">
        <p className="text-copy-13 text-gray-900">
          {t('wizard.firstWorld.savedInDefault')}{' '}
          <button
            type="button"
            aria-expanded={showRoot}
            onClick={() => setShowRoot((open) => !open)}
            className={LINK_CLASS}
          >
            {t('wizard.changeLocation')}
          </button>
        </p>
        {showRoot ? (
          <p
            data-testid="first-world-root"
            className="rounded-control border border-gray-alpha-400 bg-background-200 px-3 py-2 text-copy-14 text-gray-900"
          >
            {workspaceRoot ?? t('wizard.defaultWorkspace')}
          </p>
        ) : null}
      </div>

      {error ? (
        <p role="alert" className="mt-4 w-[480px] text-copy-13 text-red-700">
          {error}
        </p>
      ) : null}

      <div className="mt-12 flex w-[480px] items-center justify-between">
        <Button variant="tertiary" onClick={onBack}>
          {t('wizard.nav.back')}
        </Button>
        <Button
          variant="primary"
          disabled={trimmed === '' || submitting}
          onClick={() => void submit()}
        >
          {t('wizard.firstWorld.submit')}
        </Button>
      </div>
    </div>
  );
}
