/**
 * World layer tabs (plan v1219-world-module T1) — Brief / Narrative / Moment.
 *
 * The tabs ride the existing `?layer=brief|narrative|moment` URL convention
 * (V1.123 P4): the active layer is derived from the search param (unknown or
 * absent values fall back to Brief), and switching writes the param back so
 * layer choice is deep-linkable and history-stable. Content per layer lives in
 * {@link WorldApp}; this component is the switcher chrome only.
 */
import { useTranslation } from 'react-i18next';
import { useSearchParams } from 'react-router';

import { cn } from '@/lib/utils';

export type WorldLayer = 'brief' | 'narrative' | 'moment';

export const WORLD_LAYERS: readonly WorldLayer[] = ['brief', 'narrative', 'moment'];

/** Derive the active layer from `?layer=`; anything unknown defaults to Brief. */
export function parseWorldLayer(raw: string | null): WorldLayer {
  return raw === 'narrative' || raw === 'moment' ? raw : 'brief';
}

export function LayerTabs({ active }: { active: WorldLayer }) {
  const { t } = useTranslation('worlds');
  const [, setSearchParams] = useSearchParams();

  return (
    <div role="tablist" aria-label={t('worldApp.tabsAria')} className="flex gap-1">
      {WORLD_LAYERS.map((layer) => (
        <button
          key={layer}
          type="button"
          role="tab"
          aria-selected={active === layer}
          data-testid={`layer-tab-${layer}`}
          className={cn(
            'rounded-control px-3 py-1.5 text-copy-14',
            active === layer
              ? 'bg-gray-1000 text-gray-100'
              : 'text-gray-700 hover:bg-gray-alpha-100',
          )}
          onClick={() => setSearchParams({ layer })}
        >
          {t(`worldApp.layer.${layer}`)}
        </button>
      ))}
    </div>
  );
}
