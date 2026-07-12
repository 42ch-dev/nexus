import type { World } from '@42ch/nexus-contracts';
import { useTranslation } from 'react-i18next';

export const ALL_WORLDS = null;

export function countFragmentsByWorld(fragments: { world_id?: string | null }[]): Record<string, number> {
  const counts: Record<string, number> = {};
  for (const f of fragments) {
    const id = f.world_id?.trim();
    if (!id) continue;
    counts[id] = (counts[id] ?? 0) + 1;
  }
  return counts;
}

export function worldOptionLabel(world: World, fragmentCount: number, t: (key: string, options?: Record<string, unknown>) => string): string {
  const countText = fragmentCount > 0 ? t('soul.fragmentCount', { count: fragmentCount, keyword: world.title ?? world.world_id }) : t('soul.noFragments');
  return `${world.title ?? world.world_id} (${countText})`;
}

export function WorldSelector({
  worlds,
  fragmentCounts,
  selectedWorld,
  onSelect,
  disabled,
}: {
  worlds: World[];
  fragmentCounts: Record<string, number>;
  selectedWorld: string | null;
  onSelect: (worldId: string | null) => void;
  disabled?: boolean;
}) {
  const { t } = useTranslation('memory');
  const sortedWorlds = [...worlds].sort((a, b) =>
    (a.title ?? a.world_id).localeCompare(b.title ?? b.world_id),
  );

  return (
    <label className="flex items-center gap-2 text-copy-14">
      <span className="text-gray-700">{t('soul.worldLabel')}</span>
      <select
        value={selectedWorld ?? ''}
        onChange={(e) => onSelect(e.target.value === '' ? ALL_WORLDS : e.target.value)}
        disabled={disabled || sortedWorlds.length === 0}
        className="h-9 max-w-[16rem] rounded-control border border-gray-alpha-400 bg-background-100 px-2 text-copy-14 text-gray-1000 focus-visible:outline-none disabled:cursor-not-allowed disabled:bg-background-200 disabled:text-gray-700"
        data-testid="soul-world-selector"
      >
        <option value="">{t('soul.allWorlds')}</option>
        {sortedWorlds.map((world) => (
          <option key={world.world_id} value={world.world_id}>
            {worldOptionLabel(world, fragmentCounts[world.world_id] ?? 0, t)}
          </option>
        ))}
      </select>
      {sortedWorlds.length === 0 ? (
        <span className="text-copy-13 text-gray-700" data-testid="soul-world-scope-label">
          {t('soul.noWorlds')}
        </span>
      ) : selectedWorld === null ? (
        <span className="text-copy-13 text-gray-700" data-testid="soul-world-scope-label">
          {t('soul.wholeSoul')}
        </span>
      ) : (
        <span className="text-copy-13 text-gray-700" data-testid="soul-world-scope-label">
          {t('soul.subsetSoul')}
        </span>
      )}
    </label>
  );
}
