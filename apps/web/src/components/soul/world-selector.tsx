import type { World } from '@42ch/nexus-contracts';

/**
 * World projection selector (V1.82 SP-2 — web-ui.md §26).
 *
 * Drives a `world_id` query param on the fragments query and the SOUL narrative
 * so the keyword clusters, temporal drift, growth curve, and narrative all
 * re-scope to the selected world's subset. Defaults to "All worlds" (the whole
 * Creator SOUL).
 *
 * The options come from `GET /v1/daemon/narrative/worlds`, which returns every
 * Work-backed world including zero-fragment worlds. That makes the honest
 * subset-empty path reachable: a world may exist but have no fragments yet.
 *
 * Honest subset semantics (UX contract):
 *  - "All worlds" is always present and default; its helper frames the whole
 *    Creator SOUL.
 *  - A world is listed regardless of fragment count; zero-fragment worlds show
 *    an honest "no fragments" helper so the author knows the selector is scoped
 *    but empty.
 */

/** The "All worlds" sentinel value — the whole Creator SOUL projection. */
export const ALL_WORLDS = null;

/** Count fragments per world from the whole-creator fragment list. */
export function countFragmentsByWorld(fragments: { world_id?: string | null }[]): Record<string, number> {
  const counts: Record<string, number> = {};
  for (const f of fragments) {
    const id = f.world_id?.trim();
    if (!id) continue;
    counts[id] = (counts[id] ?? 0) + 1;
  }
  return counts;
}

/** Render label for a world option: title with fragment count (or "no fragments"). */
export function worldOptionLabel(world: World, fragmentCount: number): string {
  const count = fragmentCount;
  const noun = count === 1 ? 'fragment' : 'fragments';
  const countText = count > 0 ? `${count} ${noun}` : 'no fragments';
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
  const sortedWorlds = [...worlds].sort((a, b) =>
    (a.title ?? a.world_id).localeCompare(b.title ?? b.world_id),
  );

  return (
    <label className="flex items-center gap-2 text-copy-14">
      <span className="text-gray-700">World</span>
      <select
        value={selectedWorld ?? ''}
        onChange={(e) => onSelect(e.target.value === '' ? ALL_WORLDS : e.target.value)}
        disabled={disabled || sortedWorlds.length === 0}
        className="h-9 max-w-[16rem] rounded-control border border-gray-alpha-400 bg-background-100 px-2 text-copy-14 text-gray-1000 focus-visible:outline-none disabled:cursor-not-allowed disabled:bg-background-200 disabled:text-gray-700"
        data-testid="soul-world-selector"
      >
        <option value="">All worlds</option>
        {sortedWorlds.map((world) => (
          <option key={world.world_id} value={world.world_id}>
            {worldOptionLabel(world, fragmentCounts[world.world_id] ?? 0)}
          </option>
        ))}
      </select>
      {sortedWorlds.length === 0 ? (
        <span className="text-copy-13 text-gray-700" data-testid="soul-world-scope-label">
          no worlds in this workspace
        </span>
      ) : selectedWorld === null ? (
        <span className="text-copy-13 text-gray-700" data-testid="soul-world-scope-label">
          your whole Creator SOUL
        </span>
      ) : (
        <span className="text-copy-13 text-gray-700" data-testid="soul-world-scope-label">
          a subset of your Creator SOUL
        </span>
      )}
    </label>
  );
}
