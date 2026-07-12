import { getActiveLocale } from './i18n/active-locale';
import { i18n } from './i18n/config';

/**
 * Per-locale cache for Intl formatters used by the formatting helpers.
 *
 * The key includes both the locale and the serialized format options so that
 * callers with different dateStyle/timeStyle combos get distinct formatters.
 */
export class IntlFormatterCache {
  private dateTimeFormats = new Map<string, Intl.DateTimeFormat>();
  private relativeTimeFormats = new Map<string, Intl.RelativeTimeFormat>();

  private static makeKey(locale: string, options: object): string {
    const sorted = Object.keys(options)
      .sort()
      .reduce<Record<string, unknown>>((acc, key) => {
        acc[key] = (options as Record<string, unknown>)[key];
        return acc;
      }, {});
    return `${locale}:${JSON.stringify(sorted)}`;
  }

  getDateTimeFormat(locale: string, options: Intl.DateTimeFormatOptions): Intl.DateTimeFormat {
    const key = IntlFormatterCache.makeKey(locale, options);
    let formatter = this.dateTimeFormats.get(key);
    if (!formatter) {
      formatter = new Intl.DateTimeFormat(locale, options);
      this.dateTimeFormats.set(key, formatter);
    }
    return formatter;
  }

  getRelativeTimeFormat(locale: string, options: Intl.RelativeTimeFormatOptions): Intl.RelativeTimeFormat {
    const key = IntlFormatterCache.makeKey(locale, options);
    let formatter = this.relativeTimeFormats.get(key);
    if (!formatter) {
      formatter = new Intl.RelativeTimeFormat(locale, options);
      this.relativeTimeFormats.set(key, formatter);
    }
    return formatter;
  }
}

const formatterCache = new IntlFormatterCache();

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
  return formatterCache.getDateTimeFormat(getActiveLocale(), {
    dateStyle: 'medium',
    timeStyle: 'short',
  }).format(ms);
}

/** Format an ISO timestamp as a local date only (e.g. "Jun 25, 2026"). */
export function formatDate(iso: string | undefined | null): string {
  if (!iso) return '—';
  const ms = Date.parse(iso);
  if (Number.isNaN(ms)) return iso;
  return formatterCache.getDateTimeFormat(getActiveLocale(), { dateStyle: 'medium' }).format(ms);
}

/**
 * Format an ISO timestamp as a relative "time ago" string.
 * Falls back to the absolute local time when older than ~30 days.
 */
export function formatRelative(iso: string | undefined | null): string {
  if (!iso) return '—';
  const ms = Date.parse(iso);
  if (Number.isNaN(ms)) return iso;
  const diffSec = Math.round((Date.now() - ms) / 1000);
  const rtf = formatterCache.getRelativeTimeFormat(getActiveLocale(), { numeric: 'auto' });
  if (diffSec < 45) return rtf.format(-diffSec, 'second');
  if (diffSec < 3600) return rtf.format(-Math.round(diffSec / 60), 'minute');
  if (diffSec < 86400) return rtf.format(-Math.round(diffSec / 3600), 'hour');
  if (diffSec < 30 * 86400) return rtf.format(-Math.round(diffSec / 86400), 'day');
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
  const utc = formatterCache.getDateTimeFormat('en-US', {
    timeZone: 'UTC',
    dateStyle: 'medium',
    timeStyle: 'short',
  }).format(ms);
  const local = formatterCache.getDateTimeFormat(getActiveLocale(), {
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

/** Title-case helper for unknown status values without a catalog entry. */
function titleCaseStatus(value: string): string {
  return value
    .replace(/[_-]+/g, ' ')
    .split(' ')
    .map((word) => (word.length === 0 ? word : word[0]!.toUpperCase() + word.slice(1)))
    .join(' ');
}

/**
 * Localize a snake_case / kebab-case status for badges/labels.
 *
 * Looks up `common.status.<value>` in the active catalog; if no translation
 * exists, falls back to title-casing the raw value (legacy behavior for
 * unrecognized daemon free-strings). zh-CN does not receive English title-case.
 */
export function humanizeStatus(value: string | undefined | null): string {
  if (!value) return '—';
  const key = `status.${value}`;
  const translated = i18n.t(key, { ns: 'common', defaultValue: value });
  return translated === value ? titleCaseStatus(value) : translated;
}
