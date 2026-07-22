import { describe, expect, it, vi, beforeEach } from 'vitest';
import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import { ChronosTitlebar } from '@/components/layout/chronos-titlebar';
import { SettingsModalProvider } from '@/components/layout/settings-modal-context';
import { SettingsModalHost } from '@/components/layout/settings-modal-host';
import { ThemeProvider } from '@/components/theme-provider';
import { renderInApp } from '@/test/test-providers';
import { BrowserClient } from '@/lib/nexus';
import { i18n } from '@/lib/i18n/config';

vi.mock('@/components/brand/nexus-ink-logo', () => ({
  NexusInkLogo: () => <div data-testid="nexus-ink-logo">Nexus</div>,
}));

vi.mock('@/components/daemon-health-indicator', () => ({
  DaemonHealthIndicator: () => <div data-testid="daemon-health">ok</div>,
}));

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

function renderTitlebar() {
  return renderInApp(
    <ThemeProvider>
      <SettingsModalProvider>
        <ChronosTitlebar title="Works" />
        <SettingsModalHost>
          <p data-testid="settings-modal-placeholder">Settings body</p>
        </SettingsModalHost>
      </SettingsModalProvider>
    </ThemeProvider>,
    { client: new BrowserClient() },
  );
}

describe('ChronosTitlebar', () => {
  beforeEach(async () => {
    window.localStorage.clear();
    mockMatchMedia(false);
    await i18n.changeLanguage('en');
  });

  it('opens SettingsModalHost when the gear is clicked and restores focus on close', async () => {
    const user = userEvent.setup();
    renderTitlebar();

    const gear = screen.getByTestId('chronos-titlebar-settings-gear');
    await user.click(gear);

    await waitFor(() => {
      expect(screen.getByTestId('settings-modal-body')).toBeInTheDocument();
    });

    await user.keyboard('{Escape}');

    await waitFor(() => {
      expect(screen.queryByTestId('settings-modal-body')).not.toBeInTheDocument();
    });
    expect(gear).toHaveFocus();
  });
});
