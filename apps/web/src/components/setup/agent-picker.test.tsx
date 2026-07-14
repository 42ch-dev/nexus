/**
 * AgentPicker presentational unit tests (V1.101 Task 2 + fix-wave B2).
 * V1.117 P1 T3: defaultGrid + moreAgents split; icon + displayName.
 */
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

import '@/lib/i18n/config';

import {
  AgentPicker,
  type AgentPickerItem,
} from '@/components/setup/agent-picker';

const DEFAULT_GRID: AgentPickerItem[] = [
  {
    id: 'claude-native',
    name: 'Claude',
    displayName: 'Claude',
    iconUrl: 'https://example.com/claude-icon.svg',
    version: '1.0.0',
    installed: true,
    installUrl: 'https://example.com/install',
    docsUrl: 'https://example.com/docs',
  },
  {
    id: 'codex-native',
    name: 'Codex',
    displayName: 'Codex',
    installed: true,
    installUrl: 'https://example.com/codex-install',
    docsUrl: null,
  },
];

const MORE_AGENTS: AgentPickerItem[] = [
  {
    id: 'claude-acp',
    name: 'Claude ACP',
    installed: false,
    installUrl: null,
    docsUrl: null,
  },
  {
    id: 'missing',
    name: 'Missing Agent',
    installed: false,
    installUrl: null,
    docsUrl: null,
  },
];

