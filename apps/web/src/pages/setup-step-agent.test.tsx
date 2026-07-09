import { http, HttpResponse } from 'msw';
import { describe, expect, it, vi } from 'vitest';
import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useCallback, useState } from 'react';

import {
  SetupStepAgent,
  agentPickerId,
  mapScanEntriesToPickerItems,
} from '@/pages/setup-step-agent';
import { lookupAgentOutboundUrls } from '@/pages/setup-agent-urls';
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

describe('mapScanEntriesToPickerItems / URL table', () => {
  it('maps wire entries and attaches outbound URLs from the static table', () => {
    const items = mapScanEntriesToPickerItems([
      makeAgent({
        name: 'Codex',
        registry_agent_id: 'codex',
        installed: true,
        version: '0.1.0',
      }),
      makeAgent({ name: 'Unknown Agent', installed: false }),
    ]);
    expect(items[0].id).toBe('codex');
    expect(items[0].installUrl).toContain('github.com/openai/codex');
    expect(items[0].docsUrl).toBeNull();
    expect(items[1].installUrl).toBeNull();
    expect(items[1].docsUrl).toBeNull();
  });

  it('lookupAgentOutboundUrls prefers registry id then name', () => {
    expect(lookupAgentOutboundUrls('claude-code', 'Other').installUrl).toBeTruthy();
    expect(lookupAgentOutboundUrls(null, 'codex').installUrl).toBeTruthy();
    expect(lookupAgentOutboundUrls(null, 'nope')).toEqual({});
  });

  it('agentPickerId prefers registry_agent_id', () => {
    expect(
      agentPickerId(makeAgent({ name: 'Display', registry_agent_id: 'reg-1' })),
    ).toBe('reg-1');
    expect(agentPickerId(makeAgent({ name: 'Display' }))).toBe('Display');
  });
});

describe('SetupStepAgent', () => {
  it('renders the agent scan list via AgentPicker', async () => {
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
    expect(screen.getByTestId('agent-picker')).toHaveAttribute('data-status', 'ready');
  });

  it('selects an installed agent and calls onChange with the selected agent', async () => {
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

  it('does not select not-installed cards', async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    const missing = makeAgent({ name: 'missing-agent', installed: false });

    useHandlers(
      http.post('/v1/daemon/agent-host/scan', () =>
        HttpResponse.json({ agents: [missing] }),
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

    await waitFor(() => expect(screen.getByText('missing-agent')).toBeInTheDocument());
    const card = screen.getByTestId('agent-card-missing-agent');
    expect(card.tagName).toBe('DIV');
    await user.click(card);

    const selectedCalls = onChange.mock.calls.filter(
      ([next]) => next.selectedAgent != null,
    );
    expect(selectedCalls).toHaveLength(0);
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

  it('uses status=empty when scan returns no agents', async () => {
    useHandlers(
      http.post('/v1/daemon/agent-host/scan', () => HttpResponse.json({ agents: [] })),
    );

    renderHarness(makeState());

    await waitFor(() =>
      expect(screen.getByTestId('agent-picker')).toHaveAttribute('data-status', 'empty'),
    );
    expect(screen.getByText('No agents found on PATH.')).toBeInTheDocument();
    expect(screen.getByTestId('agent-picker-custom-launch')).toBeInTheDocument();
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

  it('shows error status with custom launch and retry', async () => {
    const user = userEvent.setup();
    useHandlers(
      http.post('/v1/daemon/agent-host/scan', () =>
        HttpResponse.json({ error: { code: 'internal', message: 'scan failed' } }, { status: 500 }),
      ),
    );

    renderHarness(makeState());

    await waitFor(() =>
      expect(screen.getByTestId('agent-picker')).toHaveAttribute('data-status', 'error'),
    );
    expect(screen.getByText('Could not scan for agents')).toBeInTheDocument();
    expect(screen.getByTestId('agent-picker-custom-launch')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Try again' })).toBeInTheDocument();

    // Custom launch still enables Continue on error.
    const input = screen.getByLabelText(/Use custom launch command/i);
    await user.type(input, '/bin/custom-agent');
    await waitFor(() =>
      expect(screen.getByRole('button', { name: 'Continue' })).toBeEnabled(),
    );
  });

  it('shows Install link for known agents and hides links when URLs missing', async () => {
    useHandlers(
      http.post('/v1/daemon/agent-host/scan', () =>
        HttpResponse.json({
          agents: [
            makeAgent({
              name: 'Codex',
              registry_agent_id: 'codex',
              installed: true,
            }),
            makeAgent({ name: 'mystery-agent', installed: false }),
          ],
        }),
      ),
    );

    renderHarness(makeState());

    await waitFor(() => expect(screen.getByText('Codex')).toBeInTheDocument());
    expect(screen.getByRole('link', { name: /Install/i })).toHaveAttribute(
      'href',
      'https://github.com/openai/codex',
    );
    // Codex docs URL is null in the static table.
    expect(screen.queryByRole('link', { name: /Docs/i })).not.toBeInTheDocument();
    expect(screen.getByTestId('agent-card-mystery-agent').querySelector('a')).toBeNull();
  });
});
