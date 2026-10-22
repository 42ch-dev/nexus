import { beforeEach, describe, expect, it, vi } from 'vitest';
import { act, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import { StepAgentWorkspace } from './step-agent-workspace';
import { renderInApp } from '@/test/test-providers';
import type { NexusClient } from '@/lib/nexus';
import type { DesktopCapabilities } from '@/lib/nexus/desktop-capabilities';
import type { AgentScanEntry } from '@42ch/nexus-contracts';

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

// Promise.withResolvers is unavailable at the current tsconfig lib — local
// equivalent until the lib target reaches es2024.
function withResolvers<T>(): { promise: Promise<T>; resolve: (value: T) => void } {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((res) => {
    resolve = res;
  });
  return { promise, resolve };
}

function makeClient(creativeRoot?: string): NexusClient {
  return {
    scanAgents: () => Promise.resolve({ agents: AGENTS }),
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

    expect(screen.getByText('第 2 步 / 共 2 步 · 开发者')).toBeInTheDocument();
    expect(screen.getByRole('heading', { name: 'Agent 与工作区' })).toBeInTheDocument();
    expect(screen.getByText('选择编排引擎使用的 agent，以及项目存放位置')).toBeInTheDocument();
    expect(
      screen.getByLabelText('自定义启动命令（可选，覆盖选中 agent 的默认启动方式）'),
    ).toHaveAttribute('placeholder', '例如：claude --acp --model opus');
    expect(screen.getByText('工作区根目录')).toBeInTheDocument();

    expect(await screen.findByTestId('agent-row-claude')).toHaveAttribute('aria-checked', 'true');
    expect(screen.getByRole('button', { name: '进入开发台' })).toBeEnabled();
    expect(screen.getAllByText('ACP')).toHaveLength(2);
    expect(screen.getByText('ACP · 已安装 · 推荐')).toBeInTheDocument();
  });

  it('shows the reported creative_root in the advanced area (browser, display-only)', async () => {
    const user = userEvent.setup();
    renderStep();

    await waitFor(() =>
      expect(screen.getByTestId('workspace-root')).toHaveTextContent('/home/creator/Nexus'),
    );
    expect(screen.queryByRole('button', { name: '浏览…' })).not.toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: '高级：更改位置' }));

    expect(screen.getByRole('textbox', { name: '工作区根目录' })).toHaveValue(
      '/home/creator/Nexus',
    );
    expect(screen.queryByRole('button', { name: '浏览…' })).not.toBeInTheDocument();
  });

  it('reads 默认工作区 when the service reports no root and hides the empty advanced area', async () => {
    renderStep({ client: makeClient(undefined) });

    await waitFor(() => expect(screen.getByTestId('workspace-root')).toHaveTextContent('默认工作区'));
    expect(screen.queryByRole('button', { name: '高级：更改位置' })).not.toBeInTheDocument();
  });

  it('picks a directory on desktop and persists it to the desktop config', async () => {
    const user = userEvent.setup();
    const pickDirectory = vi.fn().mockResolvedValue('/tmp/picked-root');
    const setWorkspacePath = vi.fn().mockResolvedValue(undefined);
    renderStep({
      client: makeClient(undefined),
      desktop: makeDesktop({ pickDirectory, setWorkspacePath }),
    });

    await user.click(await screen.findByRole('button', { name: '高级：更改位置' }));
    await user.click(screen.getByRole('button', { name: '浏览…' }));

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

    // Scan lands first; the IPC profile is still in flight.
    await screen.findByTestId('agent-row-claude');
    const field = screen.getByLabelText(
      '自定义启动命令（可选，覆盖选中 agent 的默认启动方式）',
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
    window.localStorage.setItem(
      'nexus-agent-profile:creator-a',
      JSON.stringify({ name: 'Claude Code', launchCommand: 'claude --opus' }),
    );
    renderStep({ client: makeClient('/home/creator/Nexus'), onFinish });

    expect(await screen.findByTestId('agent-row-claude')).toHaveAttribute('aria-checked', 'true');
    expect(
      screen.getByLabelText('自定义启动命令（可选，覆盖选中 agent 的默认启动方式）'),
    ).toHaveValue('claude --opus');

    await user.click(screen.getByRole('button', { name: '进入开发台' }));
    expect(onFinish).toHaveBeenCalledTimes(1);
    expect(JSON.parse(window.localStorage.getItem('nexus-agent-profile:creator-a')!)).toEqual({
      name: 'Claude Code',
      launchCommand: 'claude --opus',
    });
  });

  it('completes with the recommended agent and reports the finish intent', async () => {
    const user = userEvent.setup();
    const { onFinish } = renderStep();

    await screen.findByTestId('agent-row-claude');
    await user.click(screen.getByRole('button', { name: '进入开发台' }));

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

    expect(screen.getByRole('button', { name: '进入开发台' })).toBeDisabled();

    await user.type(
      screen.getByLabelText('自定义启动命令（可选，覆盖选中 agent 的默认启动方式）'),
      'my-agent --flag',
    );

    expect(screen.getByRole('button', { name: '进入开发台' })).toBeEnabled();
  });
});
