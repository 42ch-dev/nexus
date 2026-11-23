import { beforeEach, describe, expect, it, vi } from 'vitest';
import { act, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import { StepAgent } from './step-agent';
import { renderInApp } from '@/test/test-providers';
import type { NexusClient } from '@/lib/nexus';
import type { DesktopCapabilities } from '@/lib/nexus/desktop-capabilities';
import type { AgentProfile, AgentProfileResponse, AgentScanEntry } from '@42ch/nexus-contracts';

const AGENTS: AgentScanEntry[] = [
  {
    name: 'Claude Code',
    registry_agent_id: 'claude',
    launch_command: '/usr/local/bin/claude',
    installed: true,
    version: '2.1.4',
  },
  {
    name: 'Codex CLI',
    registry_agent_id: 'codex',
    launch_command: '/usr/local/bin/codex',
    installed: true,
    version: '0.9.2',
  },
];

/**
 * serves what `saveAgentProfile` persisted, so a remount reads it back.
 */
function makeClient(
  scanAgents: NexusClient['scanAgents'],
  saved: AgentProfile | null = null,
): NexusClient {
  let stored = saved;
  return {
    scanAgents,
    getAgentProfile: () => Promise.resolve({ profile: stored }),
    saveAgentProfile: vi.fn((profile: AgentProfile) => {
      stored = profile;
      return Promise.resolve({ profile: stored });
    }),
    getActiveWorkspace: () =>
      Promise.resolve({
        creator_id: 'creator-a',
        workspace_slug: 'default',
        operational_dir: '/tmp/nexus',
      }),
  } as unknown as NexusClient;
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
    restartDaemon: () => Promise.resolve(),
    resetLocalDatabase: () => Promise.resolve({ status: 'confirmed' }),
    pickDirectory: () => Promise.resolve(null),
    setWorkspacePath: () => Promise.resolve(),
    getSetupCompleted: () => Promise.resolve(true),
    setSetupCompleted: () => Promise.resolve(),
    getEntrance: () => Promise.resolve('content-creator'),
    setEntrance: () => Promise.resolve(),
    setAgentProfile: () => Promise.resolve(),
    getAgentProfile: () => Promise.resolve(null),
    switchActiveCreator: () => Promise.resolve('/tmp/nexus'),
    getWorkspaceRoot: () => Promise.resolve('/tmp/nexus'),
    toggleMaximizeWindow: () => Promise.resolve(),
    ...overrides,
  };
}

function renderStep(
  client: NexusClient,
  {
    onNext = vi.fn(),
    creatorId = 'creator-a',
    desktop = null,
  }: {
    onNext?: () => void;
    creatorId?: string;
    desktop?: DesktopCapabilities | null;
  } = {},
) {
  return {
    onNext,
    ...renderInApp(<StepAgent onBack={() => {}} onNext={onNext} />, {
      client,
      desktop,
      activeCreatorId: creatorId,
    }),
  };
}

