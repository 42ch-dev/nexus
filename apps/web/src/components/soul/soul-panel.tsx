import { useState } from 'react';

import { KeywordFrequency } from '@/components/soul/keyword-frequency';
import {
  aggregateKeywordFrequency,
  bucketByTime,
  densityFor,
} from '@/components/soul/soul-stats';
import { TemporalDrift } from '@/components/soul/temporal-drift';
import { EmptyState, ErrorState, LoadingState } from '@/components/ui/states';
import type { UseQueryResult } from '@tanstack/react-query';
import type { ListMemoryFragmentsResponse, MemoryFragmentInfo } from '@42ch/nexus-contracts';

/**
 * SOUL personality visualization panel (V1.79 P1 — Track B §D).
 *
 * Integrates the keyword-clusters + temporal-drift surfaces into the V1.78
 * Memory page as a new section (no new top-level route). The data source is the
 * existing `useMemoryFragments` query — fragments already carry `keywords` +
 * `created_at` as of the additive DTO extension, so this panel adds no new
 * endpoint, query key, or client method.
 *
 * Density-state branching (plan §F) drives the treatment:
 *  - `empty`    — zero fragments: empathetic forward-looking copy, no chart.
 *  - `low-data` — a few fragments: a simple frequency list (honest, not a forced
 *                 one-node cluster chart that would look broken).
 *  - `rich`     — full frequency + temporal drift timeline with growth folded in.
 *
 * The click-to-filter affordance on keyword rows drives the optional
 * `onFilterFragments` callback so the parent's fragments browser can scope to
 * the selected theme (the callback is a no-op when unwired).
 */
export function SoulPanel({
  fragmentsQuery,
  onFilterFragments,
}: {
  fragmentsQuery: UseQueryResult<ListMemoryFragmentsResponse>;
  onFilterFragments?: (keyword: string | null) => void;
}) {
  const [selectedKeyword, setSelectedKeyword] = useState<string | null>(null);

  if (fragmentsQuery.isError) {
    return (
      <ErrorState
        description="Could not load your SOUL visualization."
        onRetry={() => fragmentsQuery.refetch()}
      />
    );
  }
  if (fragmentsQuery.isLoading) {
    return <LoadingState label="Loading your SOUL…" />;
  }

  const fragments: MemoryFragmentInfo[] = fragmentsQuery.data?.fragments ?? [];
  const density = densityFor(fragments.length);

  const handleSelect = (kw: string | null) => {
    setSelectedKeyword(kw);
    onFilterFragments?.(kw);
  };

  if (density === 'empty') {
    return (
      <div data-testid="soul-empty-state">
        <EmptyState
          title="Your SOUL is just beginning"
          description="As you write and review, Nexus captures fragments of your creative identity — themes, patterns, obsessions — and maps them here. Come back after your first review session."
        />
      </div>
    );
  }

  if (density === 'low-data') {
    const count = fragments.length;
    return (
      <div data-testid="soul-low-data" className="flex flex-col gap-4">
        <p className="text-copy-14 text-gray-900">
          Your SOUL is taking shape. {count} fragment{count === 1 ? '' : 's'}{' '}
          captured so far — enough to see early themes. Keep writing and reviewing
          to build richer patterns.
        </p>
        <KeywordFrequency
          counts={aggregateKeywordFrequency(fragments)}
          selectedKeyword={selectedKeyword}
          onSelectKeyword={onFilterFragments ? handleSelect : undefined}
        />
      </div>
    );
  }

  // rich
  const buckets = bucketByTime(fragments);
  return (
    <div data-testid="soul-rich" className="flex flex-col gap-6">
      <p className="text-copy-13 text-gray-700">Your creative themes over time.</p>
      {buckets.length >= 2 ? (
        <TemporalDrift buckets={buckets} />
      ) : (
        // Many fragments but a single time moment: render the frequency list
        // rather than a single-point timeline that would look broken.
        <KeywordFrequency
          counts={aggregateKeywordFrequency(fragments)}
          selectedKeyword={selectedKeyword}
          onSelectKeyword={onFilterFragments ? handleSelect : undefined}
        />
      )}
      <div className="flex flex-col gap-3 border-t border-gray-alpha-400 pt-4">
        <h3 className="text-heading-16 text-gray-1000">Theme frequency</h3>
        <KeywordFrequency
          counts={aggregateKeywordFrequency(fragments)}
          selectedKeyword={selectedKeyword}
          onSelectKeyword={onFilterFragments ? handleSelect : undefined}
        />
      </div>
    </div>
  );
}
