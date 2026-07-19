import { http, HttpResponse } from 'msw';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { screen } from '@testing-library/react';

import { Header } from '@/components/layout/header';
import { ThemeProvider } from '@/components/theme-provider';
import { renderInApp } from '@/test/test-providers';
import { useHandlers } from '@/test/msw-server';
import { BrowserClient } from '@/lib/nexus';
import { i18n } from '@/lib/i18n/config';

vi.mock('@/components/brand/nexus-logo', () => ({
  NexusLogo: () => <div data-testid="nexus-logo">Nexus</div>,
}));

function mockMatchMedia(prefersDark = false) {
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

function renderHeader(title = 'Works') {
  return renderInApp(
    <ThemeProvider>
      <Header title={title} />
    </ThemeProvider>,
    { client: new BrowserClient() },
  );
}

beforeEach(async () => {
  window.localStorage.clear();
  await i18n.changeLanguage('en');
  mockMatchMedia(false);
  useHandlers(
    http.get('/v1/daemon/runtime/health', () =>
      HttpResponse.json({ status: 'ok', version: 'test' }),
    ),
  );
});

describe('Header (V1.125 P2)', () => {
  it('exposes Settings beside the theme toggle', () => {
    renderHeader();

    const settings = screen.getByTestId('header-settings-link');
    expect(settings).toHaveAttribute('href', '/settings');
    expect(settings).toHaveAttribute('aria-label', 'Settings');
    expect(screen.getByRole('button', { name: 'Switch to dark theme' })).toBeInTheDocument();
  });
});
