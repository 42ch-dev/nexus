import { createContext, useContext, useEffect, useMemo, useState, type ReactNode } from 'react';

/**
 * Theme provider for the Design Studio.
 *
 * DESIGN.md uses identical token names in both themes; only the CSS-variable
 * values swap (see index.css). Dark mode applies the `.dark` class on `<html>`
 * (Tailwind `class` strategy). Preference persists in localStorage and defaults
 * to the OS `prefers-color-scheme` via the `'system'` state.
 *
 * When `forcedTheme` is set (iframe embed), storage and OS listeners are
 * bypassed so parent chrome cannot overwrite the frame theme.
 */
export type Theme = 'light' | 'dark' | 'system';

interface ThemeContextValue {
  theme: Theme;
  /** Effective resolved theme; always `'light'` or `'dark'`. */
  resolvedTheme: 'light' | 'dark';
  setTheme: (theme: Theme) => void;
  toggleTheme: () => void;
}

const ThemeContext = createContext<ThemeContextValue | null>(null);
export const STORAGE_KEY = 'nexus-studio-theme';

function readStoredTheme(): Theme | null {
  if (typeof window === 'undefined') return null;
  const stored = window.localStorage.getItem(STORAGE_KEY);
  if (stored === 'light' || stored === 'dark' || stored === 'system') return stored;
  return null;
}

function resolveTheme(theme: Theme): 'light' | 'dark' {
  if (theme !== 'system') return theme;
  if (typeof window === 'undefined') return 'light';
  return window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
}

export function ThemeProvider({
  children,
  forcedTheme,
}: {
  children: ReactNode;
  forcedTheme?: 'light' | 'dark';
}) {
  const isForced = forcedTheme === 'light' || forcedTheme === 'dark';
  const [theme, setThemeState] = useState<Theme>(() =>
    isForced ? forcedTheme : readStoredTheme() ?? 'system',
  );
  const [resolvedTheme, setResolvedTheme] = useState<'light' | 'dark'>(() =>
    isForced ? forcedTheme : resolveTheme(theme),
  );

  useEffect(() => {
    if (isForced) {
      setThemeState(forcedTheme);
      setResolvedTheme(forcedTheme);
      return;
    }
    setResolvedTheme(resolveTheme(theme));
  }, [forcedTheme, isForced, theme]);

  useEffect(() => {
    const root = document.documentElement;
    root.classList.toggle('dark', resolvedTheme === 'dark');
    root.style.colorScheme = resolvedTheme;
    if (isForced) return;
    window.localStorage.setItem(STORAGE_KEY, theme);
  }, [theme, resolvedTheme, isForced]);

  useEffect(() => {
    if (isForced || theme !== 'system') return;
    const media = window.matchMedia('(prefers-color-scheme: dark)');
    const handler = () => setResolvedTheme(media.matches ? 'dark' : 'light');
    handler();
    media.addEventListener('change', handler);
    return () => media.removeEventListener('change', handler);
  }, [theme, isForced]);

  const value = useMemo<ThemeContextValue>(
    () => ({
      theme: isForced ? forcedTheme : theme,
      resolvedTheme: isForced ? forcedTheme : resolvedTheme,
      setTheme: (next) => {
        if (isForced) return;
        setThemeState(next);
        setResolvedTheme(resolveTheme(next));
      },
      toggleTheme: () => {
        if (isForced) return;
        const next = resolvedTheme === 'dark' ? 'light' : 'dark';
        setThemeState(next);
        setResolvedTheme(next);
      },
    }),
    [forcedTheme, isForced, resolvedTheme, theme],
  );

  return <ThemeContext.Provider value={value}>{children}</ThemeContext.Provider>;
}

export function useTheme(): ThemeContextValue {
  const ctx = useContext(ThemeContext);
  if (!ctx) throw new Error('useTheme must be used within a ThemeProvider');
  return ctx;
}
