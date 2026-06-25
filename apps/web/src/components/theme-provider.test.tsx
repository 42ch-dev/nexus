/**
 * ThemeProvider coverage (R-V164-QC1-S1-P1 T2).
 *
 * DESIGN.md uses identical token names in both themes; only the CSS-variable
 * values swap under the `.dark` class on `<html>` (Tailwind `class` strategy).
 * These tests pin the architectural surface P2 inherits: provider mount,
 * default-theme detection (localStorage → OS preference → light), the `.dark`
 * class + `data-theme` attribute + localStorage persistence on `setTheme`, and
 * the `toggleTheme` round-trip. Token *values* themselves live in index.css
 * (CSS-variable projection of DESIGN.md) and are exercised by the build, not
 * the unit layer.
 */
import { act, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { ReactNode } from 'react';

import { ThemeProvider, useTheme } from '@/components/theme-provider';

const STORAGE_KEY = 'nexus-web-theme';

/** jsdom does not implement matchMedia; the provider consults it for the OS
 *  default, so each test installs a deterministic mock. */
function mockMatchMedia(prefersDark: boolean) {
  const media = {
    matches: prefersDark,
    media: '(prefers-color-scheme: dark)',
    onchange: null,
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
    addListener: vi.fn(),
    removeListener: vi.fn(),
    dispatchEvent: vi.fn(),
  };
  vi.spyOn(window, 'matchMedia').mockReturnValue(media as unknown as MediaQueryList);
}

/** Surface the context value from inside the provider for assertions. */
function ThemeProbe({ onTheme }: { onTheme: (t: string) => void }) {
  const { theme } = useTheme();
  return <>{(onTheme(theme), null)}</>;
}

function renderWith(ui: ReactNode) {
  return render(<ThemeProvider>{ui}</ThemeProvider>);
}

beforeEach(() => {
  window.localStorage.clear();
  document.documentElement.classList.remove('dark');
  document.documentElement.removeAttribute('data-theme');
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe('ThemeProvider mount', () => {
  it('renders its children', () => {
    mockMatchMedia(false);
    renderWith(<div>child-content</div>);
    expect(screen.getByText('child-content')).toBeInTheDocument();
  });
});

describe('ThemeProvider default-theme detection', () => {
  it('prefers a stored light theme over the OS preference', () => {
    mockMatchMedia(true); // OS says dark…
    window.localStorage.setItem(STORAGE_KEY, 'light'); // …but stored says light.
    let current = 'unset';
    renderWith(<ThemeProbe onTheme={(t) => (current = t)} />);
    expect(current).toBe('light');
    expect(document.documentElement.classList.contains('dark')).toBe(false);
  });

  it('falls back to the OS preference when nothing is stored', () => {
    mockMatchMedia(true);
    let current = 'unset';
    renderWith(<ThemeProbe onTheme={(t) => (current = t)} />);
    expect(current).toBe('dark');
    expect(document.documentElement.classList.contains('dark')).toBe(true);
  });

  it('falls back to light when the OS prefers light and nothing is stored', () => {
    mockMatchMedia(false);
    let current = 'unset';
    renderWith(<ThemeProbe onTheme={(t) => (current = t)} />);
    expect(current).toBe('light');
  });

  it('ignores an unrecognized stored value', () => {
    mockMatchMedia(false);
    window.localStorage.setItem(STORAGE_KEY, 'neon');
    let current = 'unset';
    renderWith(<ThemeProbe onTheme={(t) => (current = t)} />);
    expect(current).toBe('light');
  });
});

describe('ThemeProvider token application (`.dark` swap)', () => {
  it('applies the `.dark` class + data-theme and persists on setTheme(dark)', () => {
    mockMatchMedia(false);
    let setTheme: (t: 'light' | 'dark') => void = () => {};
    renderWith(
      <ThemeProbeOnAction onReady={(api) => (setTheme = api.setTheme)} />,
    );

    act(() => setTheme('dark'));
    expect(document.documentElement.classList.contains('dark')).toBe(true);
    expect(document.documentElement.getAttribute('data-theme')).toBe('dark');
    expect(window.localStorage.getItem(STORAGE_KEY)).toBe('dark');
  });

  it('removes the `.dark` class when switching back to light', () => {
    mockMatchMedia(true); // start dark
    let setTheme: (t: 'light' | 'dark') => void = () => {};
    renderWith(
      <ThemeProbeOnAction onReady={(api) => (setTheme = api.setTheme)} />,
    );
    expect(document.documentElement.classList.contains('dark')).toBe(true);

    act(() => setTheme('light'));
    expect(document.documentElement.classList.contains('dark')).toBe(false);
    expect(document.documentElement.getAttribute('data-theme')).toBe('light');
    expect(window.localStorage.getItem(STORAGE_KEY)).toBe('light');
  });
});

describe('ThemeProvider toggleTheme', () => {
  it('flips light ↔ dark', () => {
    mockMatchMedia(false);
    let toggle: () => void = () => {};
    let current = 'unset';
    renderWith(
      <ThemeProbeApi
        onReady={(api) => (toggle = api.toggleTheme)}
        onTheme={(t) => (current = t)}
      />,
    );
    expect(current).toBe('light');

    act(() => toggle());
    expect(current).toBe('dark');
    expect(document.documentElement.classList.contains('dark')).toBe(true);

    act(() => toggle());
    expect(current).toBe('light');
    expect(document.documentElement.classList.contains('dark')).toBe(false);
  });
});

describe('useTheme outside provider', () => {
  it('throws when used without a ThemeProvider', () => {
    function Orphan() {
      useTheme();
      return null;
    }
    // Silence the expected error noise in the test reporter.
    const spy = vi.spyOn(console, 'error').mockImplementation(() => {});
    expect(() => render(<Orphan />)).toThrow(/useTheme must be used within a ThemeProvider/);
    spy.mockRestore();
  });
});

// ── helpers ─────────────────────────────────────────────────────────────────

function ThemeProbeOnAction({
  onReady,
}: {
  onReady: (api: { setTheme: (t: 'light' | 'dark') => void; toggleTheme: () => void }) => void;
}) {
  const { setTheme, toggleTheme } = useTheme();
  return <>{(onReady({ setTheme, toggleTheme }), null)}</>;
}

function ThemeProbeApi({
  onReady,
  onTheme,
}: {
  onReady: (api: { toggleTheme: () => void }) => void;
  onTheme: (t: 'light' | 'dark') => void;
}) {
  const { theme, toggleTheme } = useTheme();
  return (
    <>
      {(onReady({ toggleTheme }), onTheme(theme), null)}
    </>
  );
}
