import { beforeEach, describe, expect, it, vi } from 'vitest';
import { act, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import { StepAgentWorkspace } from './step-agent-workspace';
import { renderInApp } from '@/test/test-providers';
import type { NexusClient } from '@/lib/nexus';
import type { DesktopCapabilities } from '@/lib/nexus/desktop-capabilities';
import type { AgentProfile, AgentScanEntry } from '@42ch/nexus-contracts';

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

// equivalent until the lib target reaches es2024.
function withResolvers<T>(): { promise: Promise<T>; resolve: (value: T) => void } {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((res) => {
    resolve = res;
  });
  return { promise, resolve };
}

function makeClient(
  creativeRoot?: string,
  saved: AgentProfile | null = null,
): NexusClient {
  let stored = saved;
  return {
    scanAgents: () => Promise.resolve({ agents: AGENTS }),
    getAgentProfile: () => Promise.resolve({ profile: stored }),
    saveAgentProfile: vi.fn((profile: AgentProfile) => {
      stored = profile;
      return Promise.resolve({ profile: stored });
    }),
    getActiveWorkspace: () =>
      Promise.resolve({
        creator_id: 'creator-a',
        workspace_slug: 'default',
        creative_root: creativeRoot,
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
  {
    client = makeClient('/home/creator/Nexus'),
    desktop = null,
    onFinish = vi.fn(),
  }: {
    client?: NexusClient;
    desktop?: DesktopCapabilities | null;
    onFinish?: () => void;
  } = {},
) {
  return {
    onFinish,
    ...renderInApp(<StepAgentWorkspace onBack={() => {}} onFinish={onFinish} />, {
      client,
      desktop,
      activeCreatorId: 'creator-a',
    }),
  };
}

describe('StepAgentWorkspace (developer agent + workspace)', () => {
  beforeEach(() => window.localStorage.clear());

  it('renders the merged step copy with agent rows and the ACP pill', async () => {
    renderStep();

    expect(screen.getByText('Step 2 of 2 · Developer')).toBeInTheDocument();
    expect(screen.getByRole('heading', { name: 'Agent and Workspace' })).toBeInTheDocument();
    expect(screen.getByText('Choose the agent the orchestration engine uses, and where your projects live')).toBeInTheDocument();
    expect(
      screen.getByLabelText("Custom launch command (optional, overrides the selected agent's default launch)"),
    ).toHaveAttribute('placeholder', 'e.g. claude --acp --model opus');
    expect(screen.getByText('Workspace root')).toBeInTheDocument();

    expect(await screen.findByTestId('agent-row-claude')).toHaveAttribute('aria-checked', 'true');
    expect(screen.getByRole('button', { name: 'Finish' })).toBeEnabled();
    expect(screen.getAllByText('ACP')).toHaveLength(2);
    expect(screen.getByText('ACP · Installed · Recommended')).toBeInTheDocument();
  });

  it('shows the reported creative_root in the advanced area (browser, display-only)', async () => {
    const user = userEvent.setup();
    renderStep();

    await waitFor(() =>
      expect(screen.getByTestId('workspace-root')).toHaveTextContent('/home/creator/Nexus'),
    );
    expect(screen.queryByRole('button', { name: 'Browse…' })).not.toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: 'Advanced: Change location' }));

    expect(screen.getByRole('textbox', { name: 'Workspace root' })).toHaveValue(
      '/home/creator/Nexus',
    );
    expect(screen.queryByRole('button', { name: 'Browse…' })).not.toBeInTheDocument();
  });

  it('reads Default workspace when the service reports no root and hides the empty advanced area', async () => {
    renderStep({ client: makeClient(undefined) });

    await waitFor(() => expect(screen.getByTestId('workspace-root')).toHaveTextContent('Default workspace'));
    expect(screen.queryByRole('button', { name: 'Advanced: Change location' })).not.toBeInTheDocument();
  });

  it('picks a directory on desktop and persists it to the desktop config', async () => {
    const user = userEvent.setup();
    const pickDirectory = vi.fn().mockResolvedValue('/tmp/picked-root');
    const setWorkspacePath = vi.fn().mockResolvedValue(undefined);
    renderStep({
      client: makeClient(undefined),
      desktop: makeDesktop({ pickDirectory, setWorkspacePath }),
    });

    await user.click(await screen.findByRole('button', { name: 'Advanced: Change location' }));
    await user.click(screen.getByRole('button', { name: 'Browse…' }));

    await waitFor(() => expect(setWorkspacePath).toHaveBeenCalledWith('/tmp/picked-root'));
    expect(pickDirectory).toHaveBeenCalled();
    expect(screen.getByTestId('workspace-root')).toHaveTextContent('/tmp/picked-root');
  });

  it('restores a late desktop profile after the scan has already landed', async () => {
    const { promise, resolve } = withResolvers<{
      name: string;
      launchCommand?: string;
    } | null>();

    renderStep({
      client: makeClient(undefined),
      desktop: makeDesktop({ getAgentProfile: () => promise }),
    });

    await screen.findByTestId('agent-row-claude');
    const field = screen.getByLabelText(
      "Custom launch command (optional, overrides the selected agent's default launch)",
    );
    expect(field).toHaveValue('');

    await act(async () => {
      resolve({ name: 'custom-agent', launchCommand: 'my-cli --flag' });
    });

    await waitFor(() => expect(field).toHaveValue('my-cli --flag'));
  });

  it('restores a saved custom launch command together with its row on re-entry', async () => {
    const user = userEvent.setup();
    const onFinish = vi.fn();
    const client = makeClient('/home/creator/Nexus', {
      name: 'Claude Code',
      launch_command: 'claude --opus',
    });
    renderStep({ client, onFinish });

    expect(await screen.findByTestId('agent-row-claude')).toHaveAttribute('aria-checked', 'true');
    expect(
      screen.getByLabelText("Custom launch command (optional, overrides the selected agent's default launch)"),
    ).toHaveValue('claude --opus');

    await user.click(screen.getByRole('button', { name: 'Finish' }));
    expect(onFinish).toHaveBeenCalledTimes(1);
    expect(vi.mocked(client.saveAgentProfile)).toHaveBeenCalledWith({
      name: 'Claude Code',
      launch_command: 'claude --opus',
    });
  });

  it('completes with the recommended agent and reports the finish intent', async () => {
    const user = userEvent.setup();
    const { onFinish } = renderStep();

    await screen.findByTestId('agent-row-claude');
    await user.click(screen.getByRole('button', { name: 'Finish' }));

    expect(onFinish).toHaveBeenCalledTimes(1);
  });

  it('requires an agent or a manual launch command before finishing', async () => {
    const user = userEvent.setup();
    renderStep({
      client: {
        scanAgents: () => Promise.resolve({ agents: [] }),
        getActiveWorkspace: () =>
          Promise.resolve({
            creator_id: 'creator-a',
            workspace_slug: 'default',
            operational_dir: '/tmp/nexus',
          }),
      } as unknown as NexusClient,
    });

    expect(screen.getByRole('button', { name: 'Finish' })).toBeDisabled();

    const customLaunch = await screen.findByLabelText(
      'Custom launch command (required: no agent was found on this computer)',
    );
    expect(screen.queryByText(/Custom launch command \(optional/)).not.toBeInTheDocument();

    await user.type(customLaunch, 'my-agent --flag');

    expect(screen.getByRole('button', { name: 'Finish' })).toBeEnabled();
  });
});
