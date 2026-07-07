import { http, HttpResponse } from 'msw';
import { describe, expect, it, vi } from 'vitest';
import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useLocation } from 'react-router-dom';

import { SetupWizardPage } from '@/pages/setup-wizard-page';
import { renderInApp } from '@/test/test-providers';
import { useHandlers } from '@/test/msw-server';
import { BrowserClient } from '@/lib/nexus';
import type { DesktopCapabilities } from '@/lib/nexus/desktop-capabilities';

function makeClient() {
  return new BrowserClient();
}

function makeDesktop(overrides: Partial<DesktopCapabilities> = {}): DesktopCapabilities {
  return {
    openWith: () => Promise.resolve(),
    revealInFinder: () => Promise.resolve(),
    getDaemonStatus: () => Promise.resolve({ state: 'running', port: 8420 }),
    onDaemonStatusChanged: (callback) => {
      callback({ state: 'running', port: 8420 });
      return Promise.resolve(() => {});
    },
    startDaemon: () => Promise.resolve(),
    stopDaemon: () => Promise.resolve(),
    resetLocalDatabase: () => Promise.resolve(),
    getSetupCompleted: () => Promise.resolve(false),
    setSetupCompleted: () => Promise.resolve(),
    setAgentProfile: () => Promise.resolve(),
    getWorkspaceRoot: () => Promise.resolve('/tmp/nexus'),
    pickDirectory: () => Promise.resolve(null),
    setWorkspacePath: () => Promise.resolve(),
    ...overrides,
  };
}

function LocationDisplay() {
  const location = useLocation();
  return <div data-testid="location">{location.pathname}</div>;
}

