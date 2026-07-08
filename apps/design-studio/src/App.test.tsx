/**
 * Design Studio smoke tests — T7 task-7-brief.md.
 *
 * Coverage:
 *   1. App renders the landing page (HomePage) with section links.
 *   2. Theme toggle switches light ↔ dark and applies the `.dark` class.
 *   3. Each gallery section route renders its heading.
 *
 * These tests catch route/render regressions without snapshot-exhausting every
 * swatch or component variant. Follow apps/web conventions: vitest + jsdom +
 * @testing-library/react.
 */
import { act, render, screen } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { App } from '@/App';
import { ThemeProvider } from '@/components/theme-provider';

/* ---- helpers ------------------------------------------------------------ */

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

function renderStudio(initialRoute = '/') {
  return render(
    <ThemeProvider>
      <MemoryRouter initialEntries={[initialRoute]}>
        <App />
      </MemoryRouter>
    </ThemeProvider>,
  );
}

beforeEach(() => {
  window.localStorage.clear();
  document.documentElement.classList.remove('dark');
  document.documentElement.removeAttribute('data-theme');
});

afterEach(() => {
  vi.restoreAllMocks();
});

/* ---- landing page ------------------------------------------------------- */

describe('App landing page', () => {
  it('renders the studio heading', () => {
    mockMatchMedia(false);
    renderStudio();
    expect(
      screen.getByRole('heading', { name: 'Nexus Design Studio' }),
    ).toBeInTheDocument();
  });

  it('renders the read-only SSOT hint in footer', () => {
    mockMatchMedia(false);
    renderStudio();
    expect(screen.getByText(/Read-only/)).toBeInTheDocument();
  });

  it('renders navigation links for all five gallery sections', () => {
    mockMatchMedia(false);
    renderStudio();
    // Nav links appear in the header AND as home-page cards — verify at
    // least one of each label exists.
    const expectedLabels = ['Tokens', 'Brand', 'Components', 'Voice', 'Surfaces'];
    for (const label of expectedLabels) {
      expect(screen.getAllByText(label).length).toBeGreaterThanOrEqual(1);
    }
  });
});

/* ---- theme toggle ------------------------------------------------------- */

describe('Theme toggle', () => {
  it('starts in light mode when OS prefers light', () => {
    mockMatchMedia(false);
    renderStudio();
    expect(document.documentElement.classList.contains('dark')).toBe(false);
  });

  it('starts in dark mode when OS prefers dark', () => {
    mockMatchMedia(true);
    renderStudio();
    expect(document.documentElement.classList.contains('dark')).toBe(true);
  });

  it('toggles dark class on click', () => {
    mockMatchMedia(false);
    renderStudio();

    const toggle = screen.getByLabelText(/Switch to dark theme/);
    expect(document.documentElement.classList.contains('dark')).toBe(false);

    act(() => toggle.click());
    expect(document.documentElement.classList.contains('dark')).toBe(true);

    const toggleDark = screen.getByLabelText(/Switch to light theme/);
    act(() => toggleDark.click());
    expect(document.documentElement.classList.contains('dark')).toBe(false);
  });
});

/* ---- gallery section rendering ------------------------------------------ */

const GALLERY_SECTIONS = [
  { route: '/tokens', heading: 'Tokens' },
  { route: '/brand', heading: 'Brand' },
  { route: '/components', heading: 'Components' },
  { route: '/voice', heading: 'Voice & Content' },
  { route: '/surfaces', heading: 'Surfaces' },
] as const;

describe('Gallery section rendering', () => {
  it.each(GALLERY_SECTIONS)(
    'renders $heading at $route',
    ({ route, heading }) => {
      mockMatchMedia(false);
      renderStudio(route);
      // Heading text may appear both in the nav link and the page <h2> —
      // verify at least one instance exists.
      expect(screen.getAllByText(heading).length).toBeGreaterThanOrEqual(1);
    },
  );
});
