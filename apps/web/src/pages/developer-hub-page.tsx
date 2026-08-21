import { useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import { Link } from 'react-router';

import { usePresets, type PresetGroups } from '@/api/queries';
import { Card, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';

/**
 * Develop hub v1 (V1.170 P1 — AR-18 / product EL §4).
 *
 * One screen, first land on the Developer entrance: cards, not a dashboard of
 * live graphs. Every card links to an EXISTING surface — no new daemon
 * endpoints (`wire_contracts_changed: false`). Trigger-lane catalog,
 * declared-signals UI, and the cron editor are phase C (honest non-goals —
 * nothing here fakes next-fire or webhook binding).
 *
 * Copy is key-first under the `shell` namespace (`hub.develop.*`); values land
 * in T4 (AR-21) — a missing key renders its path until then.
 */

interface HubCardSpec {
  id: string;
  titleKey: string;
  descriptionKey: string;
  to: string;
}

const HUB_CARDS: readonly HubCardSpec[] = [
  {
    id: 'presets',
    titleKey: 'hub.develop.presets.title',
    descriptionKey: 'hub.develop.presets.description',
    to: '/strategies',
  },
  {
    id: 'capabilities',
    titleKey: 'hub.develop.capabilities.title',
    descriptionKey: 'hub.develop.capabilities.description',
    to: '/capabilities',
  },
  {
    id: 'modules',
    titleKey: 'hub.develop.modules.title',
    descriptionKey: 'hub.develop.modules.description',
    to: '/settings/modules',
  },
  {
    id: 'strategy-canvas',
    titleKey: 'hub.develop.strategyCanvas.title',
    descriptionKey: 'hub.develop.strategyCanvas.description',
    to: '/strategies',
  },
  {
    id: 'run-studio',
    titleKey: 'hub.develop.runStudio.title',
    descriptionKey: 'hub.develop.runStudio.description',
    to: '/settings/modules',
  },
  {
    id: 'connect',
    titleKey: 'hub.develop.connect.title',
    descriptionKey: 'hub.develop.connect.description',
    to: '/connect',
  },
] as const;

/** First USER preset, else first embedded non-`_system` preset, else the
 * manager — the EL §4 canvas card link target. */
function resolveStrategyCanvasTarget(presets: PresetGroups | undefined): string {
  const userPreset = presets?.user[0];
  if (userPreset) return `/strategies/${encodeURIComponent(userPreset.id)}`;
  const embeddedPreset = presets?.embedded.find(
    (preset) => !preset.id.startsWith('_system.'),
  );
  if (embeddedPreset) return `/strategies/${encodeURIComponent(embeddedPreset.id)}`;
  return '/strategies';
}

export function DeveloperHubPage() {
  const { t } = useTranslation('shell');
  const presets = usePresets();

  const strategyCanvasTarget = useMemo(
    () => resolveStrategyCanvasTarget(presets.data),
    [presets.data],
  );

  const presetCount = useMemo(() => {
    if (!presets.data) return null;
    return (
      presets.data.user.length +
      presets.data.embedded.filter((preset) => !preset.id.startsWith('_system.'))
        .length
    );
  }, [presets.data]);

  const cards: HubCardSpec[] = useMemo(
    () =>
      HUB_CARDS.map((card) =>
        card.id === 'strategy-canvas'
          ? { ...card, to: strategyCanvasTarget }
          : card,
      ),
    [strategyCanvasTarget],
  );

  return (
    <div className="flex flex-col gap-4" data-testid="developer-hub-page">
      <div className="flex flex-col gap-1">
        <h1 className="text-heading-24 font-heading text-gray-1000">
          {t('hub.develop.title')}
        </h1>
        <p className="text-copy-14 text-gray-900">{t('hub.develop.description')}</p>
      </div>

      <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3">
        {cards.map((card) => (
          <Card
            key={card.id}
            interactive
            className="flex flex-col"
            data-testid={`developer-hub-card-${card.id}`}
          >
            <CardHeader className="flex-1">
              <CardTitle>
                <Link
                  to={card.to}
                  className="text-gray-1000 transition-colors duration-state ease-standard motion-reduce:transition-none hover:text-blue-800 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-700"
                  data-testid={`developer-hub-link-${card.id}`}
                >
                  {t(card.titleKey)}
                </Link>
              </CardTitle>
              <CardDescription>{t(card.descriptionKey)}</CardDescription>
            </CardHeader>
            {card.id === 'presets' && presetCount !== null && (
              <p
                className="px-6 pb-4 text-copy-13 text-gray-700"
                data-testid="developer-hub-presets-count"
              >
                {t('hub.develop.presets.count', { count: presetCount })}
              </p>
            )}
          </Card>
        ))}
      </div>
    </div>
  );
}
