import { beforeEach, describe, expect, it, vi } from 'vitest';
import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import { StepAgent } from './step-agent';
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

function makeClient(scanAgents: NexusClient['scanAgents']): NexusClient {
  return {
    scanAgents,
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

    expect(screen.getByText('第 2 步 / 共 3 步')).toBeInTheDocument();
    expect(screen.getByRole('heading', { name: '你的 AI 搭档' })).toBeInTheDocument();

    const recommended = await screen.findByTestId('agent-row-claude');
    expect(
      screen.getByText('在你的电脑上找到了 2 个可用的 AI，推荐用第一个'),
    ).toBeInTheDocument();
    expect(recommended).toHaveAttribute('aria-checked', 'true');
    expect(screen.getByText('已安装 · 开箱即用 · 推荐')).toBeInTheDocument();
    expect(screen.getByText('2.1.4')).toBeInTheDocument();
    expect(screen.getByTestId('agent-row-codex')).toHaveAttribute('aria-checked', 'false');
    expect(screen.getByText('已安装 · 可切换')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '下一步' })).toBeEnabled();
  });

  it('persists the selected agent under the creator-scoped key', async () => {
    const user = userEvent.setup();
    const { onNext } = renderStep(makeClient(() => Promise.resolve({ agents: AGENTS })));

    await user.click(await screen.findByTestId('agent-row-codex'));

    expect(screen.getByTestId('agent-row-codex')).toHaveAttribute('aria-checked', 'true');
    expect(JSON.parse(window.localStorage.getItem('nexus-agent-profile:creator-a')!)).toEqual({
      name: 'Codex CLI',
      launchCommand: '/usr/local/bin/codex',
    });

    await user.click(screen.getByRole('button', { name: '下一步' }));
    expect(onNext).toHaveBeenCalledTimes(1);
  });

  it('persists the scan entry name, not the catalog display name', async () => {
    const user = userEvent.setup();
    renderStep(
      makeClient(() =>
        Promise.resolve({
          agents: [
            {
              name: 'Claude Code',
              // Catalog override maps this key to the display name `Claude`;
              // the saved profile must keep the scan entry name so the badge
              // resolver matches it.
              registry_agent_id: 'claude-native',
              launch_command: '/usr/local/bin/claude',
              installed: true,
            },
          ],
        }),
      ),
    );

    await user.click(await screen.findByTestId('agent-row-claude-native'));

    expect(JSON.parse(window.localStorage.getItem('nexus-agent-profile:creator-a')!)).toEqual({
      name: 'Claude Code',
      launchCommand: '/usr/local/bin/claude',
    });
  });

  it('keeps the selection after a reload in the same browser', async () => {
    window.localStorage.setItem(
      'nexus-agent-profile:creator-a',
      JSON.stringify({ name: 'Codex CLI', launchCommand: '/usr/local/bin/codex' }),
    );
    const client = makeClient(() => Promise.resolve({ agents: AGENTS }));

    const first = renderStep(client);
    expect(await screen.findByTestId('agent-row-codex')).toHaveAttribute('aria-checked', 'true');
    first.unmount();

    renderStep(client);
    expect(await screen.findByTestId('agent-row-codex')).toHaveAttribute('aria-checked', 'true');
  });

  it('does not inherit another creator id’s selection', async () => {
    window.localStorage.setItem(
      'nexus-agent-profile:creator-a',
      JSON.stringify({ name: 'Codex CLI', launchCommand: '/usr/local/bin/codex' }),
    );

    renderStep(makeClient(() => Promise.resolve({ agents: AGENTS })), {
      creatorId: 'creator-b',
    });

    expect(await screen.findByTestId('agent-row-claude')).toHaveAttribute('aria-checked', 'true');
    expect(screen.getByTestId('agent-row-codex')).toHaveAttribute('aria-checked', 'false');
  });

  it('shows the empty state and recovers through a manual launch command', async () => {
    const user = userEvent.setup();
    const { onNext } = renderStep(makeClient(() => Promise.resolve({ agents: [] })));

    expect(await screen.findByRole('heading', { name: '没有找到可用的 AI' })).toBeInTheDocument();
    expect(
      screen.getByText('Nexus 需要一个本地 AI 助手（如 Claude Code、Codex CLI）来协同创作。'),
    ).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '下一步' })).toBeDisabled();

    await user.click(screen.getByRole('button', { name: '▸ 都没有？查看如何安装一个 AI 助手' }));
    expect(screen.getByRole('link', { name: 'Claude Code' })).toHaveAttribute(
      'href',
      'https://claude.ai/code',
    );
    expect(screen.getByRole('link', { name: 'Codex CLI' })).toHaveAttribute(
      'href',
      'https://openai.com/codex/',
    );

    await user.click(screen.getByRole('button', { name: '▸ 高级：手动指定启动命令' }));
    await user.type(screen.getByRole('textbox', { name: '启动命令' }), 'claude --model opus');

    expect(screen.getByRole('button', { name: '下一步' })).toBeEnabled();
    await user.click(screen.getByRole('button', { name: '下一步' }));
    expect(onNext).toHaveBeenCalledTimes(1);
    expect(JSON.parse(window.localStorage.getItem('nexus-agent-profile:creator-a')!)).toEqual({
      name: 'claude --model opus',
      launchCommand: 'claude --model opus',
    });
  });

  it('restores a saved manual command on an empty scan so Continue can proceed', async () => {
    const user = userEvent.setup();
    const onNext = vi.fn();
    window.localStorage.setItem(
      'nexus-agent-profile:creator-a',
      JSON.stringify({ name: 'my-cli', launchCommand: 'my-cli --flag' }),
    );
    renderStep(makeClient(() => Promise.resolve({ agents: [] })), { onNext });

    expect(await screen.findByRole('heading', { name: '没有找到可用的 AI' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '下一步' })).toBeEnabled();

    await user.click(screen.getByRole('button', { name: '▸ 高级：手动指定启动命令' }));
    expect(screen.getByRole('textbox', { name: '启动命令' })).toHaveValue('my-cli --flag');

    await user.click(screen.getByRole('button', { name: '下一步' }));
    expect(onNext).toHaveBeenCalledTimes(1);
    // Manual-only profiles carry the command as the profile name.
    expect(JSON.parse(window.localStorage.getItem('nexus-agent-profile:creator-a')!)).toEqual({
      name: 'my-cli --flag',
      launchCommand: 'my-cli --flag',
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
    await user.click(screen.getByRole('button', { name: '下一步' }));

    // The Toaster mounts per render; assert exactly one actionable toast.
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

    expect(await screen.findByText('暂时无法扫描本机 AI（daemon offline）')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '下一步' })).toBeDisabled();

    await user.click(screen.getByRole('button', { name: '重试扫描' }));

    await waitFor(() => expect(screen.getByTestId('agent-row-claude')).toBeInTheDocument());
    expect(screen.getByRole('button', { name: '下一步' })).toBeEnabled();
  });

  it('keeps ACP protocol vocabulary off the creator surface', async () => {
    const { container } = renderStep(makeClient(() => Promise.resolve({ agents: AGENTS })));

    await screen.findByTestId('agent-row-claude');
    expect(container.textContent).not.toMatch(/acp/i);
  });
});
