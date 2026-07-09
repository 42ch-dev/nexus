/**
 * AgentPicker presentational unit tests (V1.101 Task 2).
 * Wiring/persistence coverage belongs to Task 3.
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
    id: 'claude-code',
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

    await user.click(screen.getByTestId('agent-card-claude-code'));
    expect(onSelect).toHaveBeenCalledWith('claude-code');

    // Not-installed is a div, not a button — click should not select.
    expect(screen.getByTestId('agent-card-missing').tagName).toBe('DIV');
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
