/**
 * Transition / edge inspector section — displays the scalar `next` transition.
 *
 * The rendered document is never the preset YAML this section would write, so
 * the field is shown with the route that does reach it instead of a Save.
 */
import { useTranslation } from 'react-i18next';

import type { EditForm } from '../state-machine';
import type { PresetState } from '@/lib/canvas/preset-yaml';

interface EdgeInspectorProps {
  selectedState: PresetState;
  form: EditForm;
  onChange: <K extends keyof EditForm>(field: K, value: EditForm[K]) => void;
}

export function EdgeInspector({ selectedState, form, onChange }: EdgeInspectorProps) {
  const { t } = useTranslation('canvas');

  if (typeof selectedState.next !== 'string') return null;

  return (
    <section className="flex flex-col gap-2" aria-label={t('strategy.inspector.transition.ariaLabel')}>
      <div className="flex items-center justify-between">
        <span className="text-label-14 font-semibold text-gray-900">{t('strategy.inspector.transition.title')}</span>
      </div>
      <label className="flex flex-col gap-1 text-copy-13">
        <span className="text-gray-700">{t('strategy.inspector.transition.targetLabel')}</span>
        <input
          type="text"
          value={form.nextTarget}
          onChange={(e) => onChange('nextTarget', e.target.value)}
          disabled
          className="rounded-control border border-gray-alpha-400 bg-background-100 px-2 py-1 text-gray-1000 focus:border-blue-1000 dark:focus:border-blue-700"
        />
      </label>
      <p data-testid="transition-structural-disabled" className="text-copy-12 text-gray-700">
        {t('strategy.inspector.structuralDisabled')}
      </p>
    </section>
  );
}
