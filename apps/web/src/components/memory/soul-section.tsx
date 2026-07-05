/**
 * SoulSection — V1.79 SOUL visualization wrapper, extended in V1.81 (Creator
 * SOUL Maturation) and V1.82 (per-World SOUL narrative completion).
 *
 * V1.79: keyword clusters + temporal drift over internalized fragments,
 * reusing the existing fragments query (fragments carry `keywords` +
 * `created_at` + `world_id`). Click-to-filter is surfaced back to the page
 * shell via `onFilterFragments`.
 *
 * V1.81 additions (web-ui.md §26):
 *  - **Narrative card** (SP-1): the headline surface, rendered above the
 *    keyword/drift viz.
 *  - **World projection selector** (SP-2): re-scopes the keyword/drift +
 *    growth-curve to a world's fragment subset. "All worlds" (default) is the
 *    whole Creator SOUL.
 *  - **Growth-curve** (SP-3): cumulative fragment growth, independent of the
 *    temporal-drift timeline.
 *  - **Auto-refresh** (SP-4): the SOUL fragments query polls on the
 *    `SOUL_REFETCH_MS` cadence; the narrative + fragments queries invalidate
 *    after a review mutation (wired in queries.ts `useReviewMemory`).
 *
 * V1.82 additions:
 *  - The selector now loads world **titles** from `GET /v1/daemon/narrative/worlds`
 *    and includes Work-backed worlds with zero fragments.
 *  - The narrative card re-scopes with the selected world: "All worlds" →
 *    Creator-level narrative; a world → that world's per-World narrative.
 *  - Per-world insufficient/empty states are independent of the Creator-level
 *    state.
 *
 * Layout: the narrative card sits at the top; below it, the world-scoped
 * keyword/drift viz + growth-curve. When a world projection returns zero
 * fragments, the whole viz area shows the honest subset-empty copy.
 */
import { useState } from 'react';

import { GrowthCurve } from '@/components/soul/growth-curve';
import { SoulNarrativeCard } from '@/components/soul/soul-narrative-card';
import { SoulPanel } from '@/components/soul/soul-panel';
import { WorldSelector, countFragmentsByWorld } from '@/components/soul/world-selector';
import { EmptyState } from '@/components/ui/states';
import {
  SOUL_REFETCH_MS,
  useMemoryFragments,
  useNarrativeWorlds,
  useReflectSoulNarrative,
  useSoulNarrative,
} from '@/api/queries';
import type { MemoryFragmentInfo } from '@42ch/nexus-contracts';

export function SoulSection({
  creatorId,
  onFilterFragments,
}: {
  creatorId: string;
  onFilterFragments: (keyword: string | null) => void;
}) {
  // World projection: null = "All worlds" (whole Creator SOUL); a world_id
  // narrows keyword/drift + growth + narrative to that world's subset.
  const [selectedWorld, setSelectedWorld] = useState<string | null>(null);

  // Workspace-scoped world list: drives the selector titles and includes
  // Work-backed worlds even when they have zero fragments.
  const worlds = useNarrativeWorlds();

  // Whole-creator fragments: drives fragment-count badges in the selector and
  // is the active view when "All worlds" is selected.
  const wholeFragments = useMemoryFragments(creatorId, undefined, {
    refetchInterval: SOUL_REFETCH_MS,
  });

  // Active view: the whole list when "All worlds", the world subset when a
  // world is selected. Same key as `wholeFragments` when no world is selected,
  // so TanStack dedupes (one fetch); a distinct key only when a world is picked.
  const activeFragments = useMemoryFragments(
    creatorId,
    selectedWorld ? { world_id: selectedWorld } : undefined,
    { refetchInterval: SOUL_REFETCH_MS },
  );

  // V1.82: narrative follows the selected scope. The query key includes
  // `world_id` so the card re-renders when the selector changes and there is
  // exactly one active observer per scope (no duplicate poll timers).
  const narrative = useSoulNarrative(creatorId, selectedWorld);
  const reflect = useReflectSoulNarrative();

  const wholeList: MemoryFragmentInfo[] = wholeFragments.data?.fragments ?? [];
  const fragmentCounts = countFragmentsByWorld(wholeList);

  const activeList: MemoryFragmentInfo[] = activeFragments.data?.fragments ?? [];
  const isWorldSubset = selectedWorld !== null;
  const isSubsetEmpty =
    isWorldSubset && !activeFragments.isLoading && !activeFragments.isError && activeList.length === 0;

  const isLoading = worlds.isLoading || wholeFragments.isLoading;

  return (
    <section data-testid="memory-soul-section">
      <div className="mb-3 flex flex-wrap items-center justify-between gap-3">
        <div>
          <h2 className="text-heading-16 text-gray-1000">SOUL</h2>
          <p className="text-copy-13 text-gray-700">
            The themes your creative work has internalized, and how they shift over time.
          </p>
        </div>
        <WorldSelector
          worlds={worlds.data ?? []}
          fragmentCounts={fragmentCounts}
          selectedWorld={selectedWorld}
          onSelect={setSelectedWorld}
          disabled={isLoading}
        />
      </div>

      <div className="mb-6 rounded-card border border-gray-alpha-400 bg-background-100 p-4 shadow-card">
        <SoulNarrativeCard
          narrative={narrative.data}
          isLoading={narrative.isLoading}
          isReflecting={reflect.isPending}
          onReflect={() => reflect.mutate({ creatorId, worldId: selectedWorld })}
          scope={isWorldSubset ? 'world' : 'creator'}
        />
      </div>

      {isSubsetEmpty ? (
        <div data-testid="soul-world-subset-empty" className="py-4">
          <EmptyState
            title="No fragments in this world yet"
            description="Your Creator SOUL is still shaped by your work here when fragments arrive."
          />
        </div>
      ) : (
        <div className="flex flex-col gap-8">
          <SoulPanel fragmentsQuery={activeFragments} onFilterFragments={onFilterFragments} />
          <div className="flex flex-col gap-3 border-t border-gray-alpha-400 pt-4">
            <h3 className="text-heading-16 text-gray-1000">Growth</h3>
            <GrowthCurve fragments={activeList} />
          </div>
        </div>
      )}
    </section>
  );
}