describe('SetupWizardPage', () => {
  it('renders a left-sidebar vertical step indicator and a card content area', () => {
    renderInApp(
      <SetupWizardPage />,
      { client: makeClient(), initialRouterEntries: ['/setup'] },
    );

    // Step indicator is a vertical list inside a fixed left sidebar.
    const nav = screen.getByRole('navigation', { name: 'Setup progress' });
    const list = nav.querySelector('ol');
    expect(list).toHaveClass('flex-col');

    // Active step carries aria-current="step".
    const activeStep = screen.getByRole('listitem', { current: 'step' });
    expect(activeStep).toHaveTextContent('1');
    expect(activeStep).toHaveTextContent('Welcome');

    // Step circle uses the sizing-token utilities.
    const circle = screen.getByText('1').closest('span');
    expect(circle).toHaveClass('h-setup-wizard-step-circle-size');
    expect(circle).toHaveClass('w-setup-wizard-step-circle-size');

    // Content area is the card with the padding-token and max-width-token utilities.
    const main = screen.getByRole('main');
    expect(main).toHaveClass('rounded-card');
    expect(main).toHaveClass('shadow-modal');
    expect(main).toHaveClass('p-setup-wizard-step-wizard-padding');
    expect(main).toHaveClass('max-w-setup-wizard-step-wizard-max-width');
  });

  it('moves through the four steps and finishes', async () => {
    const user = userEvent.setup();

    useHandlers(
      http.get('/v1/daemon/runtime/health', () => HttpResponse.json({ status: 'ok', version: 'test' })),
      http.post('/v1/daemon/agent-host/scan', () => HttpResponse.json({
          agents: [
            {
              name: 'codex',
              registry_agent_id: 'openai/codex',
              launch_command: 'codex',
              installed: true,
              version: '1.0.0',
            },
          ],
        })),
    );

    renderInApp(
      <>
        <LocationDisplay />
        <SetupWizardPage />
      </>,
      { client: makeClient(), initialRouterEntries: ['/setup'] },
    );

    // Welcome step
    expect(screen.getByRole('heading', { name: 'Welcome to Nexus' })).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Continue' }));

    // Daemon step
    await waitFor(() => expect(screen.getByText('Daemon is running.')).toBeInTheDocument());
    await user.click(screen.getByRole('button', { name: 'Continue' }));

    // Agent step
    await waitFor(() => expect(screen.getByText('codex')).toBeInTheDocument());
    await user.click(screen.getByText('codex'));
    await user.click(screen.getAllByRole('button', { name: 'Continue' })[0]);

    // Done step
    await waitFor(() => expect(screen.getByRole('heading', { name: 'You are ready' })).toBeInTheDocument());
    await user.click(screen.getByRole('button', { name: 'Open Nexus' }));

    await waitFor(() => expect(screen.getByTestId('location')).toHaveTextContent('/works'));
  });

  it('persists the selected agent profile on desktop before finishing', async () => {
    const user = userEvent.setup();
    const setAgentProfile = vi.fn(() => Promise.resolve());
    const setSetupCompleted = vi.fn(() => Promise.resolve());

    useHandlers(
      http.get('/v1/daemon/runtime/health', () => HttpResponse.json({ status: 'ok', version: 'test' })),
      http.post('/v1/daemon/agent-host/scan', () => HttpResponse.json({
          agents: [
            {
              name: 'codex',
              registry_agent_id: 'openai/codex',
              launch_command: 'codex',
              installed: true,
              version: '1.0.0',
            },
          ],
        })),
    );

    renderInApp(
      <SetupWizardPage />,
      {
        client: makeClient(),
        desktop: makeDesktop({ setAgentProfile, setSetupCompleted }),
        initialRouterEntries: ['/setup'],
      },
    );

    await user.click(screen.getByRole('button', { name: 'Continue' }));
    await waitFor(() => expect(screen.getByText('Daemon is running.')).toBeInTheDocument());
    await user.click(screen.getByRole('button', { name: 'Continue' }));

    await waitFor(() => expect(screen.getByText('codex')).toBeInTheDocument());
    await user.click(screen.getByText('codex'));
    await user.click(screen.getAllByRole('button', { name: 'Continue' })[0]);

    await waitFor(() => expect(screen.getByRole('heading', { name: 'You are ready' })).toBeInTheDocument());
    await user.click(screen.getByRole('button', { name: 'Open Nexus' }));

    await waitFor(() => expect(setAgentProfile).toHaveBeenCalledWith('codex', 'codex'));
    await waitFor(() => expect(setSetupCompleted).toHaveBeenCalledWith(true));
  });

  it('shows a toast and stays on the wizard when saving the profile fails', async () => {
    const user = userEvent.setup();
    const setAgentProfile = vi.fn(() => Promise.reject(new Error('permission denied')));
    const setSetupCompleted = vi.fn(() => Promise.resolve());

    useHandlers(
      http.get('/v1/daemon/runtime/health', () => HttpResponse.json({ status: 'ok', version: 'test' })),
      http.post('/v1/daemon/agent-host/scan', () => HttpResponse.json({
          agents: [
            {
              name: 'codex',
              registry_agent_id: 'openai/codex',
              launch_command: 'codex',
              installed: true,
              version: '1.0.0',
            },
          ],
        })),
    );

    renderInApp(
      <>
        <LocationDisplay />
        <SetupWizardPage />
      </>,
      {
        client: makeClient(),
        desktop: makeDesktop({ setAgentProfile, setSetupCompleted }),
        initialRouterEntries: ['/setup'],
      },
    );

    await user.click(screen.getByRole('button', { name: 'Continue' }));
    await waitFor(() => expect(screen.getByText('Daemon is running.')).toBeInTheDocument());
    await user.click(screen.getByRole('button', { name: 'Continue' }));

    await waitFor(() => expect(screen.getByText('codex')).toBeInTheDocument());
    await user.click(screen.getByText('codex'));
    await user.click(screen.getAllByRole('button', { name: 'Continue' })[0]);

    await waitFor(() => expect(screen.getByRole('heading', { name: 'You are ready' })).toBeInTheDocument());
    await user.click(screen.getByRole('button', { name: 'Open Nexus' }));

    await waitFor(() => expect(screen.getByText('Could not finish setup')).toBeInTheDocument());
    expect(screen.getByText('permission denied')).toBeInTheDocument();
    expect(setSetupCompleted).not.toHaveBeenCalled();
    expect(screen.getByTestId('location')).toHaveTextContent('/setup');
  });
});
