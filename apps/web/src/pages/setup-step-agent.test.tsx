import { http, HttpResponse } from 'msw';
import { describe, expect, it, vi } from 'vitest';
import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useCallback, useState } from 'react';

import { SetupStepAgent } from '@/pages/setup-step-agent';
import { renderInApp } from '@/test/test-providers';
import { useHandlers } from '@/test/msw-server';
import { BrowserClient } from '@/lib/nexus';
import type { AgentScanEntry } from '@42ch/nexus-contracts';
import type { WizardState } from '@/pages/setup-wizard-page';

function makeClient() {
  return new BrowserClient();
}

function makeState(overrides: Partial<WizardState> = {}): WizardState {
  return {
    workspaceRoot: '',
    selectedAgent: null,
    customLaunchCommand: '',
    ...overrides,
  };
}

function makeAgent(overrides: Partial<AgentScanEntry> = {}): AgentScanEntry {
  return {
    name: 'test-agent',
    installed: false,
    ...overrides,
  };
}

interface HarnessProps {
  initial: WizardState;
  onNext?: () => void;
  onBack?: () => void;
}

function Harness({ initial, onNext = vi.fn(), onBack = vi.fn() }: HarnessProps) {
  const [state, setState] = useState<WizardState>(initial);
  const onChange = useCallback((next: WizardState) => setState(next), []);
  return (
    <SetupStepAgent
      state={state}
      onChange={onChange}
      onNext={onNext}
      onBack={onBack}
    />
  );
}

function renderHarness(
  initial: WizardState,
  options: { onNext?: () => void; onBack?: () => void } = {},
) {
  return renderInApp(
    <Harness initial={initial} onNext={options.onNext} onBack={options.onBack} />,
    {
      client: makeClient(),
      initialRouterEntries: ['/setup'],
    },
  );
}

describe('SetupStepAgent', () => {
  it('renders the agent scan list', async () => {
    useHandlers(
      http.post('/v1/daemon/agent-host/scan', () =>
        HttpResponse.json({
          agents: [
            makeAgent({ name: 'nexus-mcp-agent', installed: true, version: '1.0.0' }),
            makeAgent({ name: 'custom-agent', installed: false }),
          ],
        }),
      ),
    );

    renderHarness(makeState());

    await waitFor(() => expect(screen.getByText('nexus-mcp-agent')).toBeInTheDocument());
    expect(screen.getByText('custom-agent')).toBeInTheDocument();
    expect(screen.getByText('Version 1.0.0')).toBeInTheDocument();
    expect(screen.getByText('Installed')).toBeInTheDocument();
    expect(screen.getByText('Not installed')).toBeInTheDocument();
  });

  it('selects an agent and calls onChange with the selected agent', async () => {
    const user = userEvent.setup();
    const agent = makeAgent({ name: 'nexus-mcp-agent', installed: true, version: '1.0.0' });
    const onChange = vi.fn();

    useHandlers(
      http.post('/v1/daemon/agent-host/scan', () =>
        HttpResponse.json({ agents: [agent] }),
      ),
    );

    renderInApp(
      <SetupStepAgent
        state={makeState()}
        onChange={onChange}
        onNext={vi.fn()}
        onBack={vi.fn()}
      />,
      { client: makeClient(), initialRouterEntries: ['/setup'] },
    );

    await waitFor(() => expect(screen.getByText('nexus-mcp-agent')).toBeInTheDocument());
    await user.click(screen.getByText('nexus-mcp-agent'));

    await waitFor(() => expect(onChange).toHaveBeenCalled());
    const lastCall = onChange.mock.calls[onChange.mock.calls.length - 1][0];
    expect(lastCall.selectedAgent).toEqual(agent);
    expect(lastCall.customLaunchCommand).toBe('');
  });

  it('disables Continue until an agent is selected or a custom command is entered', async () => {
    const user = userEvent.setup();
    const agent = makeAgent({ name: 'nexus-mcp-agent', installed: true });

    useHandlers(
      http.post('/v1/daemon/agent-host/scan', () =>
        HttpResponse.json({ agents: [agent] }),
      ),
    );

    renderHarness(makeState());

    const continueButton = await waitFor(() =>
      screen.getByRole('button', { name: 'Continue' }),
    );
    expect(continueButton).toBeDisabled();

    await waitFor(() => expect(screen.getByText('nexus-mcp-agent')).toBeInTheDocument());
    await user.click(screen.getByText('nexus-mcp-agent'));
    await waitFor(() => expect(continueButton).toBeEnabled());
  });

  it('renders Continue as a wide prominent CTA and Back as a smaller tertiary button', async () => {
    useHandlers(
      http.post('/v1/daemon/agent-host/scan', () => HttpResponse.json({ agents: [] })),
    );

    renderHarness(makeState());

    await waitFor(() =>
      expect(screen.getByText('No agents found on PATH.')).toBeInTheDocument(),
    );

    const continueButton = screen.getByRole('button', { name: 'Continue' });
    expect(continueButton).toHaveClass('w-full', 'max-w-setup-wizard-surface-cta-primary-max-width');

    const backButton = screen.getByRole('button', { name: 'Back' });
    expect(backButton).toHaveClass('self-start');
  });

  it('updates state when a custom launch command is typed', async () => {
    const user = userEvent.setup();
    const onNext = vi.fn();

    useHandlers(
      http.post('/v1/daemon/agent-host/scan', () => HttpResponse.json({ agents: [] })),
    );

    renderHarness(makeState(), { onNext });

    await waitFor(() =>
      expect(screen.getByText('No agents found on PATH.')).toBeInTheDocument(),
    );

    const input = screen.getByLabelText(/Use custom launch command/i);
    await user.type(input, '/usr/local/bin/my-agent');

    const continueButton = screen.getByRole('button', { name: 'Continue' });
    await waitFor(() => expect(continueButton).toBeEnabled());
    await user.click(continueButton);

    await waitFor(() => expect(onNext).toHaveBeenCalled());
  });
});
