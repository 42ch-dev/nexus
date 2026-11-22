/**
 *
 * The rendered document is never the preset YAML this section would write, so
 * the field states the route that does reach the template instead of a Save.
 */
import { useTranslation } from 'react-i18next';

import type { EditForm } from '../state-machine';

interface PromptInspectorProps {
  form: EditForm;
  onChange: <K extends keyof EditForm>(field: K, value: EditForm[K]) => void;
  promptTemplateRef: string;
}

export function PromptInspector({ form, onChange, promptTemplateRef }: PromptInspectorProps) {
  const { t } = useTranslation('canvas');

  return (
    <section className="flex flex-col gap-2" aria-label={t('strategy.inspector.prompt.ariaLabel')}>
      <div className="flex items-center justify-between">
        <span className="text-label-14 font-semibold text-gray-900">{t('strategy.inspector.prompt.title')}</span>
      </div>
      <span className="text-copy-13-mono text-gray-700">{promptTemplateRef}</span>
      <textarea
        value={form.promptBody}
        onChange={(e) => onChange('promptBody', e.target.value)}
        disabled
        rows={4}
        placeholder={t('strategy.inspector.prompt.placeholder')}
        className="rounded-control border border-gray-alpha-400 bg-background-100 px-2 py-1 text-gray-1000 focus:border-blue-1000 dark:focus:border-blue-700"
      />
      <p data-testid="prompt-structural-disabled" className="text-copy-12 text-gray-700">
        {t('strategy.inspector.prompt.structuralDisabled')}
      </p>
    </section>
  );
}
