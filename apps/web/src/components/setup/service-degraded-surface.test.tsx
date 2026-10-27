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
import { act, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import { ServiceDegradedBanner, ServiceDegradedGate } from './service-degraded-surface';
import { ServiceStatusProvider } from '@/lib/service-status-context';
import { renderInApp } from '@/test/test-providers';
import { NexusClientError } from '@/lib/nexus';
import type { DaemonRuntimeStatus, NexusClient, RuntimeStatus } from '@/lib/nexus';

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

function clientWith(
  read: () => Promise<DaemonRuntimeStatus>,
  runtime: RuntimeStatus = { workspace_initialized: false },
  profile: ProfileRead = SELECTED_PROFILE,
): NexusClient {
  return {
    daemonStatus: read,
    runtimeStatus: () => Promise.resolve(runtime),
    getAgentProfile: profile,
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

  it('takes over every route with the repair page when no provider is selected', async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    const user = userEvent.setup();
    Object.defineProperty(navigator, 'clipboard', {
      value: { writeText },
      configurable: true,
    });
    renderSurface({
      setupCompleted: true,
      client: clientWith(
        () => Promise.resolve(ENGINE_DEGRADED),
        { workspace_initialized: true },
        UNSELECTED_PROFILE,
      ),
    });

    const surface = await screen.findByTestId('service-degraded-surface');
    expect(surface).toHaveAttribute('data-variant', 'full');
    expect(surface).toHaveAttribute('data-state', 'unconfigured');
    expect(surface).toHaveTextContent('编排引擎未就绪');
    expect(surface).toHaveTextContent('配置编排提供方');
    expect(surface).toHaveTextContent('nexus42 connect');
    expect(screen.queryByTestId('engine-repair-failure')).not.toBeInTheDocument();
    // The app is not usable until the engine is configured.
    expect(screen.queryByTestId('app-content')).not.toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: '复制' }));
    expect(writeText).toHaveBeenCalledWith('nexus42 connect');
    expect(screen.getByRole('button', { name: '已复制' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '重新检测' })).toBeInTheDocument();
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
      client: clientWith(() => Promise.resolve(ENGINE_DEGRADED)),
    });

    const surface = await screen.findByTestId('service-degraded-surface');
    expect(surface).toHaveTextContent('Orchestration engine is not ready');
    expect(surface).toHaveTextContent('nexus42 connect');
    expect(screen.getByRole('button', { name: 'Recheck' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Copy' })).toBeInTheDocument();
  });
});
