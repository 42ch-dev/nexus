/**
 * Formatting helpers for the Control Room + Setup screens.
 *
 * All times are formatted in the user's local timezone for display, with an
 * accompanying UTC label where the runtime emits UTC (schedule next-fire).
 * DESIGN.md §Voice & Content: avoid protocol jargon; surface plain values.
 */

/** Format an ISO timestamp as a local date + time (e.g. "Jun 25, 2026, 9:14 AM"). */
export function formatDateTime(iso: string | undefined | null): string {
  if (!iso) return '—';
  const ms = Date.parse(iso);
  if (Number.isNaN(ms)) return iso;
  return new Intl.DateTimeFormat(undefined, {
    dateStyle: 'medium',
    timeStyle: 'short',
  }).format(ms);
}

/** Format an ISO timestamp as a local date only (e.g. "Jun 25, 2026"). */
export function formatDate(iso: string | undefined | null): string {
  if (!iso) return '—';
  const ms = Date.parse(iso);
  if (Number.isNaN(ms)) return iso;
  return new Intl.DateTimeFormat(undefined, { dateStyle: 'medium' }).format(ms);
}

/**
 * Format an ISO timestamp as a relative "time ago" string (e.g. "3h ago").
 * Falls back to the absolute local time when older than ~30 days.
 */
export function formatRelative(iso: string | undefined | null): string {
  if (!iso) return '—';
  const ms = Date.parse(iso);
  if (Number.isNaN(ms)) return iso;
  const diffMs = Date.now() - ms;
  const sec = Math.round(diffMs / 1000);
  const min = Math.round(sec / 60);
  const hr = Math.round(min / 60);
  const day = Math.round(hr / 24);
  if (sec < 45) return 'just now';
  if (min < 60) return `${min}m ago`;
  if (hr < 24) return `${hr}h ago`;
  if (day < 30) return `${day}d ago`;
  return formatDate(iso);
}

/**
 * Render a timestamp in both UTC and local time. Used by the schedule view to
 * give the CLI `creator works cron` parity (UTC) plus the author's local view.
 */
export function formatUtcAndLocal(iso: string | undefined | null): { utc: string; local: string } {
  const fallback = '—';
  if (!iso) return { utc: fallback, local: fallback };
  const ms = Date.parse(iso);
  if (Number.isNaN(ms)) return { utc: iso, local: iso };
  const utc = new Intl.DateTimeFormat('en-US', {
    timeZone: 'UTC',
    dateStyle: 'medium',
    timeStyle: 'short',
  }).format(ms);
  const local = new Intl.DateTimeFormat(undefined, {
    dateStyle: 'medium',
    timeStyle: 'short',
  }).format(ms);
  return { utc, local };
}

/** Shorten an id for table display (head + tail with an ellipsis). */
export function shortId(id: string | undefined | null, head = 8, tail = 4): string {
  if (!id) return '—';
  if (id.length <= head + tail + 1) return id;
  return `${id.slice(0, head)}…${id.slice(-tail)}`;
}

/** Title-case a snake_case / kebab-case status for badges/labels. */
export function humanizeStatus(value: string | undefined | null): string {
  if (!value) return '—';
  return value
    .replace(/[_-]+/g, ' ')
    .split(' ')
    .map((word) => (word.length === 0 ? word : word[0]!.toUpperCase() + word.slice(1)))
    .join(' ');
}
