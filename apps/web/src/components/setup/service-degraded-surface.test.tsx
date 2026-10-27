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

function clientWith(
  read: () => Promise<DaemonRuntimeStatus>,
  runtime: RuntimeStatus = { workspace_initialized: false },
): NexusClient {
  return {
    daemonStatus: read,
    runtimeStatus: () => Promise.resolve(runtime),
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

/** jsdom resolves the system locale to `en`; pin zh-CN to assert the product copy. */
beforeEach(() => window.localStorage.setItem('nexus-web-locale', 'zh-CN'));

describe('ServiceDegradedGate', () => {
  it('shows one banner inside the shell when a subsystem is degraded', async () => {
    renderSurface({
      setupCompleted: true,
      client: clientWith(() => Promise.resolve(ENGINE_DEGRADED)),
    });

    const surface = await screen.findByTestId('service-degraded-surface');
    expect(surface).toHaveAttribute('data-variant', 'banner');
    expect(surface).toHaveTextContent(
      'Nexus 服务暂不可用：编排引擎未就绪。请确认本地服务正在运行后重试。',
    );
    expect(screen.getByRole('button', { name: '重试' })).toBeInTheDocument();
    // The shell stays usable behind the single surface — no toasts.
    expect(screen.getByTestId('app-content')).toBeInTheDocument();
    expect(screen.getAllByRole('alert')).toHaveLength(1);
  });

  it('guides provider selection when the workspace is ready but no provider is selected', async () => {
    renderSurface({
      setupCompleted: true,
      client: clientWith(() => Promise.resolve(ENGINE_DEGRADED), {
        workspace_initialized: true,
      }),
    });

    const surface = await screen.findByTestId('service-degraded-surface');
    expect(surface).toHaveTextContent(
      '尚未选择编排引擎提供方 —— 在 Agent 设置中选择后即可启用运行能力。',
    );
    // A setup step, not a transport failure: no retry affordance.
    expect(screen.queryByRole('button', { name: '重试' })).not.toBeInTheDocument();
    expect(screen.getByTestId('app-content')).toBeInTheDocument();
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

  it('updates the same banner on retry instead of stacking surfaces', async () => {
    const user = userEvent.setup();
    const daemonStatus = vi
      .fn()
      .mockRejectedValueOnce(new TypeError('Failed to fetch'))
      .mockResolvedValue(ENGINE_DEGRADED);
    renderSurface({ setupCompleted: true, client: clientWith(daemonStatus) });

    const first = await screen.findByTestId('service-degraded-surface');
    expect(first).toHaveTextContent('无法连接本地服务');

    await user.click(screen.getByRole('button', { name: '重试' }));

    await waitFor(() =>
      expect(screen.getByTestId('service-degraded-surface')).toHaveTextContent('编排引擎未就绪'),
    );
    // One instance throughout: the retry updates the existing banner.
    expect(screen.getAllByTestId('service-degraded-surface')).toHaveLength(1);
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

  it('renders the EN fallback copy', async () => {
    window.localStorage.setItem('nexus-web-locale', 'en');
    renderSurface({
      setupCompleted: true,
      client: clientWith(() => Promise.resolve(ENGINE_DEGRADED)),
    });

    const surface = await screen.findByTestId('service-degraded-surface');
    expect(surface).toHaveTextContent(
      'Nexus service unavailable: the orchestration engine is not ready. Make sure the local service is running, then retry.',
    );
    expect(screen.getByRole('button', { name: 'Retry' })).toBeInTheDocument();
  });
});
