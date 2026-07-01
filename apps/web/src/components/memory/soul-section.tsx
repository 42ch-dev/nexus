/**
 * SoulSection — extracted from MemoryPage (R-V179P1-QC1-001).
 *
 * SOUL personality visualization wrapper (V1.79 P1 — Track B §D). Reuses the
 * existing fragments query — fragments already carry `keywords` + `created_at`
 * (additive DTO), so no new endpoint/query/client method. The SOUL panel reads
 * the same data the fragments browser does; click-to-filter is surfaced back to
 * the page shell via onFilterFragments.
 */
import { SoulPanel } from '@/components/soul/soul-panel';
import { useMemoryFragments } from '@/api/queries';

export function SoulSection({
  creatorId,
  onFilterFragments,
}: {
  creatorId: string;
  onFilterFragments: (keyword: string | null) => void;
}) {
  const fragments = useMemoryFragments(creatorId);

  return (
    <section data-testid="memory-soul-section">
      <div className="mb-3 flex flex-wrap items-center justify-between gap-2">
        <h2 className="text-heading-16 text-gray-1000">SOUL</h2>
        <p className="text-copy-13 text-gray-700">
          The themes your creative work has internalized, and how they shift over time.
        </p>
      </div>
      <SoulPanel fragmentsQuery={fragments} onFilterFragments={onFilterFragments} />
    </section>
  );
}
