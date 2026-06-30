import { Badge, type BadgeProps } from '@/components/ui/badge';
import { humanizeStatus } from '@/lib/format';
import { FINDING_STATUSES, type FindingStatus } from '@/lib/findings-lifecycle';
import { cn } from '@/lib/utils';
import type { ChapterStatus } from '@42ch/nexus-contracts';

/**
 * Map a free-string status to a Badge variant by keyword.
 *
 * Local API statuses are free-strings (no enum contract), so we match on
 * known substrings with sensible fallbacks. DESIGN.md semantic mapping:
 * running/healthy/completed → green; queued/informational → teal;
 * warning/needs-review → amber; failed/error → red; everything else → neutral.
 */
export function statusVariant(status: string | undefined | null): BadgeProps['variant'] {
  if (!status) return 'neutral';
  const s = status.toLowerCase();
  if (/(^|_)(running|active|healthy|completed|ok|success)($|_)/.test(s)) return 'running';
  if (/(^|_)(queued|pending|info|informational|waiting)($|_)/.test(s)) return 'queued';
  if (/(^|_)(warning|stale|needs_?review|review|paused|partial)($|_)/.test(s)) return 'warning';
  if (/(^|_)(failed|error|critical|fatal|archived|cancelled|canceled)($|_)/.test(s))
    return 'error';
  return 'neutral';
}

/** Severity uses the same mapping but leans stricter on the error band. */
export function severityVariant(severity: string | undefined | null): BadgeProps['variant'] {
  if (!severity) return 'neutral';
  const s = severity.toLowerCase();
  if (/(^|_)(critical|error|fatal|high)($|_)/.test(s)) return 'error';
  if (/(^|_)(warning|medium)($|_)/.test(s)) return 'warning';
  if (/(^|_)(info|low)($|_)/.test(s)) return 'queued';
  return 'neutral';
}

/**
 * DESIGN.md §Data Table — explicit chapter-status badge mapping.
 *
 * `not_started` neutral, `outlined` queued, `draft` warning, `finalized` running,
 * `published` preset.
 */
export function chapterStatusVariant(status: ChapterStatus | undefined | null): BadgeProps['variant'] {
  switch (status) {
    case 'outlined':
      return 'queued';
    case 'draft':
      return 'warning';
    case 'finalized':
      return 'running';
    case 'published':
      return 'preset';
    case 'not_started':
    default:
      return 'neutral';
  }
}

interface StatusBadgeProps {
  status?: string | null;
  /** Show the raw value verbatim instead of humanizing. */
  raw?: boolean;
  variant?: BadgeProps['variant'];
  className?: string;
}

/** Status pill that humanizes snake_case and maps to a semantic variant. */
export function StatusBadge({ status, raw, variant, className }: StatusBadgeProps) {
  const resolved = variant ?? statusVariant(status);
  return (
    <Badge variant={resolved} className={className}>
      {raw ? status ?? '—' : humanizeStatus(status)}
    </Badge>
  );
}

interface ChapterStatusBadgeProps {
  status?: ChapterStatus | null;
  className?: string;
}

/** Chapter status pill with the DESIGN.md mapping. */
export function ChapterStatusBadge({ status, className }: ChapterStatusBadgeProps) {
  return (
    <Badge variant={chapterStatusVariant(status)} className={className}>
      {humanizeStatus(status)}
    </Badge>
  );
}

/**
 * DESIGN.md §Findings — explicit 6-state finding-status badge mapping.
 *
 * Each finding status gets an intentional, distinct color (the generic
 * `statusVariant` keyword matcher cannot distinguish `in_review` from `resolved`
 * or `wont_fix` from `duplicate`). Colors reuse the established semantic palette
 * (amber=needs triage, teal=reviewed/ready, blue=active review, green=resolved,
 * gray=waived, purple=superseded) via the same `color-mix` pattern as the
 * generic badge variants, so they stay correct in both light and dark.
 */
function findingStatusClasses(status: FindingStatus | string | undefined | null): string {
  switch (status as FindingStatus) {
    case 'open':
      // amber — newly raised, needs triage attention.
      return 'bg-[color-mix(in_srgb,var(--color-amber-700)_12%,transparent)] text-amber-1000 border-[color-mix(in_srgb,var(--color-amber-700)_30%,transparent)]';
    case 'triaged':
      // teal — reviewed, ready to route.
      return 'bg-[color-mix(in_srgb,var(--color-teal-700)_10%,transparent)] text-teal-1000 border-[color-mix(in_srgb,var(--color-teal-700)_30%,transparent)]';
    case 'in_review':
      // blue — actively under master review.
      return 'bg-[color-mix(in_srgb,var(--color-blue-700)_10%,transparent)] text-blue-1000 border-[color-mix(in_srgb,var(--color-blue-700)_30%,transparent)]';
    case 'resolved':
      // green — addressed, positive terminal.
      return 'bg-[color-mix(in_srgb,var(--color-green-700)_10%,transparent)] text-green-1000 border-[color-mix(in_srgb,var(--color-green-700)_30%,transparent)]';
    case 'wont_fix':
      // gray — explicitly waived, quiet terminal.
      return 'bg-gray-alpha-100 text-gray-900 border-gray-alpha-300';
    case 'duplicate':
      // purple — superseded by another finding.
      return 'bg-[color-mix(in_srgb,var(--color-purple-700)_10%,transparent)] text-purple-1000 border-[color-mix(in_srgb,var(--color-purple-700)_30%,transparent)]';
    default:
      return 'bg-gray-alpha-100 text-gray-900 border-gray-alpha-300';
  }
}

interface FindingStatusBadgeProps {
  status?: string | null;
  className?: string;
}

/** Finding status pill with the DESIGN.md §Findings 6-state mapping. */
export function FindingStatusBadge({ status, className }: FindingStatusBadgeProps) {
  return (
    <Badge className={cn(findingStatusClasses(status), className)}>
      {humanizeStatus(status)}
    </Badge>
  );
}

/** Re-export the status set for affordance rendering (row actions / dropdowns). */
export { FINDING_STATUSES };
