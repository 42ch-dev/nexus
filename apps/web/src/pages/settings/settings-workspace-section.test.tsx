/**
 * Settings Profiles section — mount, desktop persist, browser-only branch.
 */
import { describe, expect, it, vi } from 'vitest';
import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import { SettingsWorkspaceSection } from '@/pages/settings/settings-workspace-section';
import { renderInApp } from '@/test/test-providers';
import type { DesktopCapabilities } from '@/lib/nexus/desktop-capabilities';

const INITIAL_PATH = '/Users/creator/Documents/Nexus';
const PICKED_PATH = '/Volumes/Studio/Nexus';

function makeDesktopCapabilities(): DesktopCapabilities {
  return {
    getWorkspaceRoot: vi.fn(() => Promise.resolve(INITIAL_PATH)),
    pickDirectory: vi.fn(() => Promise.resolve(PICKED_PATH)),
    setWorkspacePath: vi.fn(() => Promise.resolve()),
    openWith: vi.fn(() => Promise.resolve()),
    openExternalUrl: vi.fn(() => Promise.resolve()),
    revealInFinder: vi.fn(() => Promise.resolve()),
    getDaemonStatus: vi.fn(() =>
      Promise.resolve({ state: 'running' as const, port: 8420 }),
    ),
    onDaemonStatusChanged: vi.fn(() => Promise.resolve(() => {})),
    startDaemon: vi.fn(() => Promise.resolve()),
    stopDaemon: vi.fn(() => Promise.resolve()),
    resetLocalDatabase: vi.fn(() => Promise.resolve()),
    getSetupCompleted: vi.fn(() => Promise.resolve(true)),
    setSetupCompleted: vi.fn(() => Promise.resolve()),
    setAgentProfile: vi.fn(() => Promise.resolve()),
    getAgentProfile: vi.fn(() => Promise.resolve(null)),
    ensureSetupBootstrap: vi.fn(() =>
      Promise.resolve({ creator_id: 'creator-a', already_bootstrapped: true }),
    ),
    switchActiveCreator: vi.fn(() => Promise.resolve('/tmp/nexus')),
  };
}

describe('SettingsWorkspaceSection', () => {
  it('renders the section body and loads current path (desktop)', async () => {
    const desktop = makeDesktopCapabilities();

    renderInApp(<SettingsWorkspaceSection />, { desktop });

    expect(screen.getByTestId('settings-workspace-section')).toBeInTheDocument();
    expect(screen.getByTestId('settings-workspace-card')).toBeInTheDocument();
    expect(
      screen.getByRole('heading', { name: 'Profiles', level: 3 }),
    ).toBeInTheDocument();
    expect(
      screen.getByText(
        /View or change the workspace folder for the active Profile/i,
      ),
    ).toBeInTheDocument();

    await waitFor(() =>
      expect(screen.getByDisplayValue(INITIAL_PATH)).toHaveValue(
        INITIAL_PATH,
      ),
    );
    expect(desktop.getWorkspaceRoot).toHaveBeenCalledTimes(1);
  });

  it('falls back to the default workspace path when loading fails (desktop)', async () => {
    const desktop = makeDesktopCapabilities();
    vi.mocked(desktop.getWorkspaceRoot).mockRejectedValue(new Error('permission denied'));

    renderInApp(<SettingsWorkspaceSection />, { desktop });

    await waitFor(() =>
      expect(screen.getByDisplayValue('~/Documents/nexus/default')).toHaveValue(
        '~/Documents/nexus/default',
      ),
    );
    expect(desktop.getWorkspaceRoot).toHaveBeenCalledTimes(1);
    expect(screen.getByRole('button', { name: 'Change…' })).toBeEnabled();
  });

  it('persists a picked directory and shows inline honesty copy', async () => {
    const user = userEvent.setup();
    const desktop = makeDesktopCapabilities();

    renderInApp(<SettingsWorkspaceSection />, { desktop });

    await waitFor(() =>
      expect(screen.getByDisplayValue(INITIAL_PATH)).toHaveValue(
        INITIAL_PATH,
      ),
    );

    await user.click(screen.getByRole('button', { name: 'Change…' }));

    await waitFor(() =>
      expect(desktop.setWorkspacePath).toHaveBeenCalledWith(PICKED_PATH),
    );
    expect(desktop.pickDirectory).toHaveBeenCalledWith(INITIAL_PATH);

    await waitFor(() =>
      expect(screen.getByTestId('settings-workspace-saved-honesty')).toHaveTextContent(
        /Profile workspace path saved/i,
      ),
    );
    expect(screen.getByDisplayValue(PICKED_PATH)).toHaveValue(PICKED_PATH);
  });

  it('returns to idle state when picker is cancelled (pickDirectory returns null)', async () => {
    const user = userEvent.setup();
    const desktop = makeDesktopCapabilities();
    vi.mocked(desktop.pickDirectory).mockResolvedValue(null);

    renderInApp(<SettingsWorkspaceSection />, { desktop });

    await waitFor(() =>
      expect(screen.getByDisplayValue(INITIAL_PATH)).toHaveValue(
        INITIAL_PATH,
      ),
    );

    await user.click(screen.getByRole('button', { name: 'Change…' }));

    await waitFor(() =>
      expect(screen.getByRole('button', { name: 'Change…' })).not.toBeDisabled(),
    );
    expect(desktop.pickDirectory).toHaveBeenCalledWith(INITIAL_PATH);
    expect(desktop.setWorkspacePath).not.toHaveBeenCalled();
    expect(screen.getByDisplayValue(INITIAL_PATH)).toHaveValue(INITIAL_PATH);
    expect(
      screen.queryByTestId('settings-workspace-saved-honesty'),
    ).not.toBeInTheDocument();
  });

  it('shows disabled change action and browser-only helper when desktop is null', async () => {
    renderInApp(<SettingsWorkspaceSection />, { desktop: null });

    const section = screen.getByTestId('settings-workspace-section');
    expect(section).toHaveAttribute('data-desktop', 'false');

    expect(
      screen.getByText(
        'Profile workspace path changes are available on the desktop app only.',
      ),
    ).toBeInTheDocument();

    const button = screen.getByRole('button', { name: 'Change…' });
    expect(button).toBeDisabled();
  });
});
