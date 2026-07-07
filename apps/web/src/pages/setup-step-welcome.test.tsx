import { useCallback, useState } from 'react';
import { describe, expect, it, vi } from 'vitest';
import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import { SetupStepWelcome } from '@/pages/setup-step-welcome';
import { renderInApp } from '@/test/test-providers';
import type { DesktopCapabilities } from '@/lib/nexus/desktop-capabilities';
import type { WizardState } from '@/pages/setup-wizard-page';

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

function makeState(overrides: Partial<WizardState> = {}): WizardState {
  return {
    workspaceRoot: '',
    selectedAgent: null,
    customLaunchCommand: '',
    ...overrides,
  };
}

interface HarnessProps {
  initial: WizardState;
  onNext?: () => void;
}

function Harness({ initial, onNext = vi.fn() }: HarnessProps) {
  const [state, setState] = useState<WizardState>(initial);
  const onChange = useCallback((next: WizardState) => setState(next), []);
  return (
    <SetupStepWelcome
      state={state}
      onChange={onChange}
      onNext={onNext}
    />
  );
}

function renderHarness(
  initial: WizardState,
  options: { desktop?: DesktopCapabilities; onNext?: () => void } = {},
) {
  return renderInApp(<Harness initial={initial} onNext={options.onNext} />, {
    desktop: options.desktop,
    initialRouterEntries: ['/setup'],
  });
}

describe('SetupStepWelcome', () => {
  it('uses the hardcoded fallback in browser mode and hides the picker', async () => {
    renderHarness(makeState());

    await waitFor(() => expect(screen.getByText('~/Documents/nexus/default')).toBeInTheDocument());
    expect(screen.queryByRole('button', { name: 'Browse…' })).not.toBeInTheDocument();
  });

  it('shows the desktop workspace root and a picker button', async () => {
    renderHarness(makeState(), {
      desktop: makeDesktop({ getWorkspaceRoot: () => Promise.resolve('/custom/nexus') }),
    });

    await waitFor(() => expect(screen.getByText('/custom/nexus')).toBeInTheDocument());
    expect(screen.getByRole('button', { name: 'Browse…' })).toBeInTheDocument();
  });

  it('updates the workspace root when the picker returns a directory', async () => {
    const user = userEvent.setup();
    const pickDirectory = vi.fn(() => Promise.resolve('/picked/workspace'));

    renderHarness(makeState(), {
      desktop: makeDesktop({ getWorkspaceRoot: () => Promise.resolve('/custom/nexus'), pickDirectory }),
    });

    await waitFor(() => expect(screen.getByRole('button', { name: 'Browse…' })).toBeEnabled());
    await user.click(screen.getByRole('button', { name: 'Browse…' }));

    await waitFor(() => expect(pickDirectory).toHaveBeenCalledWith('/custom/nexus'));
    await waitFor(() =>
      expect(screen.getByText('/picked/workspace')).toBeInTheDocument(),
    );
  });

  it('writes the workspace path on Continue when the path is stale', async () => {
    const user = userEvent.setup();
    const onNext = vi.fn();
    const setWorkspacePath = vi.fn(() => Promise.resolve());
    const stalePath = '/Users/x/Documents/nexus42/default';

    renderHarness(makeState({ workspaceRoot: stalePath }), {
      desktop: makeDesktop({ getWorkspaceRoot: () => Promise.resolve(stalePath), setWorkspacePath }),
      onNext,
    });

    await waitFor(() => expect(screen.getByRole('button', { name: 'Continue' })).toBeEnabled());
    await user.click(screen.getByRole('button', { name: 'Continue' }));

    await waitFor(() => expect(setWorkspacePath).toHaveBeenCalledWith(stalePath));
    await waitFor(() => expect(onNext).toHaveBeenCalled());
  });

  it('does not write the workspace path when it is a custom non-stale path', async () => {
    const user = userEvent.setup();
    const onNext = vi.fn();
    const setWorkspacePath = vi.fn(() => Promise.resolve());
    const customPath = '/Users/x/MyCreative/Nexus';

    renderHarness(makeState({ workspaceRoot: customPath }), {
      desktop: makeDesktop({ getWorkspaceRoot: () => Promise.resolve(customPath), setWorkspacePath }),
      onNext,
    });

    await waitFor(() => expect(screen.getByRole('button', { name: 'Continue' })).toBeEnabled());
    await user.click(screen.getByRole('button', { name: 'Continue' }));

    await waitFor(() => expect(onNext).toHaveBeenCalled());
    expect(setWorkspacePath).not.toHaveBeenCalled();
  });

  it('surfaces a setWorkspacePath error and stays on the step', async () => {
    const user = userEvent.setup();
    const onNext = vi.fn();
    const setWorkspacePath = vi.fn(() => Promise.reject(new Error('permission denied')));
    const stalePath = '/Users/x/Documents/nexus42/default';

    renderHarness(makeState({ workspaceRoot: stalePath }), {
      desktop: makeDesktop({ getWorkspaceRoot: () => Promise.resolve(stalePath), setWorkspacePath }),
      onNext,
    });

    await waitFor(() => expect(screen.getByRole('button', { name: 'Continue' })).toBeEnabled());
    await user.click(screen.getByRole('button', { name: 'Continue' }));

    await waitFor(() => expect(screen.getByText('permission denied')).toBeInTheDocument());
    expect(onNext).not.toHaveBeenCalled();
  });

  it('surfaces a pickDirectory error and stays on the step', async () => {
    const user = userEvent.setup();
    const pickDirectory = vi.fn(() => Promise.reject(new Error('dialog failed')));

    renderHarness(makeState(), {
      desktop: makeDesktop({ getWorkspaceRoot: () => Promise.resolve('/custom/nexus'), pickDirectory }),
    });

    await waitFor(() => expect(screen.getByRole('button', { name: 'Browse…' })).toBeEnabled());
    await user.click(screen.getByRole('button', { name: 'Browse…' }));

    await waitFor(() => expect(screen.getByText('dialog failed')).toBeInTheDocument());
  });

  it('clears the previous error when the user retries Continue', async () => {
    const user = userEvent.setup();
    const onNext = vi.fn();
    const setWorkspacePath = vi
      .fn()
      .mockRejectedValueOnce(new Error('permission denied'))
      .mockResolvedValueOnce(undefined);
    const stalePath = '/Users/x/Documents/nexus42/default';

    renderHarness(makeState({ workspaceRoot: stalePath }), {
      desktop: makeDesktop({ getWorkspaceRoot: () => Promise.resolve(stalePath), setWorkspacePath }),
      onNext,
    });

    await waitFor(() => expect(screen.getByRole('button', { name: 'Continue' })).toBeEnabled());
    await user.click(screen.getByRole('button', { name: 'Continue' }));
    await waitFor(() => expect(screen.getByText('permission denied')).toBeInTheDocument());

    await user.click(screen.getByRole('button', { name: 'Continue' }));
    await waitFor(() => expect(onNext).toHaveBeenCalled());
    expect(screen.queryByText('permission denied')).not.toBeInTheDocument();
  });
});
