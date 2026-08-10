/**
 * Settings Setup section — Re-run Setup R1 (V1.103 P3).
 */
import { describe, expect, it, vi } from 'vitest';
import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { Navigate, Route, Routes, useLocation } from 'react-router';

import { SettingsAdvancedSection } from '@/pages/settings/settings-advanced-section';
import { SettingsSetupSection } from '@/pages/settings/settings-setup-section';
import { SettingsShellLayout } from '@/pages/settings/settings-shell-layout';
import { DaemonLaunchGate } from '@/components/setup/daemon-launch-gate';
import { renderInApp } from '@/test/test-providers';
import { BrowserClient } from '@/lib/nexus';
import type { DesktopCapabilities } from '@/lib/nexus/desktop-capabilities';
import { useSetupCompleted } from '@/lib/setup-completed-context';

function makeClient() {
  return new BrowserClient();
}

function makeDesktop(
  overrides: Partial<DesktopCapabilities> = {},
): DesktopCapabilities {
  return {
    openWith: () => Promise.resolve(),
    openExternalUrl: () => Promise.resolve(),
    revealInFinder: () => Promise.resolve(),
    getDaemonStatus: () => Promise.resolve({ state: 'running', port: 8420 }),
    onDaemonStatusChanged: (callback) => {
      callback({ state: 'running', port: 8420 });
      return Promise.resolve(() => {});
    },
    startDaemon: () => Promise.resolve(),
    stopDaemon: () => Promise.resolve(),
    resetLocalDatabase: () => Promise.resolve(),
    getSetupCompleted: () => Promise.resolve(true),
    setSetupCompleted: () => Promise.resolve(),
    setAgentProfile: () => Promise.resolve(),
    getAgentProfile: () => Promise.resolve(null),
    getWorkspaceRoot: () => Promise.resolve('/tmp/nexus'),
    pickDirectory: () => Promise.resolve(null),
    setWorkspacePath: () => Promise.resolve(),
    ensureSetupBootstrap: () =>
      Promise.resolve({
        creator_id: 'ctr_local1234567890ab',
        already_bootstrapped: true,
      }),
    switchActiveCreator: () => Promise.resolve('/tmp/nexus'),
    restartDaemon: () => Promise.resolve(),
    toggleMaximizeWindow: () => Promise.resolve(),
    ...overrides,
  };
}

function LocationProbe() {
  const location = useLocation();
  return <div data-testid="location">{location.pathname}</div>;
}

function CompletedProbe() {
  const { completed } = useSetupCompleted();
  return <span data-testid="setup-completed">{completed ? 'true' : 'false'}</span>;
}

const settingsRouteTree = (
  <Route path="settings" element={<SettingsShellLayout />}>
    <Route index element={<Navigate to="agent" replace />} />
    <Route path="advanced" element={<SettingsAdvancedSection />} />
    <Route path="setup" element={<SettingsSetupSection />} />
  </Route>
);