describe('StepAgent (creator AI partner)', () => {
  beforeEach(() => window.localStorage.clear());

  it('renders the installed agents with the recommended pick preselected', async () => {
    renderStep(makeClient(() => Promise.resolve({ agents: AGENTS })));

    expect(screen.getByText('Step 2 of 3')).toBeInTheDocument();
    expect(screen.getByRole('heading', { name: 'Your AI Partner' })).toBeInTheDocument();

    const recommended = await screen.findByTestId('agent-row-claude');
    expect(
      screen.getByText('Found 2 available AIs on your computer. We recommend the first one'),
    ).toBeInTheDocument();
    expect(recommended).toHaveAttribute('aria-checked', 'true');
    expect(screen.getByText('Installed · Ready to use · Recommended')).toBeInTheDocument();
    expect(screen.getByText('2.1.4')).toBeInTheDocument();
    expect(screen.getByTestId('agent-row-codex')).toHaveAttribute('aria-checked', 'false');
    expect(screen.getByText('Installed · Switchable')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Next' })).toBeEnabled();
  });

  it('persists the selected agent on the service', async () => {
    const user = userEvent.setup();
    const client = makeClient(() => Promise.resolve({ agents: AGENTS }));
    const { onNext } = renderStep(client);

    await user.click(await screen.findByTestId('agent-row-codex'));

    expect(screen.getByTestId('agent-row-codex')).toHaveAttribute('aria-checked', 'true');
    expect(vi.mocked(client.saveAgentProfile)).toHaveBeenCalledWith({
      name: 'Codex CLI',
      launch_command: '/usr/local/bin/codex',
    });

    await user.click(screen.getByRole('button', { name: 'Next' }));
    expect(onNext).toHaveBeenCalledTimes(1);
  });

  it('persists the scan entry name, not the catalog display name', async () => {
    const user = userEvent.setup();
    const client = makeClient(() =>
      Promise.resolve({
        agents: [
          {
            name: 'Claude Code',
            // the saved profile must keep the scan entry name so the badge
            // resolver matches it.
            registry_agent_id: 'claude-native',
            launch_command: '/usr/local/bin/claude',
            installed: true,
          },
        ],
      }),
    );
    renderStep(client);

    await user.click(await screen.findByTestId('agent-row-claude-native'));

    expect(vi.mocked(client.saveAgentProfile)).toHaveBeenCalledWith({
      name: 'Claude Code',
      launch_command: '/usr/local/bin/claude',
    });
  });

  it('keeps the selection after a reload in the same browser', async () => {
    const client = makeClient(() => Promise.resolve({ agents: AGENTS }), {
      name: 'Codex CLI',
      launch_command: '/usr/local/bin/codex',
    });

    const first = renderStep(client);
    expect(await screen.findByTestId('agent-row-codex')).toHaveAttribute('aria-checked', 'true');
    first.unmount();

    renderStep(client);
    expect(await screen.findByTestId('agent-row-codex')).toHaveAttribute('aria-checked', 'true');
  });

  it('applies the saved profile when it arrives after the scan', async () => {
    let resolveProfile!: (value: AgentProfileResponse) => void;
    const client = {
      scanAgents: () => Promise.resolve({ agents: AGENTS }),
      getAgentProfile: () =>
        new Promise<AgentProfileResponse>((resolve) => {
          resolveProfile = resolve;
        }),
      saveAgentProfile: vi.fn(),
    } as unknown as NexusClient;

    renderStep(client);

    // pending, and Continue stays blocked (no intermediate default).
    expect(await screen.findByTestId('agent-row-claude')).toHaveAttribute(
      'aria-checked',
      'false',
    );
    expect(screen.getByRole('button', { name: 'Next' })).toBeDisabled();

    await act(async () => {
      resolveProfile({
        profile: { name: 'Codex CLI', launch_command: '/usr/local/bin/codex' },
      });
    });

    await waitFor(() =>
      expect(screen.getByTestId('agent-row-codex')).toHaveAttribute('aria-checked', 'true'),
    );
    expect(screen.getByTestId('agent-row-claude')).toHaveAttribute('aria-checked', 'false');
    expect(screen.getByRole('button', { name: 'Next' })).toBeEnabled();
  });

  it('restores the daemon-global profile for any active creator', async () => {
    // active creator does not scope or hide it.
    const client = makeClient(() => Promise.resolve({ agents: AGENTS }), {
      name: 'Codex CLI',
      launch_command: '/usr/local/bin/codex',
    });

    renderStep(client, { creatorId: 'creator-b' });

    expect(await screen.findByTestId('agent-row-codex')).toHaveAttribute('aria-checked', 'true');
    expect(screen.getByTestId('agent-row-claude')).toHaveAttribute('aria-checked', 'false');
  });

  it('shows the empty state and recovers through a manual launch command', async () => {
    const user = userEvent.setup();
    const client = makeClient(() => Promise.resolve({ agents: [] }));
    const { onNext } = renderStep(client);

    expect(await screen.findByRole('heading', { name: 'No AI Found' })).toBeInTheDocument();
    expect(
      screen.getByText('Nexus needs a local AI assistant (such as Claude Code or Codex CLI) to write with you.'),
    ).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Next' })).toBeDisabled();

    await user.click(screen.getByRole('button', { name: '▸ None of these? See how to install an AI assistant' }));
    expect(screen.getByRole('link', { name: 'Claude Code' })).toHaveAttribute(
      'href',
      'https://claude.ai/code',
    );
    expect(screen.getByRole('link', { name: 'Codex CLI' })).toHaveAttribute(
      'href',
      'https://openai.com/codex/',
    );

    await user.click(screen.getByRole('button', { name: '▸ Advanced: Set a launch command manually' }));
    await user.type(screen.getByRole('textbox', { name: 'Launch command' }), 'claude --model opus');

    expect(screen.getByRole('button', { name: 'Next' })).toBeEnabled();
    await user.click(screen.getByRole('button', { name: 'Next' }));
    expect(onNext).toHaveBeenCalledTimes(1);
    expect(vi.mocked(client.saveAgentProfile)).toHaveBeenCalledWith({
      name: 'claude --model opus',
      launch_command: 'claude --model opus',
    });
  });

  it('restores a saved manual command on an empty scan so Continue can proceed', async () => {
    const user = userEvent.setup();
    const onNext = vi.fn();
    const client = makeClient(() => Promise.resolve({ agents: [] }), {
      name: 'my-cli --flag',
      launch_command: 'my-cli --flag',
    });
    renderStep(client, { onNext });

    expect(await screen.findByRole('heading', { name: 'No AI Found' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Next' })).toBeEnabled();

    await user.click(screen.getByRole('button', { name: '▸ Advanced: Set a launch command manually' }));
    expect(screen.getByRole('textbox', { name: 'Launch command' })).toHaveValue('my-cli --flag');

    await user.click(screen.getByRole('button', { name: 'Next' }));
    expect(onNext).toHaveBeenCalledTimes(1);
    expect(vi.mocked(client.saveAgentProfile)).toHaveBeenCalledWith({
      name: 'my-cli --flag',
      launch_command: 'my-cli --flag',
    });
  });

  it('stays on the step with an error toast when the desktop profile write fails', async () => {
    const user = userEvent.setup();
    const onNext = vi.fn();
    renderStep(makeClient(() => Promise.resolve({ agents: AGENTS })), {
      onNext,
      desktop: makeDesktop({
        setAgentProfile: () => Promise.reject(new Error('disk full')),
      }),
    });

    await user.click(await screen.findByTestId('agent-row-codex'));
    await user.click(screen.getByRole('button', { name: 'Next' }));

    expect(await screen.findAllByText('Could not save agent profile.')).toHaveLength(1);
    expect(onNext).not.toHaveBeenCalled();
  });

  it('shows the scan error and retries it', async () => {
    const user = userEvent.setup();
    const scanAgents = vi
      .fn()
      .mockRejectedValueOnce(new Error('daemon offline'))
      .mockResolvedValue({ agents: AGENTS });

    renderStep(makeClient(scanAgents));

    expect(await screen.findByText('Could not scan this computer for AI (daemon offline)')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Next' })).toBeDisabled();

    await user.click(screen.getByRole('button', { name: 'Rescan' }));

    await waitFor(() => expect(screen.getByTestId('agent-row-claude')).toBeInTheDocument());
    expect(screen.getByRole('button', { name: 'Next' })).toBeEnabled();
  });

  it('keeps ACP protocol vocabulary off the creator surface', async () => {
    const { container } = renderStep(makeClient(() => Promise.resolve({ agents: AGENTS })));

    await screen.findByTestId('agent-row-claude');
    expect(container.textContent).not.toMatch(/acp/i);
  });
});
