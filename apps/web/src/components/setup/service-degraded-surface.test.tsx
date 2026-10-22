/**
 * Service-unavailable surface (plan v1219-onboarding T4).
 *
 * The probe is `GET /v1/daemon/daemon/status`; one instance drives two
 * placements — full-screen over the wizard, a top banner inside the shell.
 * Copy is localized (`setup:degraded.*`); these cases pin the zh-CN product
 * copy and the EN fallback.
 */
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import { ServiceDegradedBanner, ServiceDegradedGate } from './service-degraded-surface';
import { ServiceStatusProvider } from '@/lib/service-status-context';
import { renderInApp } from '@/test/test-providers';
import { NexusClientError } from '@/lib/nexus';
import type { DaemonRuntimeStatus, NexusClient } from '@/lib/nexus';

function clientWith(read: () => Promise<DaemonRuntimeStatus>): NexusClient {
  return { daemonStatus: read } as unknown as NexusClient;
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
