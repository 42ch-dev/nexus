/**
 * MaturationIndicators — V1.79 Author Reflection (Track A / P0).
 *
 * In-context lightweight maturation indicators rendered from EXISTING
 * read-only data — no new backend route, no write path. Three signals:
 *
 *   1. Chapter completion-state badge — reuses {@link ChapterStatusBadge}
 *      (DESIGN.md §Data Table chapter-status mapping; the
 *      `reading-maturation-badge.chapter-completion-state` token documents this
 *      basis).
 *   2. World KB density count — key blocks for the Work's World, via
 *      {@link useWorldKbDensity} (getWorldKbGraph entity count).
 *   3. Open-findings count — non-terminal findings for the chapter, via
 *      {@link useOpenFindingsCount} (listFindings with comma-separated status).
 *
 * DESIGN.md §reading-maturation-badge tokens: KB density uses the teal
 * (informational) badge semantics; open-findings uses amber (needs-attention)
 * when count > 0, neutral when zero. Counts are interpretable without tooltips.
 */
import { BookOpen, Flag } from 'lucide-react';

import { ChapterStatusBadge } from '@/components/status-badge';
import { useOpenFindingsCount, useWorldKbDensity } from '@/components/reading/reading-hooks';
import { cn } from '@/lib/utils';
import type { ChapterStatus } from '@42ch/nexus-contracts';

interface MaturationIndicatorsProps {
  workId: string;
  chapter: number;
  status: ChapterStatus | undefined;
}

export function MaturationIndicators({ workId, chapter, status }: MaturationIndicatorsProps) {
  const findings = useOpenFindingsCount(workId, chapter);
  const kb = useWorldKbDensity(workId);

  return (
    <div className="flex flex-wrap items-center gap-2" aria-label="Chapter maturation indicators">
      <ChapterStatusBadge status={status} />
      <CountBadge
        icon={<BookOpen className="h-3.5 w-3.5" aria-hidden />}
        label="key blocks"
        count={kb.count}
        loading={kb.isLoading}
        variant="info"
      />
      <CountBadge
        icon={<Flag className="h-3.5 w-3.5" aria-hidden />}
        label="open findings"
        count={findings.count}
        loading={findings.isLoading}
        truncated={findings.truncated}
        variant={findings.count > 0 ? 'attention' : 'neutral'}
      />
    </div>
  );
}

interface CountBadgeProps {
  icon: React.ReactNode;
  label: string;
  count: number | null;
  loading: boolean;
  /**
   * When true, `count` is a lower bound (more rows exist on unloaded pages).
   * Renders an honest "N+" label instead of an exact-looking but clipped
   * integer. The `PaginationInfo` envelope has no `total`, so this is the
   * accurate representation of a truncated count (qc3 W-QC3-002).
   */
  truncated?: boolean;
  variant: 'info' | 'attention' | 'neutral';
}

/**
 * Compact count badge — DESIGN.md §reading-maturation-badge.base (20px, pill,
 * label-12). `info` = teal/informational; `attention` = amber/needs-attention;
 * `neutral` = quiet. Reuses the same `color-mix` + base-var pattern as the
 * V1.77/V1.78 badges so colors stay correct in light and dark. Count is never
 * color-only — the label and count travel together.
 */
function CountBadge({ icon, label, count, loading, truncated = false, variant }: CountBadgeProps) {
  const variantClass = VARIANT_CLASSES[variant];
  const text = count === null ? '—' : truncated ? `${count}+` : String(count);
  return (
    <span
      className={cn(
        'inline-flex h-5 items-center gap-1 whitespace-nowrap rounded-pill border px-1.5 text-label-12',
        variantClass,
      )}
      aria-label={`${text} ${label}`}
    >
      {icon}
      <span className="tabular-nums font-semibold">{loading ? '…' : text}</span>
      <span className="sr-only">{label}</span>
      <span aria-hidden className="hidden sm:inline font-normal">
        {label}
      </span>
    </span>
  );
}

const VARIANT_CLASSES: Record<CountBadgeProps['variant'], string> = {
  // teal-700 @10% / teal-1000 / teal-700 @30% — informational (DESIGN.md light;
  // the base CSS vars swap to dark values under .dark).
  info: 'bg-[color-mix(in_srgb,var(--color-teal-700)_10%,transparent)] text-teal-1000 border-[color-mix(in_srgb,var(--color-teal-700)_30%,transparent)]',
  // amber-700 @12% / amber-1000 / amber-700 @30% — needs attention.
  attention:
    'bg-[color-mix(in_srgb,var(--color-amber-700)_12%,transparent)] text-amber-1000 border-[color-mix(in_srgb,var(--color-amber-700)_30%,transparent)]',
  // neutral — quiet zero state.
  neutral: 'bg-gray-alpha-100 text-gray-900 border-gray-alpha-300',
};
