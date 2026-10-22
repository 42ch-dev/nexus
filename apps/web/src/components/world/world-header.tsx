/**
 * World app header (plan v1219-world-module T1) — world name + surface
 * subtitle (`世界 · 时间线`, product-owned copy). Sits above the layer tabs.
 */
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
