/**
 * Free-string status → Badge variant (the app-wide keyword matcher).
 *
 * Kept as a pure, import-free module so the i18n-free, contract-free Design
 * Studio chrome extracts can share it; `components/status-badge.tsx`
 * re-exports it for App call sites.
 *
 * Daemon API statuses are free-strings (no enum contract), so we match on
 * known substrings with sensible fallbacks. DESIGN.md semantic mapping:
 * running/healthy/completed → green; queued/informational → teal;
 * warning/needs-review → amber; failed/error → red; everything else → neutral.
 */
import type { BadgeProps } from '@42ch/nexus-ui';

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
