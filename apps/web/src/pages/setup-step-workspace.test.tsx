import { useCallback, useState } from 'react';
import { describe, expect, it, vi } from 'vitest';
import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import { SetupStepWorkspace } from '@/pages/setup-step-workspace';
import { renderInApp } from '@/test/test-providers';
import type { DesktopCapabilities } from '@/lib/nexus/desktop-capabilities';
import type { WizardState } from '@/pages/setup-wizard-page';
import type { CreatorDetail } from '@42ch/nexus-contracts';
import type { NexusClient } from '@/lib/nexus';

function makeClient(overrides: Partial<Pick<NexusClient, 'updateCreator'>> = {}): NexusClient {
  const updateCreator = vi.fn(() =>
    Promise.resolve({ creator_id: 'ctr_local1234567890ab', display_name: 'Test Profile' } as unknown as CreatorDetail),
  ) as unknown as NexusClient['updateCreator'];
  return {
    updateCreator: overrides.updateCreator ?? updateCreator,
  } as unknown as NexusClient;
}

function mockUpdateCreator(fn: () => Promise<unknown> = () => Promise.resolve({ creator_id: 'ctr_local1234567890ab' })): NexusClient['updateCreator'] {
  return vi.fn(fn) as unknown as NexusClient['updateCreator'];
}

function makeDesktop(overrides: Partial<DesktopCapabilities> = {}): DesktopCapabilities {
  return {
    openWith: () => Promise.resolve(),
    openExternalUrl: () => Promise.resolve(),
    revealInFinder: () => Promise.resolve(),
    getDaemonStatus: () => Promise.resolve({ state: 'running', port: 8420 }),
    onDaemonStatusChanged: () => Promise.resolve(() => {}),
    startDaemon: () => Promise.resolve(),
    stopDaemon: () => Promise.resolve(),
    resetLocalDatabase: () => Promise.resolve(),
    getSetupCompleted: () => Promise.resolve(false),
    setSetupCompleted: () => Promise.resolve(),
    setAgentProfile: () => Promise.resolve(),
    getAgentProfile: () => Promise.resolve(null),
    getWorkspaceRoot: () => Promise.resolve('/tmp/nexus'),
    pickDirectory: () => Promise.resolve(null),
    setWorkspacePath: () => Promise.resolve(),
    ensureSetupBootstrap: () => Promise.resolve({ creator_id: 'ctr_local1234567890ab', already_bootstrapped: false }),
    switchActiveCreator: () => Promise.resolve('/tmp/nexus'),
    ...overrides,
  };
}

function makeState(overrides: Partial<WizardState> = {}): WizardState {
  return {
    workspaceRoot: '',
    selectedAgent: null,
    customLaunchCommand: '',
    profileDisplayName: 'Test Profile',
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
    <SetupStepWorkspace
      state={state}
      onChange={onChange}
      onNext={onNext}
      onBack={onBack}
    />
  );
}

function renderHarness(
  initial: WizardState,
  options: { client?: NexusClient; desktop?: DesktopCapabilities; onNext?: () => void; onBack?: () => void } = {},
) {
  return renderInApp(
    <Harness initial={initial} onNext={options.onNext} onBack={options.onBack} />,
    {
      client: options.client ?? makeClient(),
      desktop: options.desktop,
      initialRouterEntries: ['/setup'],
    },
  );
}

