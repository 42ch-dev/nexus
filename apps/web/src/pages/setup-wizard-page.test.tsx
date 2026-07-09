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
    ensureSetupBootstrap: () =>
      Promise.resolve({ creator_id: 'ctr_local1234567890ab', already_bootstrapped: false }),
    ...overrides,
  };
}

function LocationDisplay() {
  const location = useLocation();
  return <div data-testid="location">{location.pathname}</div>;
}

describe('SetupWizardPage', () => {
  it('renders a centered integrated card with step indicator and content area', () => {
    renderInApp(
      <SetupWizardPage />,
      { client: makeClient(), initialRouterEntries: ['/setup'] },
    );

    // Step indicator is a vertical list inside the left panel of the integrated card.
    const innerNav = screen.getByRole('navigation', { name: 'Setup progress' });
    const list = innerNav.querySelector('ol');
    expect(list).toHaveClass('flex-col');

    // Active step carries aria-current="step".
    const activeStep = screen.getByRole('listitem', { current: 'step' });
    expect(activeStep).toHaveTextContent('1');
    expect(activeStep).toHaveTextContent('Welcome');

    // Step circle uses the sizing-token utilities.
    const circle = screen.getByText('1').closest('span');
    expect(circle).toHaveClass('h-setup-wizard-step-circle-size');
    expect(circle).toHaveClass('w-setup-wizard-step-circle-size');

    // The integrated card wraps both the step indicator panel and the content panel.
    const main = screen.getByRole('main');
    const card = main.parentElement;
    expect(card).toContainElement(innerNav);
    expect(card).toHaveClass('max-w-setup-wizard-step-wizard-max-width');
    expect(card).toHaveClass('overflow-hidden');
    expect(card).toHaveClass('rounded-popover');
    expect(card).toHaveClass('shadow-modal');
    expect(card).toHaveClass('bg-setup-wizard-surface-card-bg');
    expect(card).toHaveClass('border-setup-wizard-surface-card-border');

    // The card is centered inside the outer shell.
    const outer = card?.parentElement;
    expect(outer).toHaveClass('items-center');
    expect(outer).toHaveClass('justify-center');
    expect(outer).toHaveClass('min-h-screen');

    // The content panel is a flex column so T6's mt-auto CTA pins to the bottom.
    expect(main).toHaveClass('flex-col');
    expect(main).toHaveClass('flex-1');
    expect(main).toHaveClass('min-w-0');
  });

  it('aligns step indicator circles and labels on the same baseline and renders connectors between steps', () => {
    renderInApp(
      <SetupWizardPage />,
      { client: makeClient(), initialRouterEntries: ['/setup'] },
    );

    const steps = screen.getAllByRole('listitem');
    expect(steps).toHaveLength(4);

    steps.forEach((li, index) => {
      // Row uses a fixed height and centers its direct children (circle + label).
      expect(li).toHaveClass('items-center');
      expect(li).toHaveClass('h-setup-wizard-step-row-height');

      // Circle and label are direct siblings under the same centered row.
      const spans = Array.from(li.children).filter((child) => child.tagName === 'SPAN');
      expect(spans).toHaveLength(2);
      expect(spans[0]).toHaveTextContent(String(index + 1));
      expect(spans[1]).toHaveTextContent(['Welcome', 'Daemon', 'Agent', 'Done'][index]);
    });

    // Each non-final step has an absolutely-positioned connector behind the circle.
    for (let i = 0; i < 3; i++) {
      const connector = steps[i].querySelector('div[aria-hidden="true"]');
      expect(connector).toBeInTheDocument();
      expect(connector).toHaveClass('w-px');
      expect(connector).toHaveClass('bg-setup-wizard-step-connector');
    }

    // The last step has no connector.
    expect(steps[3].querySelector('div[aria-hidden="true"]')).not.toBeInTheDocument();
  });

  it('maps Steps complete/active/pending statuses as the wizard advances', async () => {
    const user = userEvent.setup();

    useHandlers(
      http.get('/v1/daemon/runtime/health', () => HttpResponse.json({ status: 'ok', version: 'test' })),
      http.post('/v1/daemon/agent-host/scan', () =>
        HttpResponse.json({
          agents: [
            {
              name: 'codex',
              registry_agent_id: 'openai/codex',
              launch_command: 'codex',
              installed: true,
              version: '1.0.0',
            },
          ],
        }),
      ),
    );

    renderInApp(<SetupWizardPage />, {
      client: makeClient(),
      initialRouterEntries: ['/setup'],
    });

    const progress = screen.getByRole('navigation', { name: 'Setup progress' });
    expect(progress.querySelector('[data-step-id="welcome"]')).toHaveAttribute(
      'data-step-status',
      'active',
    );
    expect(progress.querySelector('[data-step-id="daemon"]')).toHaveAttribute(
      'data-step-status',
      'pending',
    );

    await user.click(screen.getByRole('button', { name: 'Continue' }));
    await waitFor(() => expect(screen.getByText('Daemon is running.')).toBeInTheDocument());

    expect(progress.querySelector('[data-step-id="welcome"]')).toHaveAttribute(
      'data-step-status',
      'complete',
    );
    expect(progress.querySelector('[data-step-id="daemon"]')).toHaveAttribute(
      'data-step-status',
      'active',
    );
    expect(progress.querySelector('[data-step-id="agent"]')).toHaveAttribute(
      'data-step-status',
      'pending',
    );

    const daemonCta = screen.getByTestId('wizard-cta-row');
    expect(daemonCta).toHaveAttribute('data-layout', 'horizontal-adjacent');
    expect(daemonCta.querySelectorAll('button')[0]).toHaveTextContent('Back');

    await user.click(screen.getByRole('button', { name: 'Continue' }));
    await waitFor(() => expect(screen.getByText('codex')).toBeInTheDocument());

    expect(progress.querySelector('[data-step-id="daemon"]')).toHaveAttribute(
      'data-step-status',
      'complete',
    );
    expect(progress.querySelector('[data-step-id="agent"]')).toHaveAttribute(
      'data-step-status',
      'active',
    );
    expect(progress.querySelector('[data-step-id="done"]')).toHaveAttribute(
      'data-step-status',
      'pending',
    );

    await user.click(screen.getByText('codex'));
    await user.click(screen.getAllByRole('button', { name: 'Continue' })[0]);
    await waitFor(() =>
      expect(screen.getByRole('heading', { name: 'You are ready' })).toBeInTheDocument(),
    );

    expect(progress.querySelector('[data-step-id="agent"]')).toHaveAttribute(
      'data-step-status',
      'complete',
    );
    expect(progress.querySelector('[data-step-id="done"]')).toHaveAttribute(
      'data-step-status',
      'active',
    );
    expect(screen.getByTestId('wizard-cta-row').textContent).not.toMatch(/Back/);
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
