import { describe, it, expect, vi, afterEach } from 'vitest';

vi.mock('./i18n/active-locale', () => ({
  getActiveLocale: vi.fn(() => 'en' as const),
}));

import { formatDateTime, formatDate, formatUtcAndLocal, humanizeStatus } from './format';
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
