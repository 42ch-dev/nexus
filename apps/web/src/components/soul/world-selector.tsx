import type { MemoryFragmentInfo } from '@42ch/nexus-contracts';

/**
 * World projection selector (V1.81 SP-2 — web-ui.md §26).
 *
 * Drives a `world_id` query param on the fragments query so the keyword
 * clusters, temporal drift, and growth-curve re-scope to the selected world's
 * subset. Defaults to "All worlds" (the whole Creator SOUL). The narrative card
 * is world-agnostic and is NOT re-scoped by this control.
 *
 * Honest subset semantics (UX contract, web-ui.md §26.1 SP-2):
 *  - "All worlds" is always present and default; its helper frames the whole
 *    Creator SOUL.
 *  - A world is listed iff it has ≥1 fragment carrying that `world_id`.
 *    Worlds with zero fragments (and, given the contract, no resolvable Work
 *    binding — see omission note below) never become options — no dead-end
 *    empty choices.
 *
 * simplify: world-title resolution. The options are labeled with the fragment's
 * `world_id` because the Local API exposes no worlds-list / world-detail
 * endpoint to resolve `world_id` → title in V1.81 (WorkSummary also lacks
 * `world_id`). This mirrors the memory-detail-panel.tsx precedent (open item #3:
 * raw `world_id` shown as-is). When a worlds-list or world-detail endpoint
 * ships, replace `worldOptionLabel` to render the world title and re-add the
 * "Work-backed but no-fragment world" subset-empty path the product spec
 * describes. Tracked as a V1.81 deferral, not a behavior bug.
 */

/** The "All worlds" sentinel value — the whole Creator SOUL projection. */
export const ALL_WORLDS = null;

export interface WorldOption {
  worldId: string;
  fragmentCount: number;
}

/**
 * Derive the world-selector options from a creator's fragment list. A world
 * appears iff ≥1 fragment carries a non-empty `world_id`. Fragments with a
 * null/empty `world_id` are Creator-core-only and contribute to "All worlds"
 * but not to any world option. Pure + deterministic (sorted by world_id) so the
 * option order is stable across renders and unit-testable without a DOM.
 */
export function deriveWorldOptions(fragments: MemoryFragmentInfo[]): WorldOption[] {
  const counts = new Map<string, number>();
  for (const f of fragments) {
    const id = f.world_id?.trim();
    if (!id) continue;
    counts.set(id, (counts.get(id) ?? 0) + 1);
  }
  return [...counts.entries()]
    .map(([worldId, fragmentCount]) => ({ worldId, fragmentCount }))
    .sort((a, b) => a.worldId.localeCompare(b.worldId));
}

/** Render label for a world option (see `simplify` note on title resolution). */
export function worldOptionLabel(option: WorldOption): string {
  const noun = option.fragmentCount === 1 ? 'fragment' : 'fragments';
  return `${option.worldId} (${option.fragmentCount} ${noun})`;
}

export function WorldSelector({
  options,
  selectedWorld,
  onSelect,
  disabled,
}: {
  options: WorldOption[];
  selectedWorld: string | null;
  onSelect: (worldId: string | null) => void;
  disabled?: boolean;
}) {
  return (
    <label className="flex items-center gap-2 text-copy-14">
      <span className="text-gray-700">World</span>
      <select
        value={selectedWorld ?? ''}
        onChange={(e) => onSelect(e.target.value === '' ? ALL_WORLDS : e.target.value)}
        disabled={disabled || options.length === 0}
        className="h-9 max-w-[16rem] rounded-control border border-gray-alpha-400 bg-background-100 px-2 text-copy-14 text-gray-1000 focus-visible:outline-none disabled:cursor-not-allowed disabled:bg-background-200 disabled:text-gray-700"
        data-testid="soul-world-selector"
      >
        <option value="">All worlds</option>
        {options.map((opt) => (
          <option key={opt.worldId} value={opt.worldId}>
            {worldOptionLabel(opt)}
          </option>
        ))}
      </select>
      {selectedWorld === null ? (
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
