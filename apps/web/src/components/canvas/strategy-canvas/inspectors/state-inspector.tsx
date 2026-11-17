/**
 * State inspector section — displays the state label and description.
 *
 * The rendered document is never the preset YAML this section would write, so
 * the fields are shown with the route that does reach them instead of a Save.
 */
import { useTranslation } from 'react-i18next';

import type { EditForm } from '../state-machine';

interface StateInspectorProps {
  form: EditForm;
  onChange: <K extends keyof EditForm>(field: K, value: EditForm[K]) => void;
}

export function StateInspector({ form, onChange }: StateInspectorProps) {
  const { t } = useTranslation('canvas');

  return (
    <section className="flex flex-col gap-2" aria-label={t('strategy.inspector.state.ariaLabel')}>
      <div className="flex items-center justify-between">
        <span className="text-label-14 font-semibold text-gray-900">{t('strategy.inspector.state.title')}</span>
      </div>
      <label className="flex flex-col gap-1 text-copy-13">
        <span className="text-gray-700">{t('strategy.inspector.state.labelLabel')}</span>
        <input
          type="text"
          value={form.label}
          onChange={(e) => onChange('label', e.target.value)}
          disabled
          className="rounded-control border border-gray-alpha-400 bg-background-100 px-2 py-1 text-gray-1000 focus:border-blue-1000 dark:focus:border-blue-700"
        />
      </label>
      <label className="flex flex-col gap-1 text-copy-13">
        <span className="text-gray-700">{t('strategy.inspector.state.descriptionLabel')}</span>
        <textarea
          value={form.description}
          onChange={(e) => onChange('description', e.target.value)}
          disabled
          rows={3}
          className="rounded-control border border-gray-alpha-400 bg-background-100 px-2 py-1 text-gray-1000 focus:border-blue-1000 dark:focus:border-blue-700"
        />
      </label>
      <p data-testid="state-structural-disabled" className="text-copy-12 text-gray-700">
        {t('strategy.inspector.structuralDisabled')}
      </p>
    </section>
  );
}
