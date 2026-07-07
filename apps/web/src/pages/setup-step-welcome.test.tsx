import { http, HttpResponse } from 'msw';
import { describe, expect, it, vi } from 'vitest';
import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import { SetupStepWelcome } from '@/pages/setup-step-welcome';
import { renderInApp } from '@/test/test-providers';
import type { DesktopCapabilities } from '@/lib/nexus/desktop-capabilities';

function makeDesktop(overrides: Partial<DesktopCapabilities> = {}): DesktopCapabilities {
  return {
    openWith: () => Promise.resolve(),
    revealInFinder: () => Promise.resolve(),
    getDaemonStatus: () => Promise.resolve({ state: 'running', port: 8420 }),
    onDaemonStatusChanged: () => Promise.resolve(() => {}),
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

describe('SetupStepWelcome', () => {
  it('uses the hardcoded fallback in browser mode and hides the picker', () => {
    const onChange = vi.fn();
    const onNext = vi.fn();

    renderInApp(
      <SetupStepWelcome state={{ workspaceRoot: '' }} onChange={onChange} onNext={onNext} />,
      { initialRouterEntries: ['/setup'] },
    );

    expect(screen.getByText('~/Documents/nexus/default')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Browse…' })).not.toBeInTheDocument();
  });

  it('shows the desktop workspace root and a picker button', async () => {
    const onChange = vi.fn();
    const onNext = vi.fn();

    renderInApp(
      <SetupStepWelcome state={{ workspaceRoot: '' }} onChange={onChange} onNext={onNext} />,
      {
        desktop: makeDesktop({ getWorkspaceRoot: () => Promise.resolve('/custom/nexus') }),
        initialRouterEntries: ['/setup'],
      },
    );

    await waitFor(() =>
      expect(screen.getByText('/custom/nexus')).toBeInTheDocument(),
    );
    expect(screen.getByRole('button', { name: 'Browse…' })).toBeInTheDocument();
  });

  it('updates the workspace root when the picker returns a directory', async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    const onNext = vi.fn();
    const pickDirectory = vi.fn(() => Promise.resolve('/picked/workspace'));

    renderInApp(
      <SetupStepWelcome state={{ workspaceRoot: '' }} onChange={onChange} onNext={onNext} />,
      {
        desktop: makeDesktop({ getWorkspaceRoot: () => Promise.resolve('/custom/nexus'), pickDirectory }),
        initialRouterEntries: ['/setup'],
      },
    );

    await waitFor(() => expect(screen.getByRole('button', { name: 'Browse…' })).toBeEnabled());
    await user.click(screen.getByRole('button', { name: 'Browse…' }));

    await waitFor(() => expect(pickDirectory).toHaveBeenCalledWith('/custom/nexus'));
    await waitFor(() =>
      expect(onChange).toHaveBeenCalledWith({ workspaceRoot: '/picked/workspace' }),
    );
  });

  it('writes the workspace path on Continue when the path is stale', async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    const onNext = vi.fn();
    const setWorkspacePath = vi.fn(() => Promise.resolve());

    renderInApp(
      <SetupStepWelcome state={{ workspaceRoot: '/Users/x/Documents/nexus42/default' }} onChange={onChange} onNext={onNext} />,
      {
        desktop: makeDesktop({ setWorkspacePath }),
        initialRouterEntries: ['/setup'],
      },
    );

    await waitFor(() => expect(screen.getByRole('button', { name: 'Continue' })).toBeEnabled());
    await user.click(screen.getByRole('button', { name: 'Continue' }));

    await waitFor(() => expect(setWorkspacePath).toHaveBeenCalledWith('/Users/x/Documents/nexus42/default'));
    await waitFor(() => expect(onNext).toHaveBeenCalled());
  });

  it('does not write the workspace path when it is a custom non-stale path', async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    const onNext = vi.fn();
    const setWorkspacePath = vi.fn(() => Promise.resolve());

    renderInApp(
      <SetupStepWelcome state={{ workspaceRoot: '/Users/x/MyCreative/Nexus' }} onChange={onChange} onNext={onNext} />,
      {
        desktop: makeDesktop({ setWorkspacePath }),
        initialRouterEntries: ['/setup'],
      },
    );

    await waitFor(() => expect(screen.getByRole('button', { name: 'Continue' })).toBeEnabled());
    await user.click(screen.getByRole('button', { name: 'Continue' }));

    await waitFor(() => expect(onNext).toHaveBeenCalled());
    expect(setWorkspacePath).not.toHaveBeenCalled();
  });
});
