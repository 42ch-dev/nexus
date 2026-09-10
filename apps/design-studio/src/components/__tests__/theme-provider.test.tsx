import { render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { STORAGE_KEY, ThemeProvider } from '@/components/theme-provider';

function readTheme() {
  return window.localStorage.getItem(STORAGE_KEY);
}

describe('ThemeProvider forced embed mode', () => {
  beforeEach(() => {
    window.localStorage.clear();
    document.documentElement.classList.remove('dark');
    document.documentElement.style.colorScheme = '';
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('does not read or write localStorage when forcedTheme is set', () => {
    const getItem = vi.spyOn(window.localStorage, 'getItem');
    const setItem = vi.spyOn(window.localStorage, 'setItem');

    render(
      <ThemeProvider forcedTheme="dark">
        <div data-testid="child" />
      </ThemeProvider>,
    );

    expect(getItem).not.toHaveBeenCalled();
    expect(setItem).not.toHaveBeenCalled();
    expect(readTheme()).toBeNull();
    expect(document.documentElement.classList.contains('dark')).toBe(true);
  });

  it('does not attach OS theme listeners when forcedTheme is set', () => {
    const addListener = vi.fn();
    vi.spyOn(window, 'matchMedia').mockReturnValue({
      matches: false,
      media: '(prefers-color-scheme: dark)',
      addEventListener: addListener,
      removeEventListener: vi.fn(),
    } as unknown as MediaQueryList);

    render(
      <ThemeProvider forcedTheme="light">
        <div />
      </ThemeProvider>,
    );

    expect(addListener).not.toHaveBeenCalled();
    expect(document.documentElement.classList.contains('dark')).toBe(false);
    expect(document.documentElement.style.colorScheme).toBe('light');
  });

  it('keeps persisted preference behavior in ordinary top-level mode', () => {
    window.localStorage.setItem(STORAGE_KEY, 'dark');

    render(
      <ThemeProvider>
        <span>ordinary</span>
      </ThemeProvider>,
    );

    expect(readTheme()).toBe('dark');
    expect(document.documentElement.classList.contains('dark')).toBe(true);
    expect(screen.getByText('ordinary')).toBeInTheDocument();
  });
});