describe('AgentPicker', () => {
  it('renders loading state', () => {
    render(<AgentPicker status="loading" />);
    expect(screen.getByText('Scanning for local agents\u2026')).toBeInTheDocument();
    expect(screen.getByTestId('agent-picker')).toHaveAttribute('data-status', 'loading');
  });

  it('selects installed agents only', async () => {
    const user = userEvent.setup();
    const onSelect = vi.fn();
    render(
      <AgentPicker
        status="ready"
        defaultGrid={DEFAULT_GRID}
        moreAgents={MORE_AGENTS}
        selectedId={null}
        onSelect={onSelect}
        customLaunchValue=""
        onCustomLaunchChange={() => undefined}
      />,
    );

    await user.click(screen.getByTestId('agent-card-select-claude-native'));
    expect(onSelect).toHaveBeenCalledWith('claude-native');

    await user.click(screen.getByTestId('agent-picker-more'));
    expect(screen.queryByTestId('agent-card-select-claude-acp')).toBeNull();
    expect(screen.getByTestId('agent-card-claude-acp').tagName).toBe('DIV');
  });

  it('keeps Install/Docs links outside the select button (B2)', () => {
    render(
      <AgentPicker
        status="ready"
        defaultGrid={DEFAULT_GRID}
        onSelect={() => undefined}
        customLaunchValue=""
        onCustomLaunchChange={() => undefined}
      />,
    );
    const select = screen.getByTestId('agent-card-select-claude-native');
    expect(select.querySelector('a')).toBeNull();
    const card = screen.getByTestId('agent-card-claude-native');
    expect(card.querySelector('a[href="https://example.com/install"]')).not.toBeNull();
    expect(card.querySelector('a[href="https://example.com/docs"]')).not.toBeNull();
  });

  it('hides install/docs when URLs missing and shows when present', async () => {
    const user = userEvent.setup();
    render(
      <AgentPicker
        status="ready"
        defaultGrid={DEFAULT_GRID}
        moreAgents={MORE_AGENTS}
        onSelect={() => undefined}
        customLaunchValue=""
        onCustomLaunchChange={() => undefined}
      />,
    );
    const installLinks = screen.getAllByRole('link', { name: /Install/i });
    expect(installLinks[0]).toHaveAttribute('href', 'https://example.com/install');
    const docsLink = screen.getByRole('link', { name: /Docs/i });
    expect(docsLink).toHaveAttribute('href', 'https://example.com/docs');
    await user.click(screen.getByTestId('agent-picker-more'));
    expect(screen.getByTestId('agent-card-missing').querySelector('a')).toBeNull();
  });

  it('shows soft Installed Badge beside title and mutes not-installed title', async () => {
    const user = userEvent.setup();
    render(
      <AgentPicker
        status="ready"
        defaultGrid={DEFAULT_GRID}
        moreAgents={MORE_AGENTS}
        selectedId={null}
        onSelect={() => undefined}
        customLaunchValue=""
        onCustomLaunchChange={() => undefined}
      />,
    );
    expect(screen.getByTestId('agent-card-installed-badge-claude-native')).toHaveTextContent(
      'Installed',
    );
    await user.click(screen.getByTestId('agent-picker-more'));
    expect(screen.getByText('Missing Agent')).toHaveClass('text-gray-700');
    expect(screen.getAllByText('Not installed').length).toBe(2);
  });

  it('uses hollow dot when installed-unselected and lit when selected', async () => {
    const user = userEvent.setup();
    const { rerender } = render(
      <AgentPicker
        status="ready"
        defaultGrid={DEFAULT_GRID}
        moreAgents={MORE_AGENTS}
        selectedId={null}
        onSelect={() => undefined}
        customLaunchValue=""
        onCustomLaunchChange={() => undefined}
      />,
    );
    const installedCard = screen.getByTestId('agent-card-claude-native');
    expect(installedCard.querySelector('[data-dot="hollow"]')).not.toBeNull();

    await user.click(screen.getByTestId('agent-picker-more'));

    rerender(
      <AgentPicker
        status="ready"
        defaultGrid={DEFAULT_GRID}
        moreAgents={MORE_AGENTS}
        selectedId="claude-native"
        onSelect={() => undefined}
        customLaunchValue=""
        onCustomLaunchChange={() => undefined}
      />,
    );
    expect(
      screen.getByTestId('agent-card-claude-native').querySelector('[data-dot="lit"]'),
    ).not.toBeNull();
    expect(
      screen.getByTestId('agent-card-missing').querySelector('[data-dot="muted"]'),
    ).not.toBeNull();
  });

  it('uses hollow GRAY border for unselected installed dot (FB-UI-006)', () => {
    render(
      <AgentPicker
        status="ready"
        defaultGrid={DEFAULT_GRID}
        selectedId={null}
        onSelect={() => undefined}
        customLaunchValue=""
        onCustomLaunchChange={() => undefined}
      />,
    );
    const unselectedDot = screen
      .getByTestId('agent-card-claude-native')
      .querySelector('[data-dot="hollow"] span');
    expect(unselectedDot).toHaveClass('border-gray-500');
    expect(unselectedDot).not.toHaveClass('border-green-700');
  });

  it('uses filled GREEN when selected (FB-UI-006)', () => {
    render(
      <AgentPicker
        status="ready"
        defaultGrid={DEFAULT_GRID}
        selectedId="claude-native"
        onSelect={() => undefined}
        customLaunchValue=""
        onCustomLaunchChange={() => undefined}
      />,
    );
    const selectedDot = screen
      .getByTestId('agent-card-claude-native')
      .querySelector('[data-dot="lit"] span');
    expect(selectedDot).toHaveClass('bg-green-700');
  });

  it('renders an ArrowUpRight icon inside outbound links', () => {
    render(
      <AgentPicker
        status="ready"
        defaultGrid={DEFAULT_GRID}
        onSelect={() => undefined}
        customLaunchValue=""
        onCustomLaunchChange={() => undefined}
      />,
    );
    const installLinks = screen.getAllByRole('link', { name: /Install/i });
    expect(installLinks[0].querySelector('svg')).not.toBeNull();
    expect(installLinks[0].querySelector('svg')).toHaveClass('lucide-arrow-up-right');
  });

  it('shows custom launch on empty and error', () => {
    const { rerender } = render(
      <AgentPicker
        status="empty"
        customLaunchValue=""
        onCustomLaunchChange={() => undefined}
      />,
    );
    expect(screen.getByTestId('agent-picker-custom-launch')).toBeInTheDocument();

    rerender(
      <AgentPicker
        status="error"
        errorDescription="boom"
        customLaunchValue=""
        onCustomLaunchChange={() => undefined}
      />,
    );
    expect(screen.getByTestId('agent-picker-custom-launch')).toBeInTheDocument();
    expect(screen.getByText('Could not scan for agents')).toBeInTheDocument();
  });

  it('paints hover background on the outer AgentCard div, not inner button (FB-UI-007)', () => {
    render(
      <AgentPicker
        status="ready"
        defaultGrid={DEFAULT_GRID}
        selectedId={null}
        onSelect={() => undefined}
        customLaunchValue=""
        onCustomLaunchChange={() => undefined}
      />,
    );
    const card = screen.getByTestId('agent-card-claude-native');
    expect(card).toHaveClass('hover:bg-gray-alpha-100');

    const selectBtn = screen.getByTestId('agent-card-select-claude-native');
    expect(selectBtn).not.toHaveClass('hover:bg-gray-alpha-100');
  });

  it('shows Verify button when onVerify is provided', async () => {
    const user = userEvent.setup();
    const onVerify = vi.fn();
    render(
      <AgentPicker
        status="empty"
        customLaunchValue="claude"
        onCustomLaunchChange={() => undefined}
        onVerify={onVerify}
        verifyStatus="idle"
      />,
    );
    const btn = screen.getByTestId('agent-picker-verify');
    expect(btn).toHaveTextContent('Verify');
    await user.click(btn);
    expect(onVerify).toHaveBeenCalledOnce();
  });

  it('disables Verify button when command is empty', () => {
    render(
      <AgentPicker
        status="empty"
        customLaunchValue=""
        onCustomLaunchChange={() => undefined}
        onVerify={() => undefined}
        verifyStatus="idle"
      />,
    );
    expect(screen.getByTestId('agent-picker-verify')).toBeDisabled();
  });

  it('hides Verify button when onVerify is omitted', () => {
    render(
      <AgentPicker
        status="empty"
        customLaunchValue="claude"
        onCustomLaunchChange={() => undefined}
      />,
    );
    expect(screen.queryByTestId('agent-picker-verify')).toBeNull();
  });

  it('shows verifying spinner when status is loading', () => {
    render(
      <AgentPicker
        status="empty"
        customLaunchValue="claude"
        onCustomLaunchChange={() => undefined}
        onVerify={() => undefined}
        verifyStatus="loading"
      />,
    );
    const btn = screen.getByTestId('agent-picker-verify');
    expect(btn).toBeDisabled();
    expect(btn).toHaveTextContent('Verifying\u2026');
  });

  it('shows success helper when verifyStatus is success', () => {
    render(
      <AgentPicker
        status="empty"
        customLaunchValue="claude"
        onCustomLaunchChange={() => undefined}
        onVerify={() => undefined}
        verifyStatus="success"
      />,
    );
    expect(screen.getByTestId('agent-picker-verify-success')).toHaveTextContent(
      'Agent responded successfully.',
    );
    expect(screen.queryByTestId('agent-picker-verify-error')).toBeNull();
  });

  it('shows failure helper when verifyStatus is error', () => {
    render(
      <AgentPicker
        status="empty"
        customLaunchValue="/bad/path"
        onCustomLaunchChange={() => undefined}
        onVerify={() => undefined}
        verifyStatus="error"
      />,
    );
    expect(screen.getByTestId('agent-picker-verify-error')).toHaveTextContent(
      'Could not reach this agent. Check the command and try again.',
    );
    expect(screen.queryByTestId('agent-picker-verify-success')).toBeNull();
  });

  it('shows no-match helper when verifyStatus is no-match (R-V1108P1QC2-S001)', () => {
    render(
      <AgentPicker
        status="empty"
        customLaunchValue="/bin/nonexistent"
        onCustomLaunchChange={() => undefined}
        onVerify={() => undefined}
        verifyStatus="no-match"
      />,
    );
    expect(screen.getByTestId('agent-picker-verify-error')).toHaveTextContent(
      'No matching agent for this command. Check the command and try again.',
    );
    expect(screen.queryByTestId('agent-picker-verify-success')).toBeNull();
  });

  it('renders defaultGrid cards in the primary grid and moreAgents behind toggle', async () => {
    const user = userEvent.setup();
    const grid: AgentPickerItem[] = [
      { id: 'claude-native', name: 'Claude', installed: true },
      { id: 'codex-native', name: 'Codex', installed: true },
    ];
    const more: AgentPickerItem[] = [
      { id: 'custom-tool', name: 'Custom', installed: false },
    ];
    render(
      <AgentPicker
        status="ready"
        defaultGrid={grid}
        moreAgents={more}
        onSelect={() => undefined}
        customLaunchValue=""
        onCustomLaunchChange={() => undefined}
      />,
    );

    const gridEl = screen.getByTestId('agent-picker-grid');
    expect(gridEl.querySelector('[data-testid="agent-card-claude-native"]')).toBeInTheDocument();
    expect(gridEl.querySelector('[data-testid="agent-card-codex-native"]')).toBeInTheDocument();

    expect(screen.queryByTestId('agent-card-custom-tool')).toBeNull();

    await user.click(screen.getByTestId('agent-picker-more'));
    expect(screen.getByTestId('agent-card-custom-tool')).toBeInTheDocument();
  });

  it('renders displayName when provided', () => {
    const grid: AgentPickerItem[] = [
      { id: 'claude-native', name: 'Claude', displayName: 'Claude', installed: true },
      { id: 'codex-native', name: 'Codex', displayName: 'Codex', installed: true },
    ];
    render(
      <AgentPicker
        status="ready"
        defaultGrid={grid}
        onSelect={() => undefined}
        customLaunchValue=""
        onCustomLaunchChange={() => undefined}
      />,
    );

    expect(screen.getByText('Claude')).toBeInTheDocument();
    expect(screen.getByText('Codex')).toBeInTheDocument();
  });

  it('renders icon when iconUrl is provided', () => {
    const grid: AgentPickerItem[] = [
      { id: 'claude-native', name: 'Claude', iconUrl: 'https://example.com/icon.svg', installed: true },
    ];
    render(
      <AgentPicker
        status="ready"
        defaultGrid={grid}
        onSelect={() => undefined}
        customLaunchValue=""
        onCustomLaunchChange={() => undefined}
      />,
    );

    const img = screen.getByTestId('agent-card-claude-native').querySelector('img');
    expect(img).not.toBeNull();
    expect(img).toHaveAttribute('src', 'https://example.com/icon.svg');
  });

  it('shows placeholder icon when iconUrl is absent', () => {
    const grid: AgentPickerItem[] = [
      { id: 'claude-native', name: 'Claude', installed: true },
    ];
    render(
      <AgentPicker
        status="ready"
        defaultGrid={grid}
        onSelect={() => undefined}
        customLaunchValue=""
        onCustomLaunchChange={() => undefined}
      />,
    );

    const card = screen.getByTestId('agent-card-claude-native');
    expect(card.querySelector('img')).toBeNull();
    expect(card.querySelector('.lucide-user')).not.toBeNull();
  });

  it('hides More toggle when moreAgents is empty', () => {
    render(
      <AgentPicker
        status="ready"
        defaultGrid={[
          { id: 'claude-native', name: 'Claude', installed: true },
        ]}
        onSelect={() => undefined}
        customLaunchValue=""
        onCustomLaunchChange={() => undefined}
      />,
    );
    expect(screen.queryByTestId('agent-picker-more')).toBeNull();
    expect(screen.queryByTestId('agent-picker-grid-rest')).toBeNull();
  });

  it('reveals moreAgents on More and hides on Fewer (aria-expanded / aria-controls)', async () => {
    const user = userEvent.setup();
    render(
      <AgentPicker
        status="ready"
        defaultGrid={[
          { id: 'claude-native', name: 'Claude', installed: true },
        ]}
        moreAgents={[
          { id: 'custom-tool', name: 'Custom', installed: false },
        ]}
        onSelect={() => undefined}
        customLaunchValue=""
        onCustomLaunchChange={() => undefined}
      />,
    );

    const moreBtn = screen.getByTestId('agent-picker-more');
    expect(moreBtn).toHaveTextContent('More agents');
    expect(moreBtn).toHaveAttribute('aria-expanded', 'false');
    expect(moreBtn).toHaveAttribute('aria-controls', 'agent-picker-rest');
    expect(screen.queryByTestId('agent-picker-grid-rest')).toBeNull();
    expect(screen.queryByTestId('agent-card-custom-tool')).toBeNull();

    await user.click(moreBtn);

    const fewerBtn = screen.getByTestId('agent-picker-more');
    expect(fewerBtn).toHaveTextContent('Fewer agents');
    expect(fewerBtn).toHaveAttribute('aria-expanded', 'true');
    expect(screen.getByTestId('agent-picker-grid-rest')).toBeInTheDocument();
    expect(screen.getByTestId('agent-card-custom-tool')).toBeInTheDocument();

    await user.click(fewerBtn);

    expect(screen.getByTestId('agent-picker-more')).toHaveTextContent('More agents');
    expect(screen.getByTestId('agent-picker-more')).toHaveAttribute('aria-expanded', 'false');
    expect(screen.queryByTestId('agent-picker-grid-rest')).toBeNull();
  });

  it('uses openExternalUrl when desktop prop is provided (AC-P1-2)', async () => {
    const user = userEvent.setup();
    const openExternalUrl = vi.fn().mockResolvedValue(undefined);
    render(
      <AgentPicker
        status="ready"
        defaultGrid={DEFAULT_GRID}
        onSelect={() => undefined}
        customLaunchValue=""
        onCustomLaunchChange={() => undefined}
        desktop={{ openExternalUrl }}
      />,
    );

    const installLinks = screen.getAllByRole('link', { name: /Install/i });
    await user.click(installLinks[0]);
    expect(openExternalUrl).toHaveBeenCalledWith('https://example.com/install');
    expect(installLinks[0]).not.toHaveAttribute('target');
  });

  it('keeps target=_blank when desktop prop is omitted (AC-P1-8)', () => {
    render(
      <AgentPicker
        status="ready"
        defaultGrid={DEFAULT_GRID}
        onSelect={() => undefined}
        customLaunchValue=""
        onCustomLaunchChange={() => undefined}
      />,
    );

    const installLinks = screen.getAllByRole('link', { name: /Install/i });
    expect(installLinks[0]).toHaveAttribute('target', '_blank');
    expect(installLinks[0]).toHaveAttribute('rel', 'noopener noreferrer');
  });

  it('calls onExternalUrlError when openExternalUrl rejects (QC3 W001)', async () => {
    const user = userEvent.setup();
    const openExternalUrl = vi.fn().mockRejectedValue(new Error('boom'));
    const onExternalUrlError = vi.fn();
    render(
      <AgentPicker
        status="ready"
        defaultGrid={DEFAULT_GRID}
        onSelect={() => undefined}
        customLaunchValue=""
        onCustomLaunchChange={() => undefined}
        desktop={{ openExternalUrl }}
        onExternalUrlError={onExternalUrlError}
      />,
    );

    const installLinks = screen.getAllByRole('link', { name: /Install/i });
    await user.click(installLinks[0]);
    expect(openExternalUrl).toHaveBeenCalledWith('https://example.com/install');
    // The rejection is surfaced via the callback so the host can toast (AD-P1-2).
    await vi.waitFor(() => {
      expect(onExternalUrlError).toHaveBeenCalledTimes(1);
    });
  });
});
