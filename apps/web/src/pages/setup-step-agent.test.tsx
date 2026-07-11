import { http, HttpResponse } from 'msw';
import { describe, expect, it, vi } from 'vitest';
import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useCallback, useState } from 'react';

import {
  SetupStepAgent,
  agentPickerId,
  assignCollisionSafePickerIds,
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

function Harness({ initial, onNext = vi.fn(), onBack }: HarnessProps) {
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

/**
 * Expand the "More agents" toggle so non-common agent cards become visible.
 * V1.110 partition: common agents render immediately; rest are behind More.
 */
async function expandRestAgents(user: ReturnType<typeof userEvent.setup>) {
  const moreBtn = await waitFor(() => screen.getByTestId('agent-picker-more'));
  await user.click(moreBtn);
}

describe('mapScanEntriesToPickerItems / URL table', () => {
  it('maps live registry ids (claude-acp / codex-acp / gemini) to outbound URLs', () => {
    const items = mapScanEntriesToPickerItems([
      makeAgent({
        name: 'Claude Code',
        registry_agent_id: 'claude-acp',
        installed: true,
      }),
      makeAgent({
        name: 'Codex',
        registry_agent_id: 'codex-acp',
        installed: true,
        version: '0.1.0',
      }),
      makeAgent({
        name: 'Gemini CLI',
        registry_agent_id: 'gemini',
        installed: false,
      }),
      makeAgent({ name: 'Unknown Agent', installed: false }),
    ]);
    expect(items[0]!.id).toBe('claude-acp');
    expect(items[0]!.installUrl).toContain('docs.anthropic.com');
    expect(items[1]!.id).toBe('codex-acp');
    expect(items[1]!.installUrl).toContain('github.com/openai/codex');
    expect(items[1]!.docsUrl).toBeNull();
    expect(items[2]!.id).toBe('gemini');
    expect(items[2]!.installUrl).toContain('gemini-cli');
    expect(items[3]!.installUrl).toBeNull();
    expect(items[3]!.docsUrl).toBeNull();
  });

  it('lookupAgentOutboundUrls prefers live registry id then aliases', () => {
    expect(lookupAgentOutboundUrls('claude-acp', 'Other').installUrl).toBeTruthy();
    expect(lookupAgentOutboundUrls('codex-acp', 'Other').installUrl).toBeTruthy();
    expect(lookupAgentOutboundUrls('gemini', 'Other').installUrl).toBeTruthy();
    expect(lookupAgentOutboundUrls(null, 'codex').installUrl).toBeTruthy();
    expect(lookupAgentOutboundUrls(null, 'nope')).toEqual({});
  });

  it('agentPickerId prefers registry_agent_id', () => {
    expect(
      agentPickerId(makeAgent({ name: 'Display', registry_agent_id: 'reg-1' })),
    ).toBe('reg-1');
    expect(agentPickerId(makeAgent({ name: 'Display' }))).toBe('Display');
  });

  it('assignCollisionSafePickerIds suffixes duplicates (B6)', () => {
    const ids = assignCollisionSafePickerIds([
      makeAgent({ name: 'A', registry_agent_id: 'dup' }),
      makeAgent({ name: 'B', registry_agent_id: 'dup' }),
      makeAgent({ name: 'C', registry_agent_id: 'unique' }),
    ]);
    expect(ids).toEqual(['dup', 'dup#1', 'unique']);
    const items = mapScanEntriesToPickerItems([
      makeAgent({ name: 'A', registry_agent_id: 'dup', installed: true }),
      makeAgent({ name: 'B', registry_agent_id: 'dup', installed: false }),
    ]);
    expect(items.map((i) => i.id)).toEqual(['dup', 'dup#1']);
  });
});

describe('SetupStepAgent', () => {
  it('renders the agent scan list via AgentPicker', async () => {
    const user = userEvent.setup();
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

    // Both agents are non-common — expand More to reveal the rest partition.
    await expandRestAgents(user);
    await waitFor(() => expect(screen.getByText('nexus-mcp-agent')).toBeInTheDocument());
    expect(screen.getByText('custom-agent')).toBeInTheDocument();
    expect(screen.getByText('Version 1.0.0')).toBeInTheDocument();
    expect(screen.getByText('Installed')).toBeInTheDocument();
    expect(screen.getByText('Not installed')).toBeInTheDocument();
    expect(screen.getByTestId('agent-picker')).toHaveAttribute('data-status', 'ready');
  });

  it('auto-selects the first installed agent (B4)', async () => {
    const agent = makeAgent({
      name: 'Claude Code',
      registry_agent_id: 'claude-acp',
      installed: true,
    });
    useHandlers(
      http.post('/v1/daemon/agent-host/scan', () =>
        HttpResponse.json({ agents: [agent, makeAgent({ name: 'missing', installed: false })] }),
      ),
    );

    renderHarness(makeState());

    await waitFor(() =>
      expect(screen.getByTestId('agent-card-select-claude-acp')).toHaveAttribute(
        'aria-pressed',
        'true',
      ),
    );
    await waitFor(() =>
      expect(screen.getByRole('button', { name: 'Continue' })).toBeEnabled(),
    );
  });

  it('selects an installed agent and calls onChange with the selected agent', async () => {
    const user = userEvent.setup();
    const agent = makeAgent({
      name: 'Codex',
      registry_agent_id: 'codex-acp',
      installed: true,
      version: '1.0.0',
    });
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
      />,
      { client: makeClient(), initialRouterEntries: ['/setup'] },
    );

    await waitFor(() => expect(screen.getByText('Codex')).toBeInTheDocument());
    await user.click(screen.getByTestId('agent-card-select-codex-acp'));

    await waitFor(() => expect(onChange).toHaveBeenCalled());
    const lastCall = onChange.mock.calls[onChange.mock.calls.length - 1]![0];
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
      />,
      { client: makeClient(), initialRouterEntries: ['/setup'] },
    );

    // 'missing-agent' is non-common — expand More to reveal it.
    await expandRestAgents(user);
    await waitFor(() => expect(screen.getByText('missing-agent')).toBeInTheDocument());
    const card = screen.getByTestId('agent-card-missing-agent');
    expect(card.tagName).toBe('DIV');
    expect(screen.queryByTestId('agent-card-select-missing-agent')).toBeNull();
    await user.click(card);

    const selectedCalls = onChange.mock.calls.filter(
      ([next]) => next.selectedAgent != null,
    );
    expect(selectedCalls).toHaveLength(0);
  });

  it('keeps Continue disabled when only not-installed agents are present (B4)', async () => {
    const user = userEvent.setup();
    useHandlers(
      http.post('/v1/daemon/agent-host/scan', () =>
        HttpResponse.json({
          agents: [makeAgent({ name: 'missing-only', installed: false })],
        }),
      ),
    );

    renderHarness(makeState());

    // 'missing-only' is non-common — expand More to verify it rendered.
    await expandRestAgents(user);
    await waitFor(() => expect(screen.getByText('missing-only')).toBeInTheDocument());
    expect(screen.getByRole('button', { name: 'Continue' })).toBeDisabled();
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
    // FB-UI-008: Setup offers the custom launch escape hatch when no agents
    // are found, so authors can still configure a non-registry agent.
    expect(screen.getByTestId('agent-picker-custom-launch')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Continue' })).toBeDisabled();
  });

  it('renders Continue as a wide prominent CTA without Back on the first step', async () => {
    useHandlers(
      http.post('/v1/daemon/agent-host/scan', () => HttpResponse.json({ agents: [] })),
    );

    renderHarness(makeState());

    await waitFor(() =>
      expect(screen.getByText('No agents found on PATH.')).toBeInTheDocument(),
    );

    const continueButton = screen.getByRole('button', { name: 'Continue' });
    expect(continueButton).toHaveClass('w-full', 'max-w-setup-wizard-surface-cta-primary-max-width');

    const cta = screen.getByTestId('wizard-cta-row');
    expect(cta).toHaveAttribute('data-layout', 'horizontal-adjacent');
    expect(cta).toHaveClass('flex', 'items-center');
    expect(cta).not.toHaveClass('flex-col');
    expect(cta.querySelector('button[aria-label="Back"]')).not.toBeInTheDocument();
  });

  it('shows error status with retry and keeps Continue disabled without a selected agent', async () => {
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
    // FB-UI-008: custom launch escape hatch stays available even when the scan
    // errors, so authors are not blocked by a transient scan failure.
    expect(screen.getByTestId('agent-picker-custom-launch')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Try again' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Continue' })).toBeDisabled();
  });

  it('shows Install link for known registry ids and hides links when URLs missing', async () => {
    const user = userEvent.setup();
    useHandlers(
      http.post('/v1/daemon/agent-host/scan', () =>
        HttpResponse.json({
          agents: [
            makeAgent({
              name: 'Codex',
              registry_agent_id: 'codex-acp',
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
    expect(screen.queryByRole('link', { name: /Docs/i })).not.toBeInTheDocument();
    // 'mystery-agent' is non-common — expand More to verify no links render.
    await expandRestAgents(user);
    expect(screen.getByTestId('agent-card-mystery-agent').querySelector('a')).toBeNull();
  });

  it('enables Continue after verifying a custom launch command (FB-UI-008)', async () => {
    const user = userEvent.setup();
    const customCommand = '/usr/local/bin/my-agent';
    useHandlers(
      http.post('/v1/daemon/agent-host/scan', () =>
        HttpResponse.json({
          agents: [
            makeAgent({
              name: 'my-agent',
              registry_agent_id: 'my-agent',
              installed: true,
              launch_command: customCommand,
            }),
          ],
        }),
      ),
    );

    renderHarness(makeState());

    // 'my-agent' is non-common — expand More so its card is visible.
    await expandRestAgents(user);
    await waitFor(() => expect(screen.getByText('my-agent')).toBeInTheDocument());
    // Auto-select enables Continue via the installed agent.
    await waitFor(() =>
      expect(screen.getByRole('button', { name: 'Continue' })).toBeEnabled(),
    );

    // Switching to a custom command clears the selection → Continue gated on verify.
    const input = screen.getByPlaceholderText('e.g. /usr/local/bin/my-agent');
    await user.type(input, customCommand);
    expect(screen.getByRole('button', { name: 'Continue' })).toBeDisabled();

    // Verify matches the installed agent's launch command → Continue enabled.
    await user.click(screen.getByRole('button', { name: 'Verify Agent' }));
    await waitFor(() =>
      expect(screen.getByRole('button', { name: 'Continue' })).toBeEnabled(),
    );
    expect(screen.getByTestId('agent-picker-verify-success')).toBeInTheDocument();
  });

  it('keeps Continue disabled when Verify fails on an unmatched command (FB-UI-008)', async () => {
    const user = userEvent.setup();
    useHandlers(
      http.post('/v1/daemon/agent-host/scan', () => HttpResponse.json({ agents: [] })),
    );

    renderHarness(makeState());

    await waitFor(() =>
      expect(screen.getByTestId('agent-picker')).toHaveAttribute('data-status', 'empty'),
    );

    const input = screen.getByPlaceholderText('e.g. /usr/local/bin/my-agent');
    await user.type(input, '/bin/nonexistent');

    expect(screen.getByRole('button', { name: 'Continue' })).toBeDisabled();
    await user.click(screen.getByRole('button', { name: 'Verify Agent' }));
    await waitFor(() =>
      expect(screen.getByTestId('agent-picker-verify-error')).toBeInTheDocument(),
    );
    expect(screen.getByRole('button', { name: 'Continue' })).toBeDisabled();
  });
});
