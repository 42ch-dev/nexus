import { createContext, useContext, useEffect, useMemo, useState, type ReactNode } from 'react';

import { i18n } from '@/lib/i18n/config';
import type { LocalePreference, ResolvedLocale } from '@/lib/i18n/config';

export type { LocalePreference, ResolvedLocale } from '@/lib/i18n/config';

/**
 * Locale provider — localStorage preference + system-locale resolution.
 *
 * Mirrors theme-provider.tsx: preference persists in localStorage, defaults to
 * the OS/browser language via the `'system'` state, and side effects update
 * both the i18next singleton and the `<html lang>` attribute.
 */
interface LocaleContextValue {
  preference: LocalePreference;
  /** Effective resolved locale; always `'en'` or `'zh-CN'` even when `preference` is `'system'`. */
  resolvedLocale: ResolvedLocale;
  setPreference: (preference: LocalePreference) => void;
}

const LocaleContext = createContext<LocaleContextValue | null>(null);
const STORAGE_KEY = 'nexus-web-locale';

function readStoredLocale(): LocalePreference | null {
  if (typeof window === 'undefined') return null;
  const stored = window.localStorage.getItem(STORAGE_KEY);
  if (stored === 'system' || stored === 'en' || stored === 'zh-CN') return stored;
  return null;
}

function resolveSystemLocale(): ResolvedLocale {
  if (typeof window === 'undefined') return 'en';
  return navigator.language.startsWith('zh') ? 'zh-CN' : 'en';
}

function resolveLocale(preference: LocalePreference): ResolvedLocale {
  if (preference !== 'system') return preference;
  return resolveSystemLocale();
}

export function LocaleProvider({ children }: { children: ReactNode }) {
  const [preference, setPreferenceState] = useState<LocalePreference>(() => readStoredLocale() ?? 'system');
  const [resolvedLocale, setResolvedLocale] = useState<ResolvedLocale>(() => resolveLocale(preference));

  useEffect(() => {
    i18n.changeLanguage(resolvedLocale);
    document.documentElement.lang = resolvedLocale;
    window.localStorage.setItem(STORAGE_KEY, preference);
  }, [preference, resolvedLocale]);

  useEffect(() => {
    if (preference !== 'system') return;
    const handler = () => setResolvedLocale(resolveSystemLocale());
    handler();
    window.addEventListener('languagechange', handler);
    return () => window.removeEventListener('languagechange', handler);
  }, [preference]);

  const value = useMemo<LocaleContextValue>(
    () => ({
      preference,
      resolvedLocale,
      setPreference: (next) => {
        setPreferenceState(next);
        setResolvedLocale(resolveLocale(next));
      },
    }),
    [preference, resolvedLocale],
  );

  return <LocaleContext.Provider value={value}>{children}</LocaleContext.Provider>;
}

export function useLocale(): LocaleContextValue {
  const ctx = useContext(LocaleContext);
  if (!ctx) throw new Error('useLocale must be used within a LocaleProvider');
  return ctx;
}
