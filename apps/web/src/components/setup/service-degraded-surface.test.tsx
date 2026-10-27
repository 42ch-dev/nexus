/**
 * Service-unavailable surface (plan v1219-onboarding T4).
 *
 * The probe is `GET /v1/daemon/daemon/status` (plus `runtime/status` when only
 * the engine is degraded, to tell "no provider selected" from a failure); one
 * instance drives two placements — full-screen over the wizard, a top banner
 * inside the shell. Copy is localized (`setup:degraded.*`); these cases pin the
 * zh-CN product copy and the EN fallback.
 */
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { act, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import { ServiceDegradedBanner, ServiceDegradedGate } from './service-degraded-surface';
import { ServiceStatusProvider } from '@/lib/service-status-context';
import { renderInApp } from '@/test/test-providers';
import { NexusClientError } from '@/lib/nexus';
import type { DaemonRuntimeStatus, NexusClient, RuntimeStatus } from '@/lib/nexus';
import type { AgentHostConfig, AgentHostProvidersResponse } from '@/lib/nexus/types';
import type { AgentScanEntry, ScanResponse } from '@42ch/nexus-contracts';

interface Deferred<T> {
  promise: Promise<T>;
  resolve: (value: T) => void;
}

// Promise.withResolvers is unavailable at the current tsconfig lib — same
// local equivalent as brief-flow.test.tsx until the lib target reaches es2024.
function withResolvers<T>(): Deferred<T> {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((res) => {
    resolve = res;
  });
  return { promise, resolve };
}

type ProfileRead = () => Promise<{ profile: { name: string } | null }>;

/** A stored agent profile = a provider is selected (its readiness probe failed). */
const SELECTED_PROFILE: ProfileRead = () => Promise.resolve({ profile: { name: 'Claude Code' } });
const UNSELECTED_PROFILE: ProfileRead = () => Promise.resolve({ profile: null });

interface ConfigStubs {
  scan?: () => Promise<ScanResponse>;
  listProviders?: () => Promise<AgentHostProvidersResponse>;
  getConfig?: () => Promise<AgentHostConfig>;
  saveConfig?: (config: AgentHostConfig) => Promise<AgentHostConfig>;
}

const EMPTY_CONFIG: AgentHostConfig = { providers: [] };

function clientWith(
  read: () => Promise<DaemonRuntimeStatus>,
  runtime: RuntimeStatus = { workspace_initialized: false },
  profile: ProfileRead = SELECTED_PROFILE,
  config: ConfigStubs = {},
): NexusClient {
  return {
    daemonStatus: read,
    runtimeStatus: () => Promise.resolve(runtime),
    getAgentProfile: profile,
    scanAgents: config.scan ?? (() => Promise.resolve({ agents: [] })),
    listAgentHostProviders:
      config.listProviders ?? (() => Promise.resolve({ providers: [] })),
    getAgentHostConfig: config.getConfig ?? (() => Promise.resolve(EMPTY_CONFIG)),
    saveAgentHostConfig:
      config.saveConfig ?? ((saved: AgentHostConfig) => Promise.resolve(saved)),
  } as unknown as NexusClient;
}

function renderSurface({
  client,
  setupCompleted,
  entries = ['/'],
}: {
  client: NexusClient;
  setupCompleted?: boolean;
  entries?: string[];
}) {
  return renderInApp(
    <ServiceStatusProvider>
      <ServiceDegradedGate>
        <div data-testid="app-content">Content</div>
        <ServiceDegradedBanner />
      </ServiceDegradedGate>
    </ServiceStatusProvider>,
    { client, setupCompleted, initialRouterEntries: entries },
  );
}

const ENGINE_DEGRADED: DaemonRuntimeStatus = {
  degraded: { subsystems: ['engine'], reasons: ['engine not ready'] },
};

const HEALTHY: DaemonRuntimeStatus = {
  degraded: { subsystems: [], reasons: [] },
};

/** Any non-engine cause keeps the banner/full-screen split. */
const DB_DEGRADED: DaemonRuntimeStatus = {
  degraded: { subsystems: ['db'], reasons: ['db not ready'] },
};

/** jsdom resolves the system locale to `en`; pin zh-CN to assert the product copy. */
beforeEach(() => window.localStorage.setItem('nexus-web-locale', 'zh-CN'));

describe('ServiceDegradedGate', () => {
  it('shows one banner inside the shell when a non-engine subsystem is degraded', async () => {
    renderSurface({
      setupCompleted: true,
      client: clientWith(() => Promise.resolve(DB_DEGRADED)),
    });

    const surface = await screen.findByTestId('service-degraded-surface');
    expect(surface).toHaveAttribute('data-variant', 'banner');
    expect(surface).toHaveTextContent(
      'Nexus 服务暂不可用：本地数据库未就绪。请确认本地服务正在运行后重试。',
    );
    expect(screen.getByRole('button', { name: '重试' })).toBeInTheDocument();
    // The shell stays usable behind the single surface — no toasts.
    expect(screen.getByTestId('app-content')).toBeInTheDocument();
    expect(screen.getAllByRole('alert')).toHaveLength(1);
  });

  it('joins the provider catalog with scan data and fills id/protocol/command', async () => {
    const user = userEvent.setup();
    const agents: AgentScanEntry[] = [
      { name: 'dsh (native CLI)', registry_agent_id: 'dsh-native', launch_command: 'dsh', launch_args: [], installed: true },
      { name: 'Alpha Agent', registry_agent_id: 'alpha-acp', launch_command: 'alpha-acp', launch_args: ['acp'], installed: true },
    ];
    renderSurface({
      setupCompleted: true,
      client: clientWith(
        () => Promise.resolve(ENGINE_DEGRADED),
        { workspace_initialized: true },
        UNSELECTED_PROFILE,
        {
          scan: () => Promise.resolve({ agents }),
          listProviders: () =>
            Promise.resolve({
              providers: [
                { provider_id: 'dsh-native', display_name: 'dsh (native CLI)', protocol_kind: 'native_cli' },
                { provider_id: 'alpha-acp', display_name: 'Alpha Agent', protocol_kind: 'acp' },
              ],
            }),
        },
      ),
    });

    const surface = await screen.findByTestId('service-degraded-surface');
    expect(surface).toHaveAttribute('data-variant', 'full');
    expect(surface).toHaveAttribute('data-state', 'unconfigured');
    expect(surface).toHaveTextContent('编排引擎未就绪');
    expect(surface).toHaveTextContent('配置编排提供方');
    // The CLI escape hatch is gone: configuration is in-page only.
    expect(surface).not.toHaveTextContent('nexus42 connect');
    expect(screen.queryByTestId('engine-repair-failure')).not.toBeInTheDocument();
    // The app is not usable until the engine is configured.
    expect(screen.queryByTestId('app-content')).not.toBeInTheDocument();

    // Same install state + no curated priority → display-name order.
    const candidates = await screen.findAllByRole('listitem');
    expect(candidates[0]).toHaveTextContent('dsh (native CLI)');
    expect(candidates[1]).toHaveTextContent('Alpha Agent');

    await user.click(within(candidates[0]!).getByRole('button', { name: '启用' }));
    expect(screen.getByLabelText('提供方 ID')).toHaveValue('dsh-native');
    expect(screen.getByLabelText('协议')).toHaveValue('native_cli');
    expect(screen.getByLabelText('启动命令')).toHaveValue('dsh');
    expect(screen.getByRole('button', { name: '保存并重启检测' })).toBeEnabled();

    await user.click(within(candidates[1]!).getByRole('button', { name: '启用' }));
    expect(screen.getByLabelText('提供方 ID')).toHaveValue('alpha-acp');
    expect(screen.getByLabelText('协议')).toHaveValue('acp');
    expect(screen.getByLabelText('启动命令')).toHaveValue('alpha-acp');
    expect(screen.getByRole('button', { name: '重新检测' })).toBeInTheDocument();
  });

  it('dedupes scan rows by provider id and keeps the curated launch recipe', async () => {
    const user = userEvent.setup();
    const agents: AgentScanEntry[] = [
      {
        name: 'omp (ACP)',
        registry_agent_id: 'omp',
        launch_command: '/opt/homebrew/bin/omp',
        launch_args: ['acp'],
        installed: true,
        protocol_kind: 'acp',
      },
      {
        name: 'omp',
        registry_agent_id: 'omp',
        launch_command: 'omp',
        launch_args: [],
        installed: true,
        protocol_kind: 'acp',
      },
    ];
    const saved: AgentHostConfig[] = [];
    renderSurface({
      setupCompleted: true,
      client: clientWith(
        () => Promise.resolve(ENGINE_DEGRADED),
        { workspace_initialized: true },
        UNSELECTED_PROFILE,
        {
          scan: () => Promise.resolve({ agents }),
          getConfig: () => Promise.resolve(EMPTY_CONFIG),
          saveConfig: (config: AgentHostConfig) => {
            saved.push(config);
            return Promise.resolve(config);
          },
        },
      ),
    });

    const roster = await screen.findAllByRole('listitem');
    expect(roster).toHaveLength(1);
    expect(roster[0]).toHaveTextContent('omp (ACP)');

    await user.click(within(roster[0]!).getByRole('button', { name: '启用' }));
    expect(screen.getByLabelText('启动命令')).toHaveValue('/opt/homebrew/bin/omp');
    await user.click(screen.getByRole('button', { name: '保存并重启检测' }));
    await waitFor(() => expect(saved.length).toBe(1));
    // The curated row's recipe (not the bare registry duplicate) is saved.
    expect(saved[0]!.providers).toEqual([
      {
        id: 'omp',
        protocol: 'acp',
        command: '/opt/homebrew/bin/omp',
        args: ['acp'],
        enabled: true,
      },
    ]);
  });

  it('renders detection-only rows disabled while a supported row stays selectable', async () => {
    const user = userEvent.setup();
    const agents: AgentScanEntry[] = [
      {
        name: 'kimi (native CLI)',
        registry_agent_id: 'kimi',
        launch_command: '/usr/local/bin/kimi',
        launch_args: [],
        installed: true,
        protocol_kind: 'native_cli',
      },
      {
        name: 'claude (native CLI)',
        registry_agent_id: 'claude-native',
        launch_command: '/usr/local/bin/claude',
        launch_args: [],
        installed: true,
        protocol_kind: 'native_cli',
      },
    ];
    renderSurface({
      setupCompleted: true,
      client: clientWith(
        () => Promise.resolve(ENGINE_DEGRADED),
        { workspace_initialized: true },
        UNSELECTED_PROFILE,
        {
          scan: () => Promise.resolve({ agents }),
          listProviders: () =>
            Promise.resolve({
              providers: [
                { provider_id: 'kimi', display_name: 'Kimi CLI', protocol_kind: 'native_cli' },
                {
                  provider_id: 'claude-native',
                  display_name: 'Claude Code',
                  protocol_kind: 'native_cli',
                },
              ],
            }),
        },
      ),
    });

    const roster = await screen.findAllByRole('listitem');
    const kimiRow = roster.find((row) => row.textContent?.includes('Kimi CLI'))!;
    const claudeRow = roster.find((row) => row.textContent?.includes('Claude Code'))!;

    // Detection-only: rendered, badged, and impossible to choose.
    expect(within(kimiRow).getByTestId('detection-only-badge')).toHaveTextContent(
      '已检测，暂不支持从此处启用',
    );
    const kimiUse = within(kimiRow).getByRole('button', { name: '启用' });
    expect(kimiUse).toBeDisabled();
    await user.click(kimiUse);
    expect(screen.getByLabelText('提供方 ID')).toHaveValue('');

    // A supported native row in the same render stays selectable.
    const claudeUse = within(claudeRow).getByRole('button', { name: '启用' });
    expect(claudeUse).toBeEnabled();
    await user.click(claudeUse);
    expect(screen.getByLabelText('提供方 ID')).toHaveValue('claude-native');
  });

  it('persists the scan-provided launch args when saving a chosen candidate', async () => {
    const user = userEvent.setup();
    const agents: AgentScanEntry[] = [
      { name: 'Alpha Agent', registry_agent_id: 'alpha-acp', launch_command: 'alpha-acp', launch_args: ['acp'], installed: true },
    ];
    const saved: AgentHostConfig[] = [];
    renderSurface({
      setupCompleted: true,
      client: clientWith(
        () => Promise.resolve(ENGINE_DEGRADED),
        { workspace_initialized: true },
        UNSELECTED_PROFILE,
        {
          scan: () => Promise.resolve({ agents }),
          listProviders: () =>
            Promise.resolve({
              providers: [
                { provider_id: 'alpha-acp', display_name: 'Alpha Agent', protocol_kind: 'acp' },
              ],
            }),
          getConfig: () =>
            Promise.resolve({ providers: [], max_sessions: 2 } as AgentHostConfig),
          saveConfig: (config: AgentHostConfig) => {
            saved.push(config);
            return Promise.resolve(config);
          },
        },
      ),
    });

    const roster = await screen.findAllByRole('listitem');
    await user.click(within(roster[0]!).getByRole('button', { name: '启用' }));
    // Correcting the executable path of the same recipe preserves args.
    await user.clear(screen.getByLabelText('启动命令'));
    await user.type(screen.getByLabelText('启动命令'), '/usr/local/bin/alpha-acp');
    await user.click(screen.getByRole('button', { name: '保存并重启检测' }));
    await waitFor(() => expect(saved.length).toBe(1));
    expect(saved[0]!.providers).toEqual([
      {
        id: 'alpha-acp',
        protocol: 'acp',
        command: '/usr/local/bin/alpha-acp',
        args: ['acp'],
        enabled: true,
      },
    ]);
  });

  it('clears scan args when the recipe is edited manually', async () => {
    const user = userEvent.setup();
    const agents: AgentScanEntry[] = [
      { name: 'omp (ACP)', registry_agent_id: 'omp', launch_command: 'omp', launch_args: ['acp'], installed: true },
    ];
    const saved: AgentHostConfig[] = [];
    renderSurface({
      setupCompleted: true,
      client: clientWith(
        () => Promise.resolve(ENGINE_DEGRADED),
        { workspace_initialized: true },
        UNSELECTED_PROFILE,
        {
          scan: () => Promise.resolve({ agents }),
          listProviders: () =>
            Promise.resolve({
              providers: [
                { provider_id: 'omp', display_name: 'omp (ACP)', protocol_kind: 'acp' },
              ],
            }),
          getConfig: () =>
            Promise.resolve({ providers: [], max_sessions: 2 } as AgentHostConfig),
          saveConfig: (config: AgentHostConfig) => {
            saved.push(config);
            return Promise.resolve(config);
          },
        },
      ),
    });

    const roster = await screen.findAllByRole('listitem');
    await user.click(within(roster[0]!).getByRole('button', { name: '启用' }));
    // Editing the manual recipe leaves the chosen row's hidden args behind.
    await user.clear(screen.getByLabelText('提供方 ID'));
    await user.type(screen.getByLabelText('提供方 ID'), 'custom-acp');
    await user.click(screen.getByRole('button', { name: '保存并重启检测' }));
    await waitFor(() => expect(saved.length).toBe(1));
    expect(saved[0]!.providers).toEqual([
      { id: 'custom-acp', protocol: 'acp', command: 'omp', args: [], enabled: true },
    ]);
  });

  it('keeps a configured provider selectable even without a scan row', async () => {
    const user = userEvent.setup();
    // gemini-style catalog-only target: no detection row, but the user has
    // it configured and enabled — it is the repair target, never disabled.
    const agents: AgentScanEntry[] = [
      { name: 'Alpha Agent', registry_agent_id: 'alpha-acp', launch_command: 'alpha-acp', launch_args: ['acp'], installed: true },
    ];
    renderSurface({
      setupCompleted: true,
      client: clientWith(
        () => Promise.resolve(ENGINE_DEGRADED),
        { workspace_initialized: true },
        UNSELECTED_PROFILE,
        {
          scan: () => Promise.resolve({ agents }),
          listProviders: () =>
            Promise.resolve({
              providers: [
                { provider_id: 'alpha-acp', display_name: 'Alpha Agent', protocol_kind: 'acp' },
                { provider_id: 'gemini', display_name: 'Gemini CLI', protocol_kind: 'acp' },
              ],
            }),
          getConfig: () =>
            Promise.resolve({
              providers: [
                { id: 'gemini', protocol: 'acp', command: 'gemini', enabled: true },
              ],
              max_sessions: 2,
            } as AgentHostConfig),
        },
      ),
    });

    const candidates = await screen.findAllByRole('listitem');
    expect(candidates[0]).toHaveTextContent('Alpha Agent');
    const geminiRow = candidates[1]!;
    expect(geminiRow).toHaveTextContent('已配置');

    const geminiUse = within(geminiRow).getByRole('button', { name: '启用' });
    expect(geminiUse).toBeEnabled();
    await user.click(geminiUse);
    expect(screen.getByLabelText('提供方 ID')).toHaveValue('gemini');
    // Regression lock for the configured-prefill fix: the stored command
    // must back the form (never blanked), and the saved payload keeps it.
    expect(screen.getByLabelText('启动命令')).toHaveValue('gemini');
  });

  it('renders the full roster and disables not-installed candidates', async () => {
    const user = userEvent.setup();
    const agents: AgentScanEntry[] = [
      { name: 'Alpha Agent', registry_agent_id: 'alpha-acp', launch_command: 'alpha-acp', installed: true },
      { name: 'Beta CLI', registry_agent_id: 'beta-cli', installed: false },
    ];
    renderSurface({
      setupCompleted: true,
      client: clientWith(
        () => Promise.resolve(ENGINE_DEGRADED),
        { workspace_initialized: true },
        UNSELECTED_PROFILE,
        {
          scan: () => Promise.resolve({ agents }),
          listProviders: () =>
            Promise.resolve({
              providers: [
                { provider_id: 'alpha-acp', display_name: 'Alpha Agent', protocol_kind: 'acp' },
                { provider_id: 'dsh-native', display_name: 'dsh (native CLI)', protocol_kind: 'native_cli' },
              ],
            }),
        },
      ),
    });

    // Roster order: installed first; the not-installed row stays visible.
    const candidates = await screen.findAllByRole('listitem');
    expect(candidates[0]).toHaveTextContent('Alpha Agent');
    expect(candidates[1]).toHaveTextContent('dsh (native CLI)');
    expect(candidates[1]).toHaveTextContent('未安装');

    const betaUse = within(candidates[1]!).getByRole('button', { name: '启用' });
    expect(betaUse).toBeDisabled();

    // The installed row stays selectable.
    await user.click(within(candidates[0]!).getByRole('button', { name: '启用' }));
    expect(screen.getByLabelText('提供方 ID')).toHaveValue('alpha-acp');
  });

  it('offers installed agents from the scan alone on an unconfigured home', async () => {
    const user = userEvent.setup();
    const agents: AgentScanEntry[] = [
      {
        name: 'Claude Code',
        registry_agent_id: 'claude-native',
        launch_command: '/opt/homebrew/bin/claude',
        installed: true,
        protocol_kind: 'native_cli',
      },
    ];
    renderSurface({
      setupCompleted: true,
      client: clientWith(
        () => Promise.resolve(ENGINE_DEGRADED),
        { workspace_initialized: true },
        UNSELECTED_PROFILE,
        { scan: () => Promise.resolve({ agents }) },
      ),
    });

    // The catalog is empty on an unconfigured home; the scan is the source.
    const candidates = await screen.findAllByRole('listitem');
    expect(candidates).toHaveLength(1);
    expect(candidates[0]).toHaveTextContent('Claude Code');
    expect(candidates[0]).toHaveTextContent('已安装');

    await user.click(within(candidates[0]!).getByRole('button', { name: '启用' }));
    expect(screen.getByLabelText('提供方 ID')).toHaveValue('claude-native');
    expect(screen.getByLabelText('协议')).toHaveValue('native_cli');
    expect(screen.getByLabelText('启动命令')).toHaveValue('/opt/homebrew/bin/claude');
    expect(screen.getByRole('button', { name: '保存并重启检测' })).toBeEnabled();
  });

  it('saves the upserted provider and re-runs the readiness probe', async () => {
    const user = userEvent.setup();
    const daemonStatus = vi.fn(() => Promise.resolve(ENGINE_DEGRADED));
    const saved = { providers: [], max_sessions: 2 } as AgentHostConfig;
    const saveConfig = vi.fn((config: AgentHostConfig) => {
      Object.assign(saved, config);
      return Promise.resolve(config);
    });
    renderSurface({
      setupCompleted: true,
      client: clientWith(
        daemonStatus,
        { workspace_initialized: true },
        UNSELECTED_PROFILE,
        { getConfig: () => Promise.resolve(saved), saveConfig },
      ),
    });

    await screen.findByLabelText('提供方 ID');
    await user.type(screen.getByLabelText('提供方 ID'), 'manual-agent');
    await user.selectOptions(screen.getByLabelText('协议'), 'native_cli');
    await user.type(screen.getByLabelText('启动命令'), '/usr/local/bin/manual-agent');
    await user.click(screen.getByRole('button', { name: '保存并重启检测' }));

    await waitFor(() => expect(saveConfig).toHaveBeenCalledTimes(1));
    const written = saveConfig.mock.calls[0]![0];
    expect(written.max_sessions).toBe(2);
    expect(written.providers).toEqual([
      { id: 'manual-agent', protocol: 'native_cli', command: '/usr/local/bin/manual-agent', args: [], enabled: true },
    ]);
    // A successful save re-runs the same probe the recheck button uses.
    await waitFor(() =>
      expect(daemonStatus.mock.calls.length).toBeGreaterThanOrEqual(2),
    );
  });

  it('disables every other provider when saving a replacement provider', async () => {
    const user = userEvent.setup();
    const saveConfig = vi.fn((config: AgentHostConfig) => Promise.resolve(config));
    const current: AgentHostConfig = {
      max_sessions: 2,
      providers: [
        { id: 'stale-acp', protocol: 'acp', command: 'stale-agent', enabled: true },
      ],
    };
    renderSurface({
      setupCompleted: true,
      client: clientWith(
        () => Promise.resolve(ENGINE_DEGRADED),
        { workspace_initialized: true },
        UNSELECTED_PROFILE,
        { getConfig: () => Promise.resolve(current), saveConfig },
      ),
    });

    await screen.findByLabelText('提供方 ID');
    await user.type(screen.getByLabelText('提供方 ID'), 'fresh-acp');
    await user.type(screen.getByLabelText('启动命令'), 'fresh-agent');
    await user.click(screen.getByRole('button', { name: '保存并重启检测' }));

    await waitFor(() => expect(saveConfig).toHaveBeenCalledTimes(1));
    const written = saveConfig.mock.calls[0]![0];
    expect(written.max_sessions).toBe(2);
    expect(written.providers).toEqual([
      { id: 'stale-acp', protocol: 'acp', command: 'stale-agent', enabled: false },
      { id: 'fresh-acp', protocol: 'acp', command: 'fresh-agent', args: [], enabled: true },
    ]);
  });

  it('keeps the catalog candidates when the agent scan fails', async () => {
    const user = userEvent.setup();
    const saveConfig = vi.fn((config: AgentHostConfig) => Promise.resolve(config));
    renderSurface({
      setupCompleted: true,
      client: clientWith(
        () => Promise.resolve(ENGINE_DEGRADED),
        { workspace_initialized: true },
        UNSELECTED_PROFILE,
        {
          scan: () => Promise.reject(new Error('scan offline')),
          listProviders: () =>
            Promise.resolve({
              providers: [
                { provider_id: 'alpha-acp', display_name: 'Alpha Agent', protocol_kind: 'acp' },
              ],
            }),
          saveConfig,
        },
      ),
    });

    const candidates = await screen.findAllByRole('listitem');
    expect(candidates).toHaveLength(1);
    expect(candidates[0]).toHaveTextContent('Alpha Agent');
    expect(screen.queryByText('已安装')).not.toBeInTheDocument();
    expect(screen.queryByTestId('provider-catalog-failed')).not.toBeInTheDocument();

    await user.click(within(candidates[0]!).getByRole('button', { name: '启用' }));
    expect(screen.getByLabelText('提供方 ID')).toHaveValue('alpha-acp');
    await user.type(screen.getByLabelText('启动命令'), 'alpha-acp');
    await user.click(screen.getByRole('button', { name: '保存并重启检测' }));
    await waitFor(() => expect(saveConfig).toHaveBeenCalledTimes(1));
  });

  it('shows the catalog error and keeps the manual path usable', async () => {
    const user = userEvent.setup();
    const saveConfig = vi.fn((config: AgentHostConfig) => Promise.resolve(config));
    renderSurface({
      setupCompleted: true,
      client: clientWith(
        () => Promise.resolve(ENGINE_DEGRADED),
        { workspace_initialized: true },
        UNSELECTED_PROFILE,
        {
          listProviders: () => Promise.reject(new Error('catalog offline')),
          saveConfig,
        },
      ),
    });

    expect(await screen.findByTestId('provider-catalog-failed')).toHaveTextContent(
      '无法读取本机的智能体列表',
    );
    await user.type(screen.getByLabelText('提供方 ID'), 'manual-agent');
    await user.type(screen.getByLabelText('启动命令'), 'manual-agent');
    await user.click(screen.getByRole('button', { name: '保存并重启检测' }));
    await waitFor(() => expect(saveConfig).toHaveBeenCalledTimes(1));
  });

  it('adds the failure note when a selected provider is not ready', async () => {
    renderSurface({
      setupCompleted: true,
      client: clientWith(
        () => Promise.resolve(ENGINE_DEGRADED),
        { workspace_initialized: true },
        SELECTED_PROFILE,
      ),
    });

    const surface = await screen.findByTestId('service-degraded-surface');
    expect(surface).toHaveAttribute('data-variant', 'full');
    expect(surface).toHaveAttribute('data-state', 'failed');
    expect(screen.getByTestId('engine-repair-failure')).toHaveTextContent(
      '已选择编排提供方，但就绪检测未通过：编排引擎未就绪。',
    );
    // The failure note sits above the same configuration form: the saved
    // profile alone cannot make the recheck succeed.
    expect(screen.getByLabelText('提供方 ID')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '保存并重启检测' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '重新检测' })).toBeInTheDocument();
  });

  it('keeps the failure repair page while the profile selection is still loading', async () => {
    const profile = withResolvers<{ profile: { name: string } | null }>();
    const getAgentProfile = vi.fn(() => profile.promise);
    renderSurface({
      setupCompleted: true,
      client: clientWith(
        () => Promise.resolve(ENGINE_DEGRADED),
        { workspace_initialized: true },
        getAgentProfile,
      ),
    });

    await waitFor(() => expect(getAgentProfile).toHaveBeenCalledTimes(1));
    // Never a flash of setup guidance while the selection read is in flight.
    expect(screen.getByTestId('service-degraded-surface')).toHaveAttribute(
      'data-state',
      'failed',
    );
    expect(screen.getByTestId('engine-repair-failure')).toBeInTheDocument();

    await act(async () => {
      profile.resolve({ profile: null });
    });
    await waitFor(() =>
      expect(screen.getByTestId('service-degraded-surface')).toHaveAttribute(
        'data-state',
        'unconfigured',
      ),
    );
    expect(screen.queryByTestId('engine-repair-failure')).not.toBeInTheDocument();
  });

  it('ignores an uninitialized service so onboarding stays reachable', async () => {
    const daemonStatus = vi.fn(() =>
      Promise.resolve({
        degraded: { subsystems: ['db', 'engine'], reasons: ['db not ready', 'engine not ready'] },
      }),
    );
    renderSurface({ client: clientWith(daemonStatus), setupCompleted: false });

    await waitFor(() => expect(daemonStatus).toHaveBeenCalled());
    expect(screen.getByTestId('app-content')).toBeInTheDocument();
    expect(screen.queryByTestId('service-degraded-surface')).not.toBeInTheDocument();
  });

  it('takes over the screen when the first-run probe cannot reach the service', async () => {
    renderSurface({
      setupCompleted: false,
      client: clientWith(() => Promise.reject(new TypeError('Failed to fetch'))),
    });

    const surface = await screen.findByTestId('service-degraded-surface');
    expect(surface).toHaveAttribute('data-variant', 'full');
    // The raw fetch message is not copy: transport failures get a fixed reason.
    expect(surface).toHaveTextContent(
      'Nexus 服务暂不可用：无法连接本地服务。请确认本地服务正在运行后重试。',
    );
    expect(screen.queryByTestId('app-content')).not.toBeInTheDocument();
  });

  it('keeps a server-provided error message as the reason', async () => {
    renderSurface({
      setupCompleted: true,
      client: clientWith(() =>
        Promise.reject(new NexusClientError(503, 'unavailable', 'engine still starting')),
      ),
    });

    const surface = await screen.findByTestId('service-degraded-surface');
    expect(surface).toHaveTextContent(
      'Nexus 服务暂不可用：engine still starting。请确认本地服务正在运行后重试。',
    );
  });

  it('takes the full screen on the wizard route even after the gate resolved', async () => {
    renderSurface({
      setupCompleted: true,
      entries: ['/setup'],
      client: clientWith(() => Promise.reject(new TypeError('Failed to fetch'))),
    });

    const surface = await screen.findByTestId('service-degraded-surface');
    expect(surface).toHaveAttribute('data-variant', 'full');
  });

  it('replaces the banner with the repair page when the retry reports the engine', async () => {
    const user = userEvent.setup();
    const daemonStatus = vi
      .fn()
      .mockRejectedValueOnce(new TypeError('Failed to fetch'))
      .mockResolvedValue(ENGINE_DEGRADED);
    renderSurface({ setupCompleted: true, client: clientWith(daemonStatus) });

    const first = await screen.findByTestId('service-degraded-surface');
    expect(first).toHaveAttribute('data-variant', 'banner');
    expect(first).toHaveTextContent('无法连接本地服务');

    await user.click(screen.getByRole('button', { name: '重试' }));

    await waitFor(() =>
      expect(screen.getByTestId('service-degraded-surface')).toHaveAttribute(
        'data-variant',
        'full',
      ),
    );
    expect(screen.getByTestId('service-degraded-surface')).toHaveTextContent('编排引擎未就绪');
    // One instance throughout: the retry updates the existing surface.
    expect(screen.getAllByTestId('service-degraded-surface')).toHaveLength(1);
    expect(screen.queryByTestId('app-content')).not.toBeInTheDocument();
    expect(daemonStatus.mock.calls.length).toBeGreaterThanOrEqual(2);
  });

  it('ignores a stale probe that resolves after a newer probe', async () => {
    const deferreds: Array<Deferred<DaemonRuntimeStatus>> = [];
    const daemonStatus = vi.fn(() => {
      const deferred = withResolvers<DaemonRuntimeStatus>();
      deferreds.push(deferred);
      return deferred.promise;
    });
    // The first-run gate resolves after mount, which re-runs the probe — two
    // probes are then in flight at once.
    const client = {
      daemonStatus,
      runtimeStatus: () => Promise.resolve({ workspace_initialized: true }),
    } as unknown as NexusClient;
    renderSurface({ client });

    await waitFor(() => expect(daemonStatus).toHaveBeenCalledTimes(2));

    // The newest probe resolves healthy first; the stale probe must not
    // overwrite that outcome when it settles later.
    await act(async () => {
      deferreds[1]!.resolve(HEALTHY);
    });
    expect(screen.queryByTestId('service-degraded-surface')).not.toBeInTheDocument();

    await act(async () => {
      deferreds[0]!.resolve(ENGINE_DEGRADED);
    });
    expect(screen.queryByTestId('service-degraded-surface')).not.toBeInTheDocument();
  });

  it('renders the EN repair copy', async () => {
    window.localStorage.setItem('nexus-web-locale', 'en');
    renderSurface({
      setupCompleted: true,
      client: clientWith(
        () => Promise.resolve(ENGINE_DEGRADED),
        { workspace_initialized: true },
        UNSELECTED_PROFILE,
      ),
    });

    const surface = await screen.findByTestId('service-degraded-surface');
    expect(surface).toHaveTextContent('Orchestration engine is not ready');
    expect(surface).toHaveTextContent('Configure an orchestration provider');
    expect(surface).not.toHaveTextContent('nexus42 connect');
    expect(await screen.findByLabelText('Provider ID')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Save and recheck' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Recheck' })).toBeInTheDocument();
  });
});
