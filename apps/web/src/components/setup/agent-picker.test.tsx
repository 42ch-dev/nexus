/**
 * AgentPicker presentational unit tests (V1.101 Task 2 + fix-wave B2).
 * Wiring/persistence coverage belongs to Task 3 / setup-step-agent tests.
 */
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

import {
  AgentPicker,
  type AgentPickerItem,
} from '@/components/setup/agent-picker';

const AGENTS: AgentPickerItem[] = [
  {
    id: 'claude-acp',
    name: 'Claude Code',
    version: '1.0.0',
    installed: true,
    installUrl: 'https://example.com/install',
    docsUrl: 'https://example.com/docs',
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
    expect(screen.getByText('Scanning for local ACP agents…')).toBeInTheDocument();
    expect(screen.getByTestId('agent-picker')).toHaveAttribute('data-status', 'loading');
  });

  it('selects installed agents only', async () => {
    const user = userEvent.setup();
    const onSelect = vi.fn();
    render(
      <AgentPicker
        status="ready"
        agents={AGENTS}
        selectedId={null}
        onSelect={onSelect}
        customLaunchValue=""
        onCustomLaunchChange={() => undefined}
      />,
    );

    await user.click(screen.getByTestId('agent-card-select-claude-acp'));
    expect(onSelect).toHaveBeenCalledWith('claude-acp');

    // 'missing' is non-common — expand the More toggle to verify it renders
    // without a select button (not-installed cards are discoverability-only).
    await user.click(screen.getByTestId('agent-picker-more'));
    expect(screen.queryByTestId('agent-card-select-missing')).toBeNull();
    expect(screen.getByTestId('agent-card-missing').tagName).toBe('DIV');
  });

  it('keeps Install/Docs links outside the select button (B2)', () => {
    render(
      <AgentPicker
        status="ready"
        agents={AGENTS}
        onSelect={() => undefined}
        customLaunchValue=""
        onCustomLaunchChange={() => undefined}
      />,
    );
    const select = screen.getByTestId('agent-card-select-claude-acp');
    expect(select.querySelector('a')).toBeNull();
    const card = screen.getByTestId('agent-card-claude-acp');
    expect(card.querySelector('a[href="https://example.com/install"]')).not.toBeNull();
    expect(card.querySelector('a[href="https://example.com/docs"]')).not.toBeNull();
  });

  it('hides install/docs when URLs missing and shows when present', async () => {
    const user = userEvent.setup();
    render(
      <AgentPicker
        status="ready"
        agents={AGENTS}
        onSelect={() => undefined}
        customLaunchValue=""
        onCustomLaunchChange={() => undefined}
      />,
    );
    expect(screen.getByRole('link', { name: /Install/i })).toHaveAttribute(
      'href',
      'https://example.com/install',
    );
    expect(screen.getByRole('link', { name: /Docs/i })).toHaveAttribute(
      'href',
      'https://example.com/docs',
    );
    // 'missing' (no URLs) is non-common — expand to verify no links render.
    await user.click(screen.getByTestId('agent-picker-more'));
    expect(screen.getByTestId('agent-card-missing').querySelector('a')).toBeNull();
  });

  it('shows soft Installed Badge beside title and mutes not-installed title', async () => {
    const user = userEvent.setup();
    render(
      <AgentPicker
        status="ready"
        agents={AGENTS}
        selectedId={null}
        onSelect={() => undefined}
        customLaunchValue=""
        onCustomLaunchChange={() => undefined}
      />,
    );
    expect(screen.getByTestId('agent-card-installed-badge-claude-acp')).toHaveTextContent(
      'Installed',
    );
    // 'Missing Agent' is non-common — expand to verify its muted title + badge.
    await user.click(screen.getByTestId('agent-picker-more'));
    expect(screen.getByText('Missing Agent')).toHaveClass('text-gray-700');
    expect(screen.getByText('Not installed')).toBeInTheDocument();
  });

  it('uses hollow dot when installed-unselected and lit when selected', async () => {
    const user = userEvent.setup();
    const { rerender } = render(
      <AgentPicker
        status="ready"
        agents={AGENTS}
        selectedId={null}
        onSelect={() => undefined}
        customLaunchValue=""
        onCustomLaunchChange={() => undefined}
      />,
    );
    const installedCard = screen.getByTestId('agent-card-claude-acp');
    expect(installedCard.querySelector('[data-dot="hollow"]')).not.toBeNull();

    // Expand More so the non-common 'missing' card is visible after rerender.
    await user.click(screen.getByTestId('agent-picker-more'));

    rerender(
      <AgentPicker
        status="ready"
        agents={AGENTS}
        selectedId="claude-acp"
        onSelect={() => undefined}
        customLaunchValue=""
        onCustomLaunchChange={() => undefined}
      />,
    );
    expect(
      screen.getByTestId('agent-card-claude-acp').querySelector('[data-dot="lit"]'),
    ).not.toBeNull();
    expect(
      screen.getByTestId('agent-card-missing').querySelector('[data-dot="muted"]'),
    ).not.toBeNull();
  });

  it('uses hollow GRAY border for unselected installed dot (FB-UI-006)', () => {
    render(
      <AgentPicker
        status="ready"
        agents={AGENTS}
        selectedId={null}
        onSelect={() => undefined}
        customLaunchValue=""
        onCustomLaunchChange={() => undefined}
      />,
    );
    // Unselected installed: hollow gray border, NOT green.
    const unselectedDot = screen
      .getByTestId('agent-card-claude-acp')
      .querySelector('[data-dot="hollow"] span');
    expect(unselectedDot).toHaveClass('border-gray-500');
    expect(unselectedDot).not.toHaveClass('border-green-700');
  });

  it('uses filled GREEN when selected (FB-UI-006)', () => {
    render(
      <AgentPicker
        status="ready"
        agents={AGENTS}
        selectedId="claude-acp"
        onSelect={() => undefined}
        customLaunchValue=""
        onCustomLaunchChange={() => undefined}
      />,
    );
    const selectedDot = screen
      .getByTestId('agent-card-claude-acp')
      .querySelector('[data-dot="lit"] span');
    expect(selectedDot).toHaveClass('bg-green-700');
  });

  it('renders an ArrowUpRight icon inside outbound links', () => {
    render(
      <AgentPicker
        status="ready"
        agents={AGENTS}
        onSelect={() => undefined}
        customLaunchValue=""
        onCustomLaunchChange={() => undefined}
      />,
    );
    const install = screen.getByRole('link', { name: /Install/i });
    expect(install.querySelector('svg')).not.toBeNull();
    expect(install.querySelector('svg')).toHaveClass('lucide-arrow-up-right');
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
        agents={AGENTS}
        selectedId={null}
        onSelect={() => undefined}
        customLaunchValue=""
        onCustomLaunchChange={() => undefined}
      />,
    );
    const card = screen.getByTestId('agent-card-claude-acp');
    expect(card).toHaveClass('hover:bg-gray-alpha-100');

    // Inner button must not carry the hover class.
    const selectBtn = screen.getByTestId('agent-card-select-claude-acp');
    expect(selectBtn).not.toHaveClass('hover:bg-gray-alpha-100');
  });

  it('shows Verify Agent button when onVerify is provided', async () => {
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
    expect(btn).toHaveTextContent('Verify Agent');
    await user.click(btn);
    expect(onVerify).toHaveBeenCalledOnce();
  });

  it('disables Verify Agent when command is empty', () => {
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

  it('hides Verify Agent button when onVerify is omitted', () => {
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
    expect(btn).toHaveTextContent('Verifying…');
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

  it('renders common agents first in locked priority order', () => {
    // Fixture intentionally out-of-order to verify priority sorting.
    const agents: AgentPickerItem[] = [
      { id: 'pi-acp', name: 'Pi', installed: true },
      { id: 'codex-acp', name: 'Codex', installed: true },
      { id: 'claude-acp', name: 'Claude', installed: true },
      { id: 'cursor', name: 'Cursor', installed: true },
      { id: 'unknown-tool', name: 'Mystery', installed: true },
    ];
    render(
      <AgentPicker
        status="ready"
        agents={agents}
        onSelect={() => undefined}
        customLaunchValue=""
        onCustomLaunchChange={() => undefined}
      />,
    );

    const grid = screen.getByTestId('agent-picker-grid');
    const commonCardIds = Array.from(grid.querySelectorAll<HTMLElement>('[data-installed]')).map(
      (card) => card.getAttribute('data-testid'),
    );
    // Priority order: codex-acp(0), claude-acp(1), cursor(2), pi-acp(8).
    expect(commonCardIds).toEqual([
      'agent-card-codex-acp',
      'agent-card-claude-acp',
      'agent-card-cursor',
      'agent-card-pi-acp',
    ]);
    // Non-common agent stays behind More, not in the common grid.
    expect(screen.queryByTestId('agent-card-unknown-tool')).toBeNull();
    expect(screen.getByTestId('agent-picker-more')).toBeInTheDocument();
  });

  it('reveals rest agents on More and hides on Fewer (aria-expanded / aria-controls)', async () => {
    const user = userEvent.setup();
    const agents: AgentPickerItem[] = [
      { id: 'codex-acp', name: 'Codex', installed: true },
      { id: 'custom-tool', name: 'Custom', installed: false },
    ];
    render(
      <AgentPicker
        status="ready"
        agents={agents}
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

  it('hides More toggle when there are no rest agents', () => {
    render(
      <AgentPicker
        status="ready"
        agents={[
          { id: 'codex-acp', name: 'Codex', installed: true },
          { id: 'claude-acp', name: 'Claude', installed: true },
        ]}
        onSelect={() => undefined}
        customLaunchValue=""
        onCustomLaunchChange={() => undefined}
      />,
    );
    expect(screen.queryByTestId('agent-picker-more')).toBeNull();
    expect(screen.queryByTestId('agent-picker-grid-rest')).toBeNull();
  });

  it('renders Verify button and Input fused in one height-aligned row (FB-D4)', () => {
    render(
      <AgentPicker
        status="empty"
        customLaunchValue="/usr/local/bin/agent"
        onCustomLaunchChange={() => undefined}
        onVerify={() => undefined}
        verifyStatus="idle"
      />,
    );

    const verifyBtn = screen.getByTestId('agent-picker-verify');
    const input = screen.getByPlaceholderText('e.g. /usr/local/bin/my-agent');

    // Both controls share one flex row wrapper (no size mismatch).
    expect(verifyBtn.parentElement).toBe(input.parentElement);
    expect(verifyBtn.parentElement).toHaveClass('flex');
    expect(verifyBtn).toHaveClass('h-10');
    expect(input).toHaveClass('h-10');
  });
});
