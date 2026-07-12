import { describe, it, expect, vi, afterEach } from 'vitest';

vi.mock('./i18n/active-locale', () => ({
  getActiveLocale: vi.fn(() => 'en' as const),
}));

import { formatDateTime, formatDate, formatUtcAndLocal, humanizeStatus, IntlFormatterCache } from './format';
import { getActiveLocale } from './i18n/active-locale';

const ISO = '2026-03-15T14:30:00Z';
const DASH = String.fromCharCode(8212);

describe('format.ts locale wiring', () => {
  afterEach(() => {
    vi.clearAllMocks();
  });

  describe('formatDateTime', () => {
    it('calls getActiveLocale()', () => {
      vi.mocked(getActiveLocale).mockReturnValue('en');
      const result = formatDateTime(ISO);
      expect(result).not.toBe(DASH);
      expect(getActiveLocale).toHaveBeenCalled();
    });

    it('returns dash for falsy input', () => {
      expect(formatDateTime(null)).toBe(DASH);
      expect(formatDateTime(undefined)).toBe(DASH);
    });
  });

  describe('formatDate', () => {
    it('calls getActiveLocale()', () => {
      vi.mocked(getActiveLocale).mockReturnValue('en');
      const result = formatDate(ISO);
      expect(result).not.toBe(DASH);
      expect(getActiveLocale).toHaveBeenCalled();
    });

    it('returns dash for falsy input', () => {
      expect(formatDate(null)).toBe(DASH);
    });
  });

  describe('formatUtcAndLocal', () => {
    it('utc uses en-US explicitly', () => {
      vi.mocked(getActiveLocale).mockReturnValue('zh-CN');
      const { utc } = formatUtcAndLocal(ISO);
      expect(utc).not.toBe(DASH);
      expect(utc).not.toBe(ISO);
    });

    it('local calls getActiveLocale()', () => {
      vi.mocked(getActiveLocale).mockReturnValue('zh-CN');
      const { local } = formatUtcAndLocal(ISO);
      expect(local).not.toBe(DASH);
      expect(getActiveLocale).toHaveBeenCalled();
    });

    it('returns dashes for falsy input', () => {
      expect(formatUtcAndLocal(null)).toEqual({ utc: DASH, local: DASH });
    });
  });

  describe('humanizeStatus', () => {
    it('returns dash for falsy input', () => {
      expect(humanizeStatus(null)).toBe(DASH);
      expect(humanizeStatus(undefined)).toBe(DASH);
    });

    it('returns a non-empty string for valid status', () => {
      const result = humanizeStatus('active');
      expect(result).toBeTruthy();
      expect(typeof result).toBe('string');
    });
  });
});

describe('IntlFormatterCache', () => {
  it('reuses DateTimeFormat instances for the same locale and options', () => {
    const cache = new IntlFormatterCache();
    const a = cache.getDateTimeFormat('en', { dateStyle: 'medium' });
    const b = cache.getDateTimeFormat('en', { dateStyle: 'medium' });
    expect(a).toBe(b);
  });

  it('creates different DateTimeFormat instances for different locales', () => {
    const cache = new IntlFormatterCache();
    const a = cache.getDateTimeFormat('en', { dateStyle: 'medium' });
    const b = cache.getDateTimeFormat('zh-CN', { dateStyle: 'medium' });
    expect(a).not.toBe(b);
  });

  it('creates different DateTimeFormat instances for different options', () => {
    const cache = new IntlFormatterCache();
    const a = cache.getDateTimeFormat('en', { dateStyle: 'medium' });
    const b = cache.getDateTimeFormat('en', { dateStyle: 'long' });
    expect(a).not.toBe(b);
  });

  it('reuses RelativeTimeFormat instances for the same locale and options', () => {
    const cache = new IntlFormatterCache();
    const a = cache.getRelativeTimeFormat('en', { numeric: 'auto' });
    const b = cache.getRelativeTimeFormat('en', { numeric: 'auto' });
    expect(a).toBe(b);
  });

  it('treats option property order as identical for the cache key', () => {
    const cache = new IntlFormatterCache();
    const a = cache.getDateTimeFormat('en', { dateStyle: 'medium', timeStyle: 'short' });
    const b = cache.getDateTimeFormat('en', { timeStyle: 'short', dateStyle: 'medium' });
    expect(a).toBe(b);
  });
});
