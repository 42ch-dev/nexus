/**
 * 操作台 (Hub) — the developer entrance's landing surface (plan v1220 T1/T2).
 *
 * Read-only status aggregation: four cards (本地服务 / Agent / Harness 摘要 /
 * 已装模块), each reading existing daemon endpoints and linking into its rail
 * app. No new wire contract (IA §6), no mutating action on this page.
 */
import { useTranslation } from 'react-i18next';

import { HubCards } from '@/components/developer/hub-cards';

export function HubPage() {
  const { t } = useTranslation('shell');

  return (
    <div className="flex flex-col gap-4" data-testid="developer-hub-page">
      <div className="flex flex-col gap-1">
        <h1 className="text-heading-24 font-heading text-gray-1000">
          {t('hub.operating.title')}
        </h1>
        <p className="text-copy-14 text-gray-900">{t('hub.operating.description')}</p>
      </div>

      <HubCards />
    </div>
  );
}
