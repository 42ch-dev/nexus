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

    // Not-installed has no select button.
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

  it('hides install/docs when URLs missing and shows when present', () => {
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
    expect(screen.getByTestId('agent-card-missing').querySelector('a')).toBeNull();
  });

  it('shows soft Installed Badge and mutes not-installed cards', () => {
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
    expect(screen.getByTestId('agent-card-missing')).toHaveClass('opacity-60');
    expect(screen.getByText('Not installed')).toBeInTheDocument();
  });

  it('uses hollow dot when installed-unselected and lit when selected', () => {
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

  it('uses ArrowUpRight outbound icons sized to label cap-height', () => {
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
    const icon = install.querySelector('svg');
    expect(icon).toHaveClass('h-[1em]', 'w-[1em]');
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
});
