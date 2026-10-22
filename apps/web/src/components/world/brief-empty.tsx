/**
 * Brief empty state (plan v1219-world-module T2) — the guided entry into the
 * first Brief: final prompt + example copy (product-owned) and the
 * `+ 写 Brief` action that mounts the inline {@link BriefEditor}.
 */
import { useTranslation } from 'react-i18next';

export interface BriefEmptyProps {
  onCreate: () => void;
}

export function BriefEmpty({ onCreate }: BriefEmptyProps) {
  const { t } = useTranslation('worlds');
  return (
    <div className="rounded-card border border-gray-500 p-6" data-testid="world-brief-empty">
      <h2 className="font-display text-display-24 text-gray-1000">
        {t('worldApp.briefEmpty.title')}
      </h2>
      <p className="text-copy-14 text-gray-900">{t('worldApp.briefEmpty.body')}</p>
      <p className="text-copy-14 text-gray-700">{t('worldApp.briefEmpty.example')}</p>
      <button
        type="button"
        data-testid="world-brief-create"
        className="rounded-control bg-gray-1000 px-3 py-1.5 text-copy-14 text-gray-100 hover:bg-gray-900"
        onClick={onCreate}
      >
        {t('worldApp.briefEmpty.action')}
      </button>
    </div>
  );
}
