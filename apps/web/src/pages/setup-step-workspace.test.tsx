import { useCallback, useState } from 'react';
import { describe, expect, it, vi } from 'vitest';
import { screen, waitFor, within } from '@testing-library/react';
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
    restartDaemon: () => Promise.resolve(),
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
    const button = screen.getByRole('button', { name: 'Change…' });
    expect(button).toBeInTheDocument();
    expect(button).toBeDisabled();
  });

  it('renders Name your Profile heading and Back to Agent CTA', async () => {
    const onBack = vi.fn();
    renderHarness(makeState({ workspaceRoot: '/custom/nexus' }), { onBack });

    expect(screen.getByRole('heading', { name: 'Name your Profile' })).toBeInTheDocument();
    // V1.121 P2: wizard step titles are content voice (serif display tier).
    const title = screen.getByRole('heading', { name: 'Name your Profile' });
    expect(title).toHaveClass('font-display');
    expect(title).toHaveClass('text-display-24');
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
    // workspacePicked freezes the path so the mount name→slug reconcile (P1)
    // does not rewrite the trailing segment; this test is about row layout.
    renderHarness(makeState({ workspaceRoot: longPath, workspacePicked: true }), {
      desktop: makeDesktop({ getWorkspaceRoot: () => Promise.resolve(longPath) }),
    });

    await waitFor(() => expect(screen.getByDisplayValue(longPath)).toBeInTheDocument());
    const pathInput = screen.getByDisplayValue(longPath);
    const pathContainer = pathInput.parentElement;

    expect(pathInput).toHaveClass('truncate');
    expect(pathInput).toHaveClass('min-w-0');
    expect(pathInput).toHaveClass('flex-1');
    expect(pathContainer?.parentElement).toHaveAttribute('data-testid', 'workspace-location-row');

    const changeFolderButton = screen.getByRole('button', { name: 'Change…' });
    expect(changeFolderButton).toHaveClass('shrink-0');
  });

  it('shows the desktop workspace root and a picker button in the same row', async () => {
    // workspacePicked freezes the resolved path so the mount name→slug
    // reconcile (P1) keeps the desktop-resolved root verbatim.
    renderHarness(makeState({ workspacePicked: true }), {
      desktop: makeDesktop({ getWorkspaceRoot: () => Promise.resolve('/custom/nexus') }),
    });

    await waitFor(() => expect(screen.getByDisplayValue('/custom/nexus')).toBeInTheDocument());
    const changeFolderButton = screen.getByRole('button', { name: 'Change…' });
    expect(changeFolderButton).toBeInTheDocument();
    expect(changeFolderButton.closest('[data-testid="workspace-location-row"]')).toBeInTheDocument();
  });

  it('updates the workspace root when the picker returns a directory', async () => {
    const user = userEvent.setup();
    const pickDirectory = vi.fn(() => Promise.resolve('/picked/workspace'));

    // workspacePicked freezes the resolved path so the mount name→slug
    // reconcile (P1) keeps `/custom/nexus` as the picker seed.
    renderHarness(makeState({ workspacePicked: true }), {
      desktop: makeDesktop({ getWorkspaceRoot: () => Promise.resolve('/custom/nexus'), pickDirectory }),
    });

    await waitFor(() => expect(screen.getByRole('button', { name: 'Change…' })).toBeEnabled());
    await user.click(screen.getByRole('button', { name: 'Change…' }));

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

    // P1 default name `default`: slug('default') === basename(stalePath), so the
    // mount reconcile leaves the path as-is and Continue persists stalePath.
    renderHarness(makeState({ workspaceRoot: stalePath, profileDisplayName: 'default' }), {
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

    await waitFor(() =>
      expect(within(screen.getByTestId('wizard-continue-error')).getByText('permission denied')).toBeInTheDocument(),
    );
    // V1.121 P2: error surface consumes the P1 error-surface tokens (no raw
    // color-mix arbitrary classes).
    const errorRegion = screen.getByTestId('wizard-continue-error');
    expect(errorRegion).toHaveClass('bg-error-surface');
    expect(errorRegion).toHaveClass('border-error-surface-border');
    expect(errorRegion.className).not.toContain('color-mix');
    expect(screen.queryByRole('button', { name: 'Reset' })).not.toBeInTheDocument();
    expect(onNext).not.toHaveBeenCalled();
  });

  it('surfaces a pickDirectory error and stays on the step', async () => {
    const user = userEvent.setup();
    const pickDirectory = vi.fn(() => Promise.reject(new Error('dialog failed')));

    renderHarness(makeState(), {
      desktop: makeDesktop({ getWorkspaceRoot: () => Promise.resolve('/custom/nexus'), pickDirectory }),
    });

    await waitFor(() => expect(screen.getByRole('button', { name: 'Change…' })).toBeEnabled());
    await user.click(screen.getByRole('button', { name: 'Change…' }));

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
    await waitFor(() =>
      expect(within(screen.getByTestId('wizard-continue-error')).getByText('permission denied')).toBeInTheDocument(),
    );

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

    // P1 default name `default`: slug('default') === basename(stalePath), so the
    // mount reconcile leaves the path as-is and Continue persists stalePath.
    renderHarness(makeState({ workspaceRoot: stalePath, profileDisplayName: 'default' }), {
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

    await waitFor(() =>
      expect(within(screen.getByTestId('wizard-continue-error')).getByText(/config write failed/)).toBeInTheDocument(),
    );
    // soft_bootstrap: inline alert shows the soft helper, Reset is NOT offered.
    expect(
      within(screen.getByTestId('wizard-continue-error')).getByText('Fix the issue and tap Continue again.'),
    ).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Reset' })).not.toBeInTheDocument();
    expect(onNext).not.toHaveBeenCalled();
  });

  it('reset local database after bootstrap failure reloads without startDaemon', async () => {
    const user = userEvent.setup();
    const onNext = vi.fn();
    // Migration-class error -> classified `migration_db` -> Reset renders (AD-P0).
    const ensureSetupBootstrap = vi.fn(() =>
      Promise.reject(new Error('Failed to open creator database: Failed to run database migrations: schema mismatch')),
    );
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
      expect(screen.getByRole('button', { name: 'Reset' })).toBeInTheDocument(),
    );
    // AC-P0-3: migration-class failure -> inline alert + Reset allowed.
    expect(screen.getByTestId('wizard-continue-error')).toHaveAttribute('data-continue-error-class', 'migration_db');
    expect(
      within(screen.getByTestId('wizard-continue-error')).getByText('Failed to open creator database: Failed to run database migrations: schema mismatch'),
    ).toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: 'Reset' }));
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
    await waitFor(() =>
      expect(within(screen.getByTestId('wizard-continue-error')).getByText(/config write failed/)).toBeInTheDocument(),
    );
    // AC-P0-2 / AC-P0-5: soft_bootstrap shows inline error but NO Reset; Continue
    // stays enabled so the author can retry without destructive recovery.
    expect(screen.getByTestId('wizard-continue-error')).toHaveAttribute('data-continue-error-class', 'soft_bootstrap');
    expect(screen.queryByRole('button', { name: 'Reset' })).not.toBeInTheDocument();

    await waitFor(() => expect(screen.getByRole('button', { name: 'Continue' })).toBeEnabled());
    await user.click(screen.getByRole('button', { name: 'Continue' }));
    await waitFor(() => expect(onNext).toHaveBeenCalled());
    expect(screen.queryByRole('button', { name: 'Reset' })).not.toBeInTheDocument();
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

    await waitFor(() =>
      expect(within(screen.getByTestId('wizard-continue-error')).getByText('display name conflict')).toBeInTheDocument(),
    );
    // AC-P0-2: display-name failure is soft (no Reset, no advance).
    expect(screen.getByTestId('wizard-continue-error')).toHaveAttribute('data-continue-error-class', 'soft_display_name');
    expect(screen.queryByRole('button', { name: 'Reset' })).not.toBeInTheDocument();
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

/**
 * AC-P0-* acceptance verification for plan 2026-07-15-v1.119-setup-continue-unblock.
 *
 * Each test maps one acceptance criterion and exercises the `data-continue-error-class`
 * / `data-continue-error-phase` test surfaces added in T2/T3. Together with the
 * scenario tests above (which carry the same class assertions), these give a single
 * traceable home for each AC-P0-*.
 */
describe('AC-P0-* acceptance (setup-continue-unblock)', () => {
  it('AC-P0-1: happy-path Continue advances to Done without showing Reset', async () => {
    const user = userEvent.setup();
    const onNext = vi.fn();
    const stalePath = '/Users/x/Documents/nexus42/default';

    renderHarness(makeState({ workspaceRoot: stalePath, profileDisplayName: 'default' }), {
      desktop: makeDesktop({ getWorkspaceRoot: () => Promise.resolve(stalePath) }),
      onNext,
    });

    await waitFor(() => expect(screen.getByRole('button', { name: 'Continue' })).toBeEnabled());
    await user.click(screen.getByRole('button', { name: 'Continue' }));

    await waitFor(() => expect(onNext).toHaveBeenCalled());
    // No error state on the happy path: no inline alert, no Reset, no phase.
    expect(screen.queryByTestId('wizard-continue-error')).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Reset' })).not.toBeInTheDocument();
    expect(screen.getByTestId('wizard-cta-row')).not.toHaveAttribute('data-continue-error-phase');
  });

  it('AC-P0-2: soft failure shows inline error (no Reset)', async () => {
    const user = userEvent.setup();
    const onNext = vi.fn();
    const ensureSetupBootstrap = vi.fn(() => Promise.reject(new Error('config write failed')));

    renderHarness(makeState({ workspaceRoot: '/custom/nexus' }), {
      desktop: makeDesktop({ ensureSetupBootstrap }),
      onNext,
    });

    await waitFor(() => expect(screen.getByRole('button', { name: 'Continue' })).toBeEnabled());
    await user.click(screen.getByRole('button', { name: 'Continue' }));

    const alert = await screen.findByTestId('wizard-continue-error');
    expect(alert).toHaveAttribute('data-continue-error-class', 'soft_bootstrap');
    expect(screen.getByTestId('wizard-cta-row')).toHaveAttribute('data-continue-error-phase', 'bootstrap');
    // Soft classes never offer Reset (spec product rule 3).
    expect(screen.queryByRole('button', { name: 'Reset' })).not.toBeInTheDocument();
    expect(onNext).not.toHaveBeenCalled();
  });

  it('AC-P0-3: migration-class failure shows inline error + Reset', async () => {
    const user = userEvent.setup();
    const ensureSetupBootstrap = vi.fn(() =>
      Promise.reject(new Error('Failed to open creator database: Failed to run database migrations: schema mismatch')),
    );

    renderHarness(makeState({ workspaceRoot: '/custom/nexus' }), {
      desktop: makeDesktop({ ensureSetupBootstrap }),
    });

    await waitFor(() => expect(screen.getByRole('button', { name: 'Continue' })).toBeEnabled());
    await user.click(screen.getByRole('button', { name: 'Continue' }));

    const alert = await screen.findByTestId('wizard-continue-error');
    expect(alert).toHaveAttribute('data-continue-error-class', 'migration_db');
    expect(screen.getByTestId('wizard-cta-row')).toHaveAttribute('data-continue-error-phase', 'bootstrap');
    expect(screen.getByRole('button', { name: 'Reset' })).toBeInTheDocument();
  });

  it('AC-P0-4: inline alert is present whenever an error is set (toast secondary only)', async () => {
    const user = userEvent.setup();
    const setWorkspacePath = vi.fn(() => Promise.reject(new Error('permission denied')));
    const stalePath = '/Users/x/Documents/nexus42/default';

    renderHarness(makeState({ workspaceRoot: stalePath }), {
      desktop: makeDesktop({ getWorkspaceRoot: () => Promise.resolve(stalePath), setWorkspacePath }),
    });

    await waitFor(() => expect(screen.getByRole('button', { name: 'Continue' })).toBeEnabled());
    await user.click(screen.getByRole('button', { name: 'Continue' }));

    // The inline alert is the primary signal — it must be present (with role=alert)
    // whenever continueError is set, regardless of the toast.
    const alert = await screen.findByTestId('wizard-continue-error');
    expect(alert).toHaveAttribute('role', 'alert');
    expect(alert).toHaveAttribute('data-continue-error-class', 'soft_workspace_path');
  });

  it('AC-P0-5: after a soft failure, Continue retry succeeds without reload', async () => {
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
    // First Continue: soft failure (soft_bootstrap) — Continue stays enabled.
    await user.click(screen.getByRole('button', { name: 'Continue' }));
    const alert = await screen.findByTestId('wizard-continue-error');
    expect(alert).toHaveAttribute('data-continue-error-class', 'soft_bootstrap');

    // Retry without reload — second Continue resolves and advances.
    await waitFor(() => expect(screen.getByRole('button', { name: 'Continue' })).toBeEnabled());
    await user.click(screen.getByRole('button', { name: 'Continue' }));
    await waitFor(() => expect(onNext).toHaveBeenCalled());
  });
});

/**
 * AC-P1-* acceptance verification for plan 2026-07-15-v1.119-setup-workspace-profile-path.
 *
 * Each test maps one acceptance criterion from the P1 spec
 * (`.mstar/iterations/v1.119/specs/setup-workspace-profile-path.md`):
 * default name, focus layout, unpicked name→path sync, picked path freeze,
 * and Continue enabled on first paint. The slug pipeline itself is covered
 * exhaustively in `src/lib/workspace-profile-slug.test.ts`.
 */
describe('AC-P1-* acceptance (workspace-profile-path)', () => {
  it('AC-P1-1: Profile name field defaults to `default` (wizard init value)', async () => {
    // Mirrors the SetupWizardPage useState init (profileDisplayName: 'default'
    // in setup-wizard-page.tsx). The Workspace step receives it and renders
    // `default` without the author typing — no Continue-for-empty-name block.
    renderHarness(makeState({ profileDisplayName: 'default' }), {
      desktop: makeDesktop({ getWorkspaceRoot: () => Promise.resolve('/home/alice/Documents/nexus/default') }),
    });

    const input = await waitFor(() => screen.getByTestId('wizard-profile-name'));
    expect(input).toHaveValue('default');
  });

  it('AC-P1-2: focused Profile name input reserves scroll margin so the folder label is not covered', async () => {
    renderHarness(makeState({ profileDisplayName: 'default', workspacePicked: true }), {
      desktop: makeDesktop({ getWorkspaceRoot: () => Promise.resolve('/home/alice/Documents/nexus/default') }),
    });

    const input = await waitFor(() => screen.getByTestId('wizard-profile-name'));
    // The browser auto scroll-into-view on focus must leave room above so the
    // "Workspace folder" label stays visible at 480px card width (spec § Focus).
    expect(input).toHaveClass('scroll-mt-4');
  });

  it('AC-P1-3: typing a Profile name updates the path last segment while the folder is unpicked', async () => {
    const user = userEvent.setup();
    renderHarness(makeState({ profileDisplayName: 'default' }), {
      desktop: makeDesktop({ getWorkspaceRoot: () => Promise.resolve('/home/alice/Documents/nexus/default') }),
    });

    // Mount: slug('default') === basename → path unchanged.
    await waitFor(() =>
      expect(screen.getByDisplayValue('/home/alice/Documents/nexus/default')).toBeInTheDocument(),
    );

    const input = screen.getByTestId('wizard-profile-name');
    await user.clear(input);
    await user.type(input, 'alice');

    // onChange sync: last path segment becomes slug('alice') = 'alice'.
    await waitFor(() =>
      expect(screen.getByDisplayValue('/home/alice/Documents/nexus/alice')).toBeInTheDocument(),
    );
  });

  it('AC-P1-4: after Browse picks a folder, renaming the Profile does not alter the workspace path', async () => {
    const user = userEvent.setup();
    const pickDirectory = vi.fn(() => Promise.resolve('/picked/creative-zone'));

    renderHarness(makeState({ profileDisplayName: 'default' }), {
      desktop: makeDesktop({
        getWorkspaceRoot: () => Promise.resolve('/home/alice/Documents/nexus/default'),
        pickDirectory,
      }),
    });

    await waitFor(() => expect(screen.getByRole('button', { name: 'Change…' })).toBeEnabled());
    await user.click(screen.getByRole('button', { name: 'Change…' }));
    await waitFor(() =>
      expect(screen.getByDisplayValue('/picked/creative-zone')).toBeInTheDocument(),
    );

    // Rename the Profile after picking — workspacePicked is true → path frozen.
    const input = screen.getByTestId('wizard-profile-name');
    await user.clear(input);
    await user.type(input, 'Alice');

    expect(screen.getByDisplayValue('/picked/creative-zone')).toBeInTheDocument();
  });

  it('AC-P1-5: Continue is enabled on first paint with the default name and resolved path', async () => {
    renderHarness(makeState({ profileDisplayName: 'default' }), {
      desktop: makeDesktop({ getWorkspaceRoot: () => Promise.resolve('/home/alice/Documents/nexus/default') }),
    });

    // Default name `default` + desktop-resolved path → Continue is not disabled
    // for an empty name on first paint (the F7 confusion P1 removes).
    await waitFor(() => expect(screen.getByRole('button', { name: 'Continue' })).toBeEnabled());
    expect(screen.getByTestId('wizard-profile-name')).toHaveValue('default');
  });
});
