import { i18n } from '@/lib/i18n/config';
import type { ResolvedLocale } from '@/lib/i18n/config';

/**
 * Thin re-export of the currently resolved active locale for P1 `format.ts`.
 *
 * Returns the locale i18next believes is active; if the value is not one of the
 * supported resolved locales, falls back to English.
 */
export function getActiveLocale(): ResolvedLocale {
  const lang = i18n.language;
  return lang === 'zh-CN' ? 'zh-CN' : 'en';
}
