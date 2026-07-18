import { Badge, type BadgeProps } from '@/components/ui/badge';
import { humanizeStatus } from '@/lib/format';
import { FINDING_STATUSES, type FindingStatus } from '@/lib/findings-lifecycle';
import { cn } from '@/lib/utils';
import type { ChapterStatus } from '@42ch/nexus-contracts';
import { useTranslation } from 'react-i18next';

/**
 * Map a free-string status to a Badge variant by keyword.
 *
 * Daemon API statuses are free-strings (no enum contract), so we match on
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
  const { t } = useTranslation('canvas');
  const label = status ? t(`chapter.status.${status}` as const) : humanizeStatus(status);
  return (
    <Badge variant={chapterStatusVariant(status)} className={className}>
      {label}
    </Badge>
  );
}

/**
 * DESIGN.md §Findings — explicit 6-state finding-status badge mapping.
 *
 * Each finding status gets an intentional, distinct color (the generic
 * `statusVariant` keyword matcher cannot distinguish `in_review` from `resolved`
 * or `wont_fix` from `duplicate`). Colors consume the DESIGN.md frontmatter
 * `components.finding-status-pill` tokens (projected as
 * `--color-finding-status-*` CSS vars via @nexus/design-tokens), so they stay
 * correct in both light and dark themes.
 */
function findingStatusClasses(status: FindingStatus | string | undefined | null): string {
  switch (status as FindingStatus) {
    case 'open':
      // amber — newly raised, needs triage attention.
      return 'bg-finding-status-open-bg text-finding-status-open-text border-finding-status-open-border';
    case 'triaged':
      // teal — reviewed, ready to route.
      return 'bg-finding-status-triaged-bg text-finding-status-triaged-text border-finding-status-triaged-border';
    case 'in_review':
      // blue — actively under master review.
      return 'bg-finding-status-in-review-bg text-finding-status-in-review-text border-finding-status-in-review-border';
    case 'resolved':
      // green — addressed, positive terminal.
      return 'bg-finding-status-resolved-bg text-finding-status-resolved-text border-finding-status-resolved-border';
    case 'wont_fix':
      // gray — explicitly waived, quiet terminal.
      return 'bg-finding-status-wont-fix-bg text-finding-status-wont-fix-text border-finding-status-wont-fix-border';
    case 'duplicate':
      // purple — superseded by another finding.
      return 'bg-finding-status-duplicate-bg text-finding-status-duplicate-text border-finding-status-duplicate-border';
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
  const { t } = useTranslation('findings');
  const normalized = (status ?? '').toLowerCase() as FindingStatus;
  const label = FINDING_STATUSES.includes(normalized)
    ? t(`status.${normalized}` as const)
    : humanizeStatus(status);
  return (
    <Badge className={cn(findingStatusClasses(status), className)}>
      {label}
    </Badge>
  );
}

/** Re-export the status set for affordance rendering (row actions / dropdowns). */
export { FINDING_STATUSES };