describe('SetupStepWorkspace', () => {
  it('uses the hardcoded fallback in browser mode and disables the picker', async () => {
    renderHarness(makeState());

    await waitFor(() => expect(screen.getByDisplayValue('~/Documents/nexus/default')).toBeInTheDocument());
    const button = screen.getByRole('button', { name: 'Change Folder…' });
    expect(button).toBeInTheDocument();
    expect(button).toBeDisabled();
  });

  it('renders Name your Profile heading and Back to Agent CTA', async () => {
    const onBack = vi.fn();
    renderHarness(makeState({ workspaceRoot: '/custom/nexus' }), { onBack });

    expect(screen.getByRole('heading', { name: 'Name your Profile' })).toBeInTheDocument();
    const backButton = screen.getByRole('button', { name: 'Back' });
    expect(backButton).toBeInTheDocument();
    expect(backButton).not.toHaveTextContent('Back');

    const user = userEvent.setup();
    await user.click(backButton);
    expect(onBack).toHaveBeenCalled();
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

    await waitFor(() => expect(screen.getByDisplayValue(longPath)).toBeInTheDocument());
    const pathInput = screen.getByDisplayValue(longPath);
    const pathContainer = pathInput.parentElement;

    expect(pathInput).toHaveClass('truncate');
    expect(pathInput).toHaveClass('min-w-0');
    expect(pathInput).toHaveClass('flex-1');
    expect(pathContainer?.parentElement).toHaveAttribute('data-testid', 'workspace-location-row');

    const changeFolderButton = screen.getByRole('button', { name: 'Change Folder…' });
    expect(changeFolderButton).toHaveClass('shrink-0');
  });

  it('shows the desktop workspace root and a picker button in the same row', async () => {
    renderHarness(makeState(), {
      desktop: makeDesktop({ getWorkspaceRoot: () => Promise.resolve('/custom/nexus') }),
    });

    await waitFor(() => expect(screen.getByDisplayValue('/custom/nexus')).toBeInTheDocument());
    const changeFolderButton = screen.getByRole('button', { name: 'Change Folder…' });
    expect(changeFolderButton).toBeInTheDocument();
    expect(changeFolderButton.closest('[data-testid="workspace-location-row"]')).toBeInTheDocument();
  });

  it('updates the workspace root when the picker returns a directory', async () => {
    const user = userEvent.setup();
    const pickDirectory = vi.fn(() => Promise.resolve('/picked/workspace'));

    renderHarness(makeState(), {
      desktop: makeDesktop({ getWorkspaceRoot: () => Promise.resolve('/custom/nexus'), pickDirectory }),
    });

    await waitFor(() => expect(screen.getByRole('button', { name: 'Change Folder…' })).toBeEnabled());
    await user.click(screen.getByRole('button', { name: 'Change Folder…' }));

    await waitFor(() => expect(pickDirectory).toHaveBeenCalledWith('/custom/nexus'));
    await waitFor(() =>
      expect(screen.getByDisplayValue('/picked/workspace')).toBeInTheDocument(),
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

    await waitFor(() => expect(screen.getByRole('button', { name: 'Change Folder…' })).toBeEnabled());
    await user.click(screen.getByRole('button', { name: 'Change Folder…' }));

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
    const setCallOrder = setWorkspacePath.mock.invocationCallOrder[0];
    const bootstrapCallOrder = ensureSetupBootstrap.mock.invocationCallOrder[0];
    expect(setCallOrder).toBeLessThan(bootstrapCallOrder);
  });

  it('surfaces bootstrap error and stays on the workspace step', async () => {
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
    await waitFor(() =>
      expect(
        screen.getByText(/Retry Continue, or use Reset local database below if the problem persists/),
      ).toBeInTheDocument(),
    );
    expect(screen.getByRole('button', { name: 'Reset local database' })).toBeInTheDocument();
    expect(onNext).not.toHaveBeenCalled();
  });

  it('reset local database after bootstrap failure reloads without startDaemon', async () => {
    const user = userEvent.setup();
    const onNext = vi.fn();
    const ensureSetupBootstrap = vi.fn(() => Promise.reject(new Error('config write failed')));
    const resetLocalDatabase = vi.fn(() => Promise.resolve());
    const startDaemon = vi.fn(() => Promise.resolve());
    const reloadSpy = vi.fn();
    Object.defineProperty(window, 'location', {
      value: { ...window.location, reload: reloadSpy },
      writable: true,
    });

    renderHarness(makeState({ workspaceRoot: '/custom/nexus' }), {
      desktop: makeDesktop({ ensureSetupBootstrap, resetLocalDatabase, startDaemon }),
      onNext,
    });

    await waitFor(() => expect(screen.getByRole('button', { name: 'Continue' })).toBeEnabled());
    await user.click(screen.getByRole('button', { name: 'Continue' }));
    await waitFor(() =>
      expect(screen.getByRole('button', { name: 'Reset local database' })).toBeInTheDocument(),
    );

    await user.click(screen.getByRole('button', { name: 'Reset local database' }));
    await waitFor(() => expect(resetLocalDatabase).toHaveBeenCalled());
    expect(startDaemon).not.toHaveBeenCalled();
    expect(reloadSpy).toHaveBeenCalled();
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
    expect(screen.getByRole('button', { name: 'Reset local database' })).toBeInTheDocument();

    await waitFor(() => expect(screen.getByRole('button', { name: 'Continue' })).toBeEnabled());
    await user.click(screen.getByRole('button', { name: 'Continue' }));
    await waitFor(() => expect(onNext).toHaveBeenCalled());
    expect(screen.queryByRole('button', { name: 'Reset local database' })).not.toBeInTheDocument();
  });

  it('renders the Profile name field', async () => {
    renderHarness(makeState({ workspaceRoot: '/custom/nexus' }), {
      desktop: makeDesktop({ getWorkspaceRoot: () => Promise.resolve('/custom/nexus') }),
    });

    await waitFor(() => expect(screen.getByTestId('wizard-profile-name')).toBeInTheDocument());
    expect(screen.getByLabelText('Profile name')).toBeInTheDocument();
  });

  it('updates the Profile name as the author types', async () => {
    const user = userEvent.setup();
    renderHarness(makeState({ workspaceRoot: '/custom/nexus' }), {
      desktop: makeDesktop({ getWorkspaceRoot: () => Promise.resolve('/custom/nexus') }),
    });

    const input = await waitFor(() => screen.getByTestId('wizard-profile-name'));
    await user.clear(input);
    await user.type(input, 'Alice');
    expect(input).toHaveValue('Alice');
  });

  it('persists the Profile display name after bootstrap', async () => {
    const user = userEvent.setup();
    const onNext = vi.fn();
    const ensureSetupBootstrap = vi.fn(() =>
      Promise.resolve({ creator_id: 'ctr_local1234567890ab', already_bootstrapped: false }),
    );
    const updateCreator = mockUpdateCreator();

    renderHarness(makeState({ workspaceRoot: '/custom/nexus', profileDisplayName: 'Alice' }), {
      desktop: makeDesktop({
        getWorkspaceRoot: () => Promise.resolve('/custom/nexus'),
        ensureSetupBootstrap,
      }),
      client: makeClient({ updateCreator }),
      onNext,
    });

    await waitFor(() => expect(screen.getByRole('button', { name: 'Continue' })).toBeEnabled());
    await user.click(screen.getByRole('button', { name: 'Continue' }));

    await waitFor(() => expect(ensureSetupBootstrap).toHaveBeenCalled());
    await waitFor(() =>
      expect(updateCreator).toHaveBeenCalledWith('ctr_local1234567890ab', { display_name: 'Alice' }),
    );
    await waitFor(() => expect(onNext).toHaveBeenCalled());
  });

  it('blocks Continue when the Profile name is empty', async () => {
    const onNext = vi.fn();
    const updateCreator = mockUpdateCreator();

    renderHarness(makeState({ workspaceRoot: '/custom/nexus', profileDisplayName: '' }), {
      desktop: makeDesktop({ getWorkspaceRoot: () => Promise.resolve('/custom/nexus') }),
      client: makeClient({ updateCreator }),
      onNext,
    });

    await waitFor(() =>
      expect(screen.getByRole('button', { name: 'Continue' })).toBeDisabled(),
    );
    expect(updateCreator).not.toHaveBeenCalled();
    expect(onNext).not.toHaveBeenCalled();
  });

  it('surfaces an updateCreator error and stays on the step', async () => {
    const user = userEvent.setup();
    const onNext = vi.fn();
    const updateCreator = mockUpdateCreator(() => Promise.reject(new Error('display name conflict')));

    renderHarness(makeState({ workspaceRoot: '/custom/nexus', profileDisplayName: 'Alice' }), {
      desktop: makeDesktop({ getWorkspaceRoot: () => Promise.resolve('/custom/nexus') }),
      client: makeClient({ updateCreator }),
      onNext,
    });

    await waitFor(() => expect(screen.getByRole('button', { name: 'Continue' })).toBeEnabled());
    await user.click(screen.getByRole('button', { name: 'Continue' }));

    await waitFor(() => expect(screen.getByText('display name conflict')).toBeInTheDocument());
    expect(onNext).not.toHaveBeenCalled();
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
