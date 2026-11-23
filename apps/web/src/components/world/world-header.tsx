import { useTranslation } from 'react-i18next';

export function WorldHeader({ name }: { name: string }) {
  const { t } = useTranslation('worlds');
  return (
    <header data-testid="world-header">
      <h1 className="font-display text-display-24 text-gray-1000">{name}</h1>
      <p className="text-copy-14 text-gray-900">{t('worldApp.subtitle')}</p>
    </header>
  );
}
