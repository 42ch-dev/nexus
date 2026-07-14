import { http, HttpResponse } from 'msw';
import { describe, expect, it, vi } from 'vitest';
import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { Route, Routes, useLocation } from 'react-router-dom';

import { SetupWizardPage } from '@/pages/setup-wizard-page';
import { SetupGate } from '@/components/setup/setup-gate';
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
    getAgentProfile: () => Promise.resolve(null),
    getWorkspaceRoot: () => Promise.resolve('/tmp/nexus'),
    pickDirectory: () => Promise.resolve(null),
    setWorkspacePath: () => Promise.resolve(),
    ensureSetupBootstrap: () =>
      Promise.resolve({ creator_id: 'ctr_local1234567890ab', already_bootstrapped: false }),
    switchActiveCreator: () => Promise.resolve('/tmp/nexus'),
    ...overrides,
  };
}

function LocationDisplay() {
  const location = useLocation();
  return <div data-testid="location">{location.pathname}</div>;
}

function useWizardScanHandlers() {
  useHandlers(
    http.get('/v1/daemon/runtime/health', () => HttpResponse.json({ status: 'ok', version: 'test' })),
    http.post('/v1/daemon/agent-host/scan', () =>
      HttpResponse.json({
        agents: [
          {
            name: 'codex',
            registry_agent_id: 'codex-acp',
            launch_command: 'codex',
            installed: true,
            version: '1.0.0',
          },
        ],
      }),
    ),
    http.patch('/v1/daemon/creators/:id', () =>
      HttpResponse.json({
        creator_id: 'ctr_local1234567890ab',
        display_name: 'Test Profile',
        has_api_key: false,
        has_cached_token: false,
        is_active: true,
      }),
    ),
  );
}

async function advanceAgentToWorkspace(user: ReturnType<typeof userEvent.setup>) {
  await waitFor(() => expect(screen.getByText('codex')).toBeInTheDocument());
  await user.click(screen.getByText('codex'));
  await user.click(screen.getAllByRole('button', { name: 'Continue' })[0]);
  await waitFor(() =>
    expect(screen.getByRole('heading', { name: 'Name your Profile' })).toBeInTheDocument(),
  );
}

async function fillProfileName(
  user: ReturnType<typeof userEvent.setup>,
  name: string = 'Test Profile',
) {
  const input = await waitFor(() => screen.getByTestId('wizard-profile-name'));
  await user.clear(input);
  await user.type(input, name);
}

