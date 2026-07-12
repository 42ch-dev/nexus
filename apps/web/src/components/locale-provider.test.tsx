/**
 * LocaleProvider coverage.
 *
 * Mirrors theme-provider.test.tsx to pin the architectural surface P1 inherits:
 * provider mount, default-locale detection (stored → system → fallback),
 * persistence, the `<html lang>` attribute + i18next language sync, and the
 * system-locale listener. Catalog values themselves are tested in T2–T4.
 */
import { act, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { ReactNode } from 'react';

import { i18n } from '@/lib/i18n/config';
import { LocaleProvider, useLocale, type LocalePreference } from '@/components/locale-provider';

const STORAGE_KEY = 'nexus-web-locale';

/** jsdom does not provide a configurable real navigator.language, so each test
 *  installs a deterministic getter on the navigator instance. */
function setNavigatorLanguage(language: string) {
  Object.defineProperty(navigator, 'language', {
    get: () => language,
    configurable: true,
  });
}

/** Surface the preference value from inside the provider for assertions. */
function LocaleProbe({ onPreference }: { onPreference: (p: LocalePreference) => void }) {
  const { preference } = useLocale();
  return <>{(onPreference(preference), null)}</>;
}

/** Surface the resolved locale from inside the provider for assertions. */
function LocaleResolvedProbe({ onResolved }: { onResolved: (r: 'en' | 'zh-CN') => void }) {
  const { resolvedLocale } = useLocale();
  return <>{(onResolved(resolvedLocale), null)}</>;
}

function renderWith(ui: ReactNode) {
  return render(<LocaleProvider>{ui}</LocaleProvider>);
}

beforeEach(() => {
  window.localStorage.clear();
  document.documentElement.removeAttribute('lang');
  // @ts-expect-error - delete the test-defined own property to start fresh
  delete navigator.language;
  setNavigatorLanguage('en');
  void i18n.changeLanguage('en');
});

afterEach(() => {
  // @ts-expect-error - clean up the test-defined navigator override
  delete navigator.language;
});

describe('LocaleProvider mount', () => {
  it('renders its children', () => {
    setNavigatorLanguage('en');
    renderWith(<div>child-content</div>);
    expect(screen.getByText('child-content')).toBeInTheDocument();
  });
});

describe('LocaleProvider default-locale detection', () => {
  it('prefers a stored locale over the system preference', () => {
    setNavigatorLanguage('zh-CN'); // OS says Chinese…
    window.localStorage.setItem(STORAGE_KEY, 'en'); // …but stored says English.
    let current: LocalePreference = 'system';
    renderWith(<LocaleProbe onPreference={(p) => (current = p)} />);
    expect(current).toBe('en');
    expect(i18n.language).toBe('en');
    expect(document.documentElement.lang).toBe('en');
  });

  it('falls back to system when nothing is stored', () => {
    setNavigatorLanguage('zh-CN');
    let current: LocalePreference = 'en';
    let resolved: 'en' | 'zh-CN' = 'en';
    renderWith(
      <>
        <LocaleProbe onPreference={(p) => (current = p)} />
        <LocaleResolvedProbe onResolved={(r) => (resolved = r)} />
      </>,
    );
    expect(current).toBe('system');
    expect(resolved).toBe('zh-CN');
    expect(i18n.language).toBe('zh-CN');
    expect(document.documentElement.lang).toBe('zh-CN');
    expect(window.localStorage.getItem(STORAGE_KEY)).toBe('system');
  });

  it('resolves system to English when the browser language is not Chinese', () => {
    setNavigatorLanguage('fr-FR');
    let resolved: 'en' | 'zh-CN' = 'zh-CN';
    renderWith(<LocaleResolvedProbe onResolved={(r) => (resolved = r)} />);
    expect(resolved).toBe('en');
    expect(document.documentElement.lang).toBe('en');
  });

  it('ignores an unrecognized stored value and falls back to system', () => {
    setNavigatorLanguage('en');
    window.localStorage.setItem(STORAGE_KEY, 'klingon');
    let current: LocalePreference = 'en';
    let resolved: 'en' | 'zh-CN' = 'zh-CN';
    renderWith(
      <>
        <LocaleProbe onPreference={(p) => (current = p)} />
        <LocaleResolvedProbe onResolved={(r) => (resolved = r)} />
      </>,
    );
    expect(current).toBe('system');
    expect(resolved).toBe('en');
  });
});

describe('LocaleProvider setPreference', () => {
  it('persists the preference and updates the resolved locale', () => {
    setNavigatorLanguage('en');
    let setPreference: (p: LocalePreference) => void = () => {};
    renderWith(<LocaleProbeApi onReady={(api) => (setPreference = api.setPreference)} />);

    act(() => setPreference('zh-CN'));
    expect(window.localStorage.getItem(STORAGE_KEY)).toBe('zh-CN');
    expect(i18n.language).toBe('zh-CN');
    expect(document.documentElement.lang).toBe('zh-CN');
  });

  it('switches back to system and follows navigator.language', () => {
    setNavigatorLanguage('en');
    window.localStorage.setItem(STORAGE_KEY, 'zh-CN');
    let setPreference: (p: LocalePreference) => void = () => {};
    renderWith(<LocaleProbeApi onReady={(api) => (setPreference = api.setPreference)} />);

    act(() => setPreference('system'));
    expect(window.localStorage.getItem(STORAGE_KEY)).toBe('system');
    expect(i18n.language).toBe('en');
    expect(document.documentElement.lang).toBe('en');
  });

  it('rejects an invalid preference and keeps the current state', () => {
    setNavigatorLanguage('en');
    let setPreference: (p: LocalePreference) => void = () => {};
    let current: LocalePreference = 'system';
    renderWith(
      <>
        <LocaleProbe onPreference={(p) => (current = p)} />
        <LocaleProbeApi onReady={(api) => (setPreference = api.setPreference)} />
      </>,
    );

    act(() => setPreference('invalid' as LocalePreference));
    expect(current).toBe('system');
    expect(i18n.language).toBe('en');
    expect(document.documentElement.lang).toBe('en');
    expect(window.localStorage.getItem(STORAGE_KEY)).toBe('system');
  });
});

describe('LocaleProvider system follow', () => {
  it('re-resolves when navigator.language changes while preference is system', () => {
    setNavigatorLanguage('en');
    let resolved: 'en' | 'zh-CN' = 'en';
    renderWith(<LocaleResolvedProbe onResolved={(r) => (resolved = r)} />);
    expect(resolved).toBe('en');

    setNavigatorLanguage('zh-CN');
    act(() => window.dispatchEvent(new Event('languagechange')));
    expect(resolved).toBe('zh-CN');
    expect(i18n.language).toBe('zh-CN');
    expect(document.documentElement.lang).toBe('zh-CN');
  });
});

describe('useLocale outside provider', () => {
  it('throws when used without a LocaleProvider', () => {
    function Orphan() {
      useLocale();
      return null;
    }
    // Silence the expected error noise in the test reporter.
    const spy = vi.spyOn(console, 'error').mockImplementation(() => {});
    expect(() => render(<Orphan />)).toThrow(/useLocale must be used within a LocaleProvider/);
    spy.mockRestore();
  });
});

// ── helpers ─────────────────────────────────────────────────────────────────

function LocaleProbeApi({
  onReady,
}: {
  onReady: (api: { setPreference: (p: LocalePreference) => void }) => void;
}) {
  const { setPreference } = useLocale();
  return <>{(onReady({ setPreference }), null)}</>;
}
