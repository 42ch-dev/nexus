/**
 * SoulSection — V1.79 SOUL visualization wrapper, extended in V1.81 (Creator
 * SOUL Maturation).
 *
 * V1.79: keyword clusters + temporal drift over internalized fragments,
 * reusing the existing fragments query (fragments carry `keywords` +
 * `created_at` + `world_id`). Click-to-filter is surfaced back to the page
 * shell via `onFilterFragments`.
 *
 * V1.81 additions (web-ui.md §26):
 *  - **Narrative card** (SP-1): the headline surface, rendered above the
 *    keyword/drift viz. World-agnostic (Creator-level whole) — the narrative
 *    query is NOT re-scoped by the world selector.
 *  - **World projection selector** (SP-2): re-scopes the keyword/drift +
 *    growth-curve to a world's fragment subset. "All worlds" (default) is the
 *    whole Creator SOUL. World options are derived from the whole-creator
 *    fragment list (see world-selector.tsx `simplify` note on title resolution).
 *  - **Growth-curve** (SP-3): cumulative fragment growth, independent of the
 *    temporal-drift timeline.
 *  - **Auto-refresh** (SP-4): the SOUL fragments query polls on the
 *    `SOUL_REFETCH_MS` cadence; the narrative + fragments queries invalidate
 *    after a review mutation (wired in queries.ts `useReviewMemory`).
 *
 * Layout: the narrative card sits at the top (the first thing the author sees
 * on the SOUL tab); below it, the world-scoped keyword/drift viz + growth-curve.
 * When a world projection returns zero fragments, the whole viz area shows the
 * honest subset-empty copy instead of forcing an empty chart.
 */
import { useState } from 'react';

import { GrowthCurve } from '@/components/soul/growth-curve';
import { SoulNarrativeCard } from '@/components/soul/soul-narrative-card';
import { SoulPanel } from '@/components/soul/soul-panel';
import {
  WorldSelector,
  deriveWorldOptions,
} from '@/components/soul/world-selector';
import { EmptyState } from '@/components/ui/states';
import { SOUL_REFETCH_MS, useReflectSoulNarrative, useSoulNarrative } from '@/api/queries';
import { useMemoryFragments } from '@/api/queries';
import type { MemoryFragmentInfo } from '@42ch/nexus-contracts';

export function SoulSection({
  creatorId,
  onFilterFragments,
}: {
  creatorId: string;
  onFilterFragments: (keyword: string | null) => void;
}) {
  // World projection: null = "All worlds" (whole Creator SOUL); a world_id
  // narrows keyword/drift + growth to that world's subset. The narrative card
  // is intentionally NOT re-scoped (world-agnostic by contract).
  const [selectedWorld, setSelectedWorld] = useState<string | null>(null);

  // Whole-creator fragments: drives the world-selector options + fragment-count
  // badges, and is the active view when "All worlds" is selected.
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

  // Narrative is world-agnostic — always the whole-Creator cache.
  const narrative = useSoulNarrative(creatorId);
  const reflect = useReflectSoulNarrative();

  const wholeList: MemoryFragmentInfo[] = wholeFragments.data?.fragments ?? [];
  const worldOptions = deriveWorldOptions(wholeList);

  const activeList: MemoryFragmentInfo[] = activeFragments.data?.fragments ?? [];
  const isWorldSubset = selectedWorld !== null;
  const isSubsetEmpty =
    isWorldSubset && !activeFragments.isLoading && !activeFragments.isError && activeList.length === 0;

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
          options={worldOptions}
          selectedWorld={selectedWorld}
          onSelect={setSelectedWorld}
          disabled={wholeFragments.isLoading}
        />
      </div>

      <div className="mb-6 rounded-card border border-gray-alpha-400 bg-background-100 p-4 shadow-card">
        <SoulNarrativeCard
          narrative={narrative.data}
          isLoading={narrative.isLoading}
          isReflecting={reflect.isPending}
          onReflect={() => reflect.mutate(creatorId)}
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
