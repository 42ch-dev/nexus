/**
 * 操作台 (Hub) — the developer entrance's landing surface (plan v1220 T1).
 *
 * Read-only status aggregation: four cards, each linking into its rail app.
 * This task ships the page shell + card frames; T2 fills the cards with live
 * data from the existing daemon endpoints (no new wire contract, IA §6).
 */
import { useTranslation } from 'react-i18next';

import { HubCard } from '@/components/developer/hub-card';

const HUB_CARD_TITLES = [
  { id: 'service', titleKey: 'hub.operating.service.title' },
  { id: 'agent', titleKey: 'hub.operating.agent.title' },
  { id: 'harness', titleKey: 'hub.operating.harness.title' },
  { id: 'modules', titleKey: 'hub.operating.modules.title' },
] as const;

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

      <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
        {HUB_CARD_TITLES.map((card) => (
          <HubCard key={card.id} id={card.id} title={t(card.titleKey)} />
        ))}
      </div>
    </div>
  );
}