describe('SettingsSetupSection', () => {
  it('renders locked helper and browser-only disabled CTA without desktop', () => {
    renderInApp(
      <Routes>{settingsRouteTree}</Routes>,
      {
        client: makeClient(),
        initialRouterEntries: ['/settings/setup'],
        setupCompleted: true,
      },
    );

    const section = screen.getByTestId('settings-setup-section');
    expect(section).toHaveAttribute('data-desktop', 'false');
    expect(
      screen.getByText(
        /Return to the first-run wizard to walk through setup steps again\. Your workspace and agent choices are kept/i,
      ),
    ).toBeInTheDocument();
    expect(screen.getByTestId('settings-setup-browser-only')).toBeInTheDocument();
    expect(
      screen.getByText(/Re-run setup is available on the desktop app only/i),
    ).toBeInTheDocument();
    const cta = screen.getByTestId('settings-rerun-setup');
    expect(cta).toBeDisabled();
    expect(cta).toHaveAttribute(
      'title',
      'Open the Nexus desktop app to re-run setup.',
    );
  });

  it('confirm clears setup_completed, syncs context, and navigates to /setup', async () => {
    const user = userEvent.setup();
    const setSetupCompleted = vi.fn(() => Promise.resolve());
    const resetLocalDatabase = vi.fn(() => Promise.resolve());
    const setWorkspacePath = vi.fn(() => Promise.resolve());
    const setAgentProfile = vi.fn(() => Promise.resolve());

    renderInApp(
      <DaemonLaunchGate>
        <>
          <CompletedProbe />
          <Routes>
            {settingsRouteTree}
            <Route
              path="setup"
              element={
                <>
                  <div data-testid="setup-wizard">Wizard</div>
                  <LocationProbe />
                </>
              }
            />
          </Routes>
        </>
      </DaemonLaunchGate>,
      {
        client: makeClient(),
        desktop: makeDesktop({
          setSetupCompleted,
          resetLocalDatabase,
          setWorkspacePath,
          setAgentProfile,
        }),
        initialRouterEntries: ['/settings/setup'],
        setupCompleted: true,
      },
    );

    await waitFor(() =>
      expect(screen.getByTestId('settings-setup-section')).toBeInTheDocument(),
    );

    expect(screen.getByTestId('setup-completed')).toHaveTextContent('true');

    await user.click(screen.getByTestId('settings-rerun-setup'));
    const dialog = screen.getByRole('dialog');
    expect(within(dialog).getByText('Re-run Setup?')).toBeInTheDocument();
    expect(
      within(dialog).getByText(
        /This restarts the setup wizard from the beginning\. Your workspace path and agent profile are not deleted/i,
      ),
    ).toBeInTheDocument();

    await user.click(
      within(dialog).getByTestId('settings-rerun-setup-confirm-action'),
    );

    await waitFor(() => expect(setSetupCompleted).toHaveBeenCalledWith(false));
    expect(setSetupCompleted).toHaveBeenCalledTimes(1);
    expect(resetLocalDatabase).not.toHaveBeenCalled();
    expect(setWorkspacePath).not.toHaveBeenCalled();
    expect(setAgentProfile).not.toHaveBeenCalled();

    await waitFor(() =>
      expect(screen.getByTestId('setup-wizard')).toBeInTheDocument(),
    );
    expect(screen.getByTestId('location')).toHaveTextContent('/setup');
    expect(screen.getByTestId('setup-completed')).toHaveTextContent('false');
    expect(screen.queryByTestId('settings-setup-section')).not.toBeInTheDocument();
  });

  it('cancel leaves marker unchanged and stays on Setup', async () => {
    const user = userEvent.setup();
    const setSetupCompleted = vi.fn(() => Promise.resolve());

    renderInApp(
      <>
        <CompletedProbe />
        <Routes>{settingsRouteTree}</Routes>
      </>,
      {
        client: makeClient(),
        desktop: makeDesktop({ setSetupCompleted }),
        initialRouterEntries: ['/settings/setup'],
        setupCompleted: true,
      },
    );

    expect(screen.getByTestId('setup-completed')).toHaveTextContent('true');

    await user.click(screen.getByTestId('settings-rerun-setup'));
    const dialog = screen.getByRole('dialog');
    await user.click(within(dialog).getByTestId('settings-rerun-setup-cancel'));

    await waitFor(() =>
      expect(screen.queryByRole('dialog')).not.toBeInTheDocument(),
    );
    expect(setSetupCompleted).not.toHaveBeenCalled();
    expect(screen.getByTestId('setup-completed')).toHaveTextContent('true');
    expect(screen.getByTestId('settings-setup-section')).toBeInTheDocument();
  });

  it('shows a toast and stays on Setup when clear IPC fails', async () => {
    const user = userEvent.setup();
    const setSetupCompleted = vi.fn(() => Promise.reject(new Error('ipc unavailable')));
    const resetLocalDatabase = vi.fn(() => Promise.resolve());

    renderInApp(
      <>
        <CompletedProbe />
        <Routes>
          {settingsRouteTree}
          <Route path="setup" element={<div data-testid="setup-wizard">Wizard</div>} />
        </Routes>
      </>,
      {
        client: makeClient(),
        desktop: makeDesktop({ setSetupCompleted, resetLocalDatabase }),
        initialRouterEntries: ['/settings/setup'],
        setupCompleted: true,
      },
    );

    await user.click(screen.getByTestId('settings-rerun-setup'));
    const dialog = screen.getByRole('dialog');
    await user.click(
      within(dialog).getByTestId('settings-rerun-setup-confirm-action'),
    );

    await waitFor(() =>
      expect(screen.getByText('Could not re-run setup')).toBeInTheDocument(),
    );
    expect(screen.getByText('ipc unavailable')).toBeInTheDocument();
    expect(setSetupCompleted).toHaveBeenCalledWith(false);
    expect(resetLocalDatabase).not.toHaveBeenCalled();
    expect(screen.getByTestId('setup-completed')).toHaveTextContent('true');
    expect(screen.getByTestId('settings-setup-section')).toBeInTheDocument();
    expect(screen.queryByTestId('setup-wizard')).not.toBeInTheDocument();
    // Dialog stays open so the author can retry or cancel.
    expect(screen.getByRole('dialog')).toBeInTheDocument();
  });

  it('renders the Advanced re-run wizard CTA with the destructive/danger variant (AC-P1-4)', async () => {
    const user = userEvent.setup();

    renderInApp(
      <>
        <CompletedProbe />
        <Routes>{settingsRouteTree}</Routes>
      </>,
      {
        client: makeClient(),
        desktop: makeDesktop(),
        initialRouterEntries: ['/settings/setup'],
        setupCompleted: true,
      },
    );

    // The Advanced "Re-run" trigger CTA is the destructive variant: a
    // red-800 fill with brand-deep-blue text in dark (4.90:1, ≥ WCAG AA).
    const trigger = screen.getByTestId('settings-rerun-setup');
    expect(trigger).toHaveClass('bg-red-800');

    // Open the confirm dialog and assert the confirm action is also danger.
    await user.click(trigger);
    const dialog = screen.getByRole('dialog');
    const confirmAction = within(dialog).getByTestId(
      'settings-rerun-setup-confirm-action',
    );
    expect(confirmAction).toHaveClass('bg-red-800');
  });
});
