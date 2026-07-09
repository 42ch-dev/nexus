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
    ensureSetupBootstrap: () => Promise.resolve({ creator_id: 'ctr_local1234567890ab', already_bootstrapped: false }),
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

  it('renders Continue as a wide prominent bottom CTA', async () => {
    renderHarness(makeState({ workspaceRoot: '/custom/nexus' }));

    const continueButton = await waitFor(() => screen.getByRole('button', { name: 'Continue' }));
    expect(continueButton).toHaveClass('w-full', 'max-w-setup-wizard-surface-cta-primary-max-width');
  });

  it('truncates a long workspace path inside a shrinkable input row', async () => {
    const longPath = '/very/long/path/'.repeat(10);
    renderHarness(makeState({ workspaceRoot: longPath }), {
      desktop: makeDesktop({ getWorkspaceRoot: () => Promise.resolve(longPath) }),
    });

    await waitFor(() => expect(screen.getByText(longPath)).toBeInTheDocument());
    const pathText = screen.getByText(longPath);
    const pathContainer = pathText.parentElement;

    // Truncation requires overflow:hidden + text-overflow:ellipsis + white-space:nowrap.
    expect(pathText).toHaveClass('truncate');
    // The flex child must be allowed to shrink below its intrinsic width so the
    // long path does not push the row past the right edge of the card.
    expect(pathContainer).toHaveClass('min-w-0');
    expect(pathContainer).toHaveClass('flex-1');
    expect(pathContainer?.parentElement).toHaveAttribute('data-testid', 'workspace-location-row');

    // The Browse button keeps its intrinsic width so the path container absorbs
    // all available horizontal space in the row.
    const browseButton = screen.getByRole('button', { name: 'Browse…' });
    expect(browseButton).toHaveClass('flex-shrink-0');
  });

  it('shows the desktop workspace root and a picker button in the same row', async () => {
    renderHarness(makeState(), {
      desktop: makeDesktop({ getWorkspaceRoot: () => Promise.resolve('/custom/nexus') }),
    });

    await waitFor(() => expect(screen.getByText('/custom/nexus')).toBeInTheDocument());
    const browseButton = screen.getByRole('button', { name: 'Browse…' });
    expect(browseButton).toBeInTheDocument();
    expect(browseButton.closest('[data-testid="workspace-location-row"]')).toBeInTheDocument();
    expect(screen.getByText('/custom/nexus').parentElement).toHaveClass('min-w-0');
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

  it('allows retry after a setWorkspacePath error and continues on success', async () => {
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
  });

  it('calls ensureSetupBootstrap after workspace persist and advances on success', async () => {
    const user = userEvent.setup();
    const onNext = vi.fn();
    const setWorkspacePath = vi.fn(() => Promise.resolve());
    const ensureSetupBootstrap = vi.fn(() =>
      Promise.resolve({ creator_id: 'ctr_local1234567890ab', already_bootstrapped: false }),
    );
    const stalePath = '/Users/x/Documents/nexus42/default';

    renderHarness(makeState({ workspaceRoot: stalePath }), {
      desktop: makeDesktop({
        getWorkspaceRoot: () => Promise.resolve(stalePath),
        setWorkspacePath,
        ensureSetupBootstrap,
      }),
      onNext,
    });

    await waitFor(() => expect(screen.getByRole('button', { name: 'Continue' })).toBeEnabled());
    await user.click(screen.getByRole('button', { name: 'Continue' }));

    await waitFor(() => expect(setWorkspacePath).toHaveBeenCalledWith(stalePath));
    await waitFor(() => expect(ensureSetupBootstrap).toHaveBeenCalled());
    await waitFor(() => expect(onNext).toHaveBeenCalled());
    // Bootstrap runs after workspace persist: verify call order.
    const setCallOrder = setWorkspacePath.mock.invocationCallOrder[0];
    const bootstrapCallOrder = ensureSetupBootstrap.mock.invocationCallOrder[0];
    expect(setCallOrder).toBeLessThan(bootstrapCallOrder);
  });

  it('surfaces bootstrap error and stays on the welcome step', async () => {
    const user = userEvent.setup();
    const onNext = vi.fn();
    const ensureSetupBootstrap = vi.fn(() => Promise.reject(new Error('config write failed')));

    renderHarness(makeState({ workspaceRoot: '/custom/nexus' }), {
      desktop: makeDesktop({ ensureSetupBootstrap }),
      onNext,
    });

    await waitFor(() => expect(screen.getByRole('button', { name: 'Continue' })).toBeEnabled());
    await user.click(screen.getByRole('button', { name: 'Continue' }));

    await waitFor(() => expect(screen.getByText(/config write failed/)).toBeInTheDocument());
    await waitFor(() => expect(screen.getByText(/Try again or reset/)).toBeInTheDocument());
    expect(onNext).not.toHaveBeenCalled();
  });

  it('retries bootstrap after failure and advances on success', async () => {
    const user = userEvent.setup();
    const onNext = vi.fn();
    const ensureSetupBootstrap = vi
      .fn()
      .mockRejectedValueOnce(new Error('config write failed'))
      .mockResolvedValueOnce({ creator_id: 'ctr_local1234567890ab', already_bootstrapped: false });

    renderHarness(makeState({ workspaceRoot: '/custom/nexus' }), {
      desktop: makeDesktop({ ensureSetupBootstrap }),
      onNext,
    });

    await waitFor(() => expect(screen.getByRole('button', { name: 'Continue' })).toBeEnabled());
    await user.click(screen.getByRole('button', { name: 'Continue' }));
    await waitFor(() => expect(screen.getByText(/config write failed/)).toBeInTheDocument());

    // After error, loading resets and Continue is re-enabled.
    await waitFor(() => expect(screen.getByRole('button', { name: 'Continue' })).toBeEnabled());
    await user.click(screen.getByRole('button', { name: 'Continue' }));
    await waitFor(() => expect(onNext).toHaveBeenCalled());
  });

  it('skips bootstrap in browser mode and advances directly', async () => {
    const user = userEvent.setup();
    const onNext = vi.fn();

    renderHarness(makeState({ workspaceRoot: '/tmp/nexus' }), { onNext });

    await waitFor(() => expect(screen.getByRole('button', { name: 'Continue' })).toBeEnabled());
    await user.click(screen.getByRole('button', { name: 'Continue' }));
    await waitFor(() => expect(onNext).toHaveBeenCalled());
  });
});