describe('SetupWizardPage', () => {
  it('renders a portrait card with top Steps and no left rail', () => {
    useWizardScanHandlers();
    renderInApp(
      <SetupWizardPage />,
      { client: makeClient(), initialRouterEntries: ['/setup'] },
    );

    const card = screen.getByTestId('setup-wizard-card');
    expect(card).toHaveAttribute('data-shell', 'portrait');
    expect(card).toHaveClass('max-w-setup-wizard-step-wizard-max-width');
    expect(card).toHaveClass('h-setup-wizard-wizard-max-height');
    expect(card).toHaveClass('max-h-[85vh]');
    expect(card).toHaveClass('overflow-hidden');
    expect(card).toHaveClass('rounded-popover');
    expect(card).toHaveClass('shadow-modal');
    expect(card).toHaveClass('bg-setup-wizard-surface-card-bg');
    expect(card).toHaveClass('border-setup-wizard-surface-card-border');
    expect(card.querySelector('.w-setup-wizard-surface-step-panel-width')).toBeNull();
    expect(card.querySelector('aside')).toBeNull();

    const topSteps = screen.getByTestId('top-step-indicator');
    expect(card).toContainElement(topSteps);
    const list = topSteps.querySelector('ol');
    expect(list).toHaveClass('flex');
    expect(list).not.toHaveClass('flex-col');

    const activeStep = screen.getByRole('listitem', { current: 'step' });
    expect(activeStep).toHaveAttribute('data-step-id', 'agent');
    expect(activeStep).toHaveTextContent('1');
    expect(activeStep).toHaveTextContent('Agent');

    const circle = screen.getByText('1').closest('span');
    expect(circle).toHaveClass('h-setup-wizard-step-circle-size');
    expect(circle).toHaveClass('w-setup-wizard-step-circle-size');

    const main = screen.getByRole('main');
    expect(card).toContainElement(main);
    expect(main).toHaveClass('flex-col');
    expect(main).toHaveClass('flex-1');
    expect(main).toHaveClass('min-h-0');
    expect(main).toHaveClass('min-w-0');

    const outer = card.parentElement;
    expect(outer).toHaveClass('items-center');
    expect(outer).toHaveClass('justify-center');
    expect(outer).toHaveClass('min-h-screen');
  });

  it('renders horizontal top Steps with connectors between circles', () => {
    useWizardScanHandlers();
    renderInApp(
      <SetupWizardPage />,
      { client: makeClient(), initialRouterEntries: ['/setup'] },
    );

    const steps = screen.getAllByRole('listitem');
    expect(steps).toHaveLength(3);

    steps.forEach((li, index) => {
      expect(li).toHaveClass('items-center');
      expect(li).toHaveAttribute('data-step-id', ['agent', 'workspace', 'done'][index]);

      const spans = Array.from(li.children).filter((child) => child.tagName === 'SPAN');
      expect(spans).toHaveLength(2);
      expect(spans[0]).toHaveTextContent(String(index + 1));
      expect(spans[1]).toHaveTextContent(['Agent', 'Workspace', 'Done'][index]);
    });

    for (let i = 0; i < 2; i++) {
      const connector = steps[i].querySelector('[data-testid="step-connector"]');
      expect(connector).toBeInTheDocument();
      expect(connector).toHaveClass('h-px');
      expect(connector).toHaveClass('bg-setup-wizard-step-connector');
    }

    expect(steps[2].querySelector('[data-testid="step-connector"]')).not.toBeInTheDocument();
  });

  it('maps Steps complete/active/pending statuses as the wizard advances', async () => {
    const user = userEvent.setup();
    useWizardScanHandlers();

    renderInApp(<SetupWizardPage />, {
      client: makeClient(),
      initialRouterEntries: ['/setup'],
    });

    const progress = screen.getByRole('navigation', { name: 'Setup progress' });
    expect(progress.querySelector('[data-step-id="agent"]')).toHaveAttribute(
      'data-step-status',
      'active',
    );
    expect(progress.querySelector('[data-step-id="workspace"]')).toHaveAttribute(
      'data-step-status',
      'pending',
    );
    expect(screen.getByTestId('wizard-cta-row').querySelector('button[aria-label="Back"]')).not.toBeInTheDocument();

    await advanceAgentToWorkspace(user);

    expect(progress.querySelector('[data-step-id="agent"]')).toHaveAttribute(
      'data-step-status',
      'complete',
    );
    expect(progress.querySelector('[data-step-id="workspace"]')).toHaveAttribute(
      'data-step-status',
      'active',
    );
    expect(progress.querySelector('[data-step-id="done"]')).toHaveAttribute(
      'data-step-status',
      'pending',
    );

    const workspaceCta = screen.getByTestId('wizard-cta-row');
    expect(workspaceCta).toHaveAttribute('data-layout', 'horizontal-adjacent');
    expect(workspaceCta.querySelector('button[aria-label="Back"]')).toBeInTheDocument();
    expect(workspaceCta.querySelector('button[aria-label="Back"]')).not.toHaveTextContent('Back');

    await fillProfileName(user);
    await user.click(screen.getByRole('button', { name: 'Continue' }));
    await waitFor(() =>
      expect(screen.getByRole('heading', { name: /You're ready/ })).toBeInTheDocument(),
    );

    expect(progress.querySelector('[data-step-id="workspace"]')).toHaveAttribute(
      'data-step-status',
      'complete',
    );
    expect(progress.querySelector('[data-step-id="done"]')).toHaveAttribute(
      'data-step-status',
      'active',
    );
    expect(screen.getByTestId('wizard-cta-row').querySelector('button[aria-label="Back"]')).toBeInTheDocument();
  });

  it('moves through Agent → Workspace → Done and finishes', async () => {
    const user = userEvent.setup();
    useWizardScanHandlers();

    renderInApp(
      <>
        <LocationDisplay />
        <SetupWizardPage />
      </>,
      { client: makeClient(), initialRouterEntries: ['/setup'] },
    );

    expect(screen.getByRole('heading', { name: 'Choose an ACP agent' })).toBeInTheDocument();
    await advanceAgentToWorkspace(user);

    await fillProfileName(user);
    await user.click(screen.getByRole('button', { name: 'Continue' }));

    await waitFor(() => expect(screen.getByRole('heading', { name: /You're ready/ })).toBeInTheDocument());
    await user.click(screen.getByRole('button', { name: 'Open Nexus' }));

    await waitFor(() => expect(screen.getByTestId('location')).toHaveTextContent('/works'));
  });

  it('navigates Back from Workspace to Agent and from Done to Workspace', async () => {
    const user = userEvent.setup();
    useWizardScanHandlers();

    renderInApp(<SetupWizardPage />, {
      client: makeClient(),
      initialRouterEntries: ['/setup'],
    });

    await advanceAgentToWorkspace(user);
    await user.click(screen.getByRole('button', { name: 'Back' }));
    await waitFor(() =>
      expect(screen.getByRole('heading', { name: 'Choose an ACP agent' })).toBeInTheDocument(),
    );

    await advanceAgentToWorkspace(user);
    await fillProfileName(user);
    await user.click(screen.getByRole('button', { name: 'Continue' }));
    await waitFor(() =>
      expect(screen.getByRole('heading', { name: /You're ready/ })).toBeInTheDocument(),
    );

    await user.click(screen.getByRole('button', { name: 'Back' }));
    await waitFor(() =>
      expect(screen.getByRole('heading', { name: 'Name your Profile' })).toBeInTheDocument(),
    );
  });

  it('persists the selected agent profile on desktop before finishing', async () => {
    const user = userEvent.setup();
    const setAgentProfile = vi.fn(() => Promise.resolve());
    const setSetupCompleted = vi.fn(() => Promise.resolve());
    useWizardScanHandlers();

    renderInApp(
      <SetupWizardPage />,
      {
        client: makeClient(),
        desktop: makeDesktop({ setAgentProfile, setSetupCompleted }),
        initialRouterEntries: ['/setup'],
      },
    );

    await advanceAgentToWorkspace(user);
    await fillProfileName(user);
    await user.click(screen.getByRole('button', { name: 'Continue' }));

    await waitFor(() => expect(screen.getByRole('heading', { name: /You're ready/ })).toBeInTheDocument());
    await user.click(screen.getByRole('button', { name: 'Open Nexus' }));

    await waitFor(() => expect(setAgentProfile).toHaveBeenCalledWith('codex', 'codex'));
    await waitFor(() => expect(setSetupCompleted).toHaveBeenCalledWith(true));
  });

  it('shows a toast and stays on the workspace step when the profile display name update fails', async () => {
    const user = userEvent.setup();
    const setAgentProfile = vi.fn(() => Promise.resolve());
    const setSetupCompleted = vi.fn(() => Promise.resolve());
    useWizardScanHandlers();
    useHandlers(
      http.patch('/v1/daemon/creators/:id', () =>
        HttpResponse.json(
          { error: { code: 'permission_denied', message: 'permission denied' } },
          { status: 500 },
        ),
      ),
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

    await advanceAgentToWorkspace(user);
    await fillProfileName(user);
    await user.click(screen.getByRole('button', { name: 'Continue' }));

    await waitFor(() => expect(screen.getByText('permission denied')).toBeInTheDocument());
    expect(screen.getByRole('heading', { name: 'Name your Profile' })).toBeInTheDocument();
    expect(screen.queryByRole('heading', { name: /You're ready/ })).not.toBeInTheDocument();
    expect(setAgentProfile).not.toHaveBeenCalled();
    expect(setSetupCompleted).not.toHaveBeenCalled();
    expect(screen.getByTestId('location')).toHaveTextContent('/setup');
  });

  it('finish reaches gated /works while setSetupCompleted IPC is still pending', async () => {
    const user = userEvent.setup();
    let resolveIpc!: () => void;
    const setSetupCompleted = vi.fn(
      () =>
        new Promise<void>((resolve) => {
          resolveIpc = resolve;
        }),
    );
    const setAgentProfile = vi.fn(() => Promise.resolve());
    useWizardScanHandlers();

    renderInApp(
      <>
        <LocationDisplay />
        <Routes>
          <Route path="/setup" element={<SetupWizardPage />} />
          <Route
            path="/works"
            element={
              <SetupGate>
                <div data-testid="main-shell">Works</div>
              </SetupGate>
            }
          />
        </Routes>
      </>,
      {
        client: makeClient(),
        desktop: makeDesktop({ setAgentProfile, setSetupCompleted }),
        initialRouterEntries: ['/setup'],
        setupCompleted: false,
      },
    );

    await advanceAgentToWorkspace(user);
    await fillProfileName(user);
    await user.click(screen.getByRole('button', { name: 'Continue' }));
    await waitFor(() =>
      expect(screen.getByRole('heading', { name: /You're ready/ })).toBeInTheDocument(),
    );

    await user.click(screen.getByRole('button', { name: 'Open Nexus' }));

    await waitFor(() => expect(screen.getByTestId('main-shell')).toBeInTheDocument());
    expect(screen.getByTestId('location')).toHaveTextContent('/works');
    expect(setSetupCompleted).toHaveBeenCalledWith(true);
    resolveIpc();
  });

  it('scrolls long agent lists inside the portrait wizard card', async () => {
    // Use common priority ids so cards render in the common grid (visible
    // without expanding More). Two extra non-common agents exercise the rest
    // partition behind the More toggle.
    const commonIds = [
      'codex-acp', 'claude-acp', 'cursor', 'opencode', 'hermes',
      'kimi', 'qoder', 'github-copilot-cli', 'pi-acp', 'kiro',
    ];
    useHandlers(
      http.post('/v1/daemon/agent-host/scan', () =>
        HttpResponse.json({
          agents: [
            ...commonIds.map((id, i) => ({
              name: `agent-${i}`,
              registry_agent_id: id,
              installed: true,
              version: '1.0.0',
            })),
            { name: 'extra-a', registry_agent_id: 'extra-a', installed: true, version: '1.0.0' },
            { name: 'extra-b', registry_agent_id: 'extra-b', installed: true, version: '1.0.0' },
          ],
        }),
      ),
    );

    renderInApp(<SetupWizardPage />, {
      client: makeClient(),
      initialRouterEntries: ['/setup'],
    });

    const card = screen.getByTestId('setup-wizard-card');
    const main = screen.getByRole('main');
    expect(card).toHaveClass('overflow-hidden');
    expect(main).toHaveClass('min-h-0', 'min-w-0', 'flex-1', 'overflow-hidden');

    await waitFor(() => expect(screen.getByTestId('agent-picker-grid')).toBeInTheDocument());
    const grid = screen.getByTestId('agent-picker-grid');
    expect(grid.children.length).toBeGreaterThan(6);
    expect(screen.getByTestId('wizard-cta-row')).toBeInTheDocument();
  });
});
