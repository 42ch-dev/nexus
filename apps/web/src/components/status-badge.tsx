import { Badge, type BadgeProps } from '@/components/ui/badge';
import { humanizeStatus } from '@/lib/format';

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
