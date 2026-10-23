/**
 * Hub status cards (plan v1220 T2) — live data per card, read-only surface,
 * per-card loading / error isolation (`正在加载…` / `加载失败：{reason}`), and
 * the Agent card's saved-profile query state.
 */
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { delay, http, HttpResponse } from 'msw';
import { screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import { HubCards } from '@/components/developer/hub-cards';
import { i18n } from '@/lib/i18n/config';
import { BrowserClient } from '@/lib/nexus';
import type { DesktopCapabilities } from '@/lib/nexus/desktop-capabilities';
import { healthOk } from '@/test/handlers';
import { server, useHandlers } from '@/test/msw-server';
import { renderInApp } from '@/test/test-providers';

const CREATOR_ID = 'creator-a';

function session(
  sessionId: string,
  status: string,
  presetId = 'narrative-weaver',
): Record<string, unknown> {
  return {
    session_id: sessionId,
    creator_id: CREATOR_ID,
    preset_id: presetId,
    status,
  };
}

const MODULES_RESPONSE = {
  items: [
    {
      module_id: 'combat-engine',
      name: 'combat-engine',
      version: '1.0.0',
      required_key_block_types: [],
      status: 'ok',
    },
    {
      module_id: 'dice',
      name: 'dice',
      version: '1.0.0',
      required_key_block_types: [],
      status: 'ok',
    },
  ],
  has_more: false,
};

/** Every card's source — healthy and populated. */
function hubHandlers() {
  useHandlers(
    healthOk('0.1.0'),
    http.get('/v1/daemon/daemon/status', () =>
      HttpResponse.json({ degraded: { subsystems: [], reasons: [] } }),
    ),
    http.post('/v1/daemon/agent-host/scan', () =>
      HttpResponse.json({
        agents: [{ name: 'my-agent', installed: true, version: '1.2.3' }],
      }),
    ),
    http.get('/v1/daemon/presets', () =>
      HttpResponse.json({
        embedded: [{ id: 'embedded/a', source: 'embedded' }],
        system: [{ id: '_system.b', source: 'system' }],
        user: [{ id: 'user/c', source: 'user' }],
      }),
    ),
    http.get('/v1/daemon/orchestration/sessions', () =>
      HttpResponse.json({
        items: [
          session('s1', 'running'),
          session('s2', 'completed'),
          session('s3', 'waiting_for_input'),
        ],
        pagination: { limit: 100, has_more: false },
      }),
    ),
    http.get('/v1/daemon/compute/modules', () => HttpResponse.json(MODULES_RESPONSE)),
  );
}

function renderHub(options: { desktop?: DesktopCapabilities } = {}) {
  return renderInApp(<HubCards />, {
    client: new BrowserClient(),
    activeCreatorId: CREATOR_ID,
    desktop: options.desktop ?? null,
  });
}

/** Minimal desktop capabilities: only the saved-profile IPC seam is exercised. */
function profileDesktop(getAgentProfile: () => Promise<{ name: string } | null>) {
  return { getAgentProfile } as unknown as DesktopCapabilities;
}

describe('HubCards', () => {
  beforeEach(async () => {
    window.localStorage.clear();
    await i18n.changeLanguage('en');
    window.localStorage.setItem(
      `nexus-agent-profile:${CREATOR_ID}`,
      JSON.stringify({ name: 'my-agent' }),
    );
  });

  it('renders the four cards from live daemon data, navigation-only', async () => {
    hubHandlers();
    renderHub();

    expect(await screen.findByTestId('hub-card-service-version')).toHaveTextContent(
      'nexus-service v0.1.0',
    );
    expect(screen.getByTestId('hub-card-service-url')).toHaveTextContent(
      window.location.origin,
    );
    expect(screen.getByTestId('hub-card-service-subsystems')).toHaveTextContent(
      'db / engine OK',
    );

    expect(await screen.findByTestId('hub-card-agent-name')).toHaveTextContent('my-agent');
    expect(screen.getByTestId('hub-card-agent-version')).toHaveTextContent('1.2.3');
    expect(screen.getByTestId('hub-card-agent')).toHaveTextContent('Connected');

    // 1 embedded + 1 system + 1 user preset; 2 live sessions (`completed` is terminal).
    expect(await screen.findByTestId('hub-card-harness-counts')).toHaveTextContent(
      '3 presets · 2 running',
    );

    expect(await screen.findByTestId('hub-card-modules-count')).toHaveTextContent(
      '2 compute modules',
    );
    expect(screen.getByTestId('hub-card-modules-list')).toHaveTextContent(
      'combat-engine · dice',
    );

    // Cards navigate only — the links go to the owning rail apps.
    expect(screen.getByTestId('hub-card-agent-go')).toHaveAttribute('href', '/strategies');
    expect(screen.getByTestId('hub-card-harness-go')).toHaveAttribute('href', '/strategies');
    expect(screen.getByTestId('hub-card-modules-go')).toHaveAttribute('href', '/modules');
    expect(screen.queryAllByRole('textbox')).toHaveLength(0);
    expect(screen.queryAllByRole('button')).toHaveLength(0);
  });

  it('shows the degraded service pill and the degraded subsystems', async () => {
    hubHandlers();
    useHandlers(
      http.get('/v1/daemon/daemon/status', () =>
        HttpResponse.json({
          degraded: { subsystems: ['db', 'engine'], reasons: ['db locked', 'engine busy'] },
        }),
      ),
    );
    renderHub();

    expect(await screen.findByTestId('hub-card-service-subsystems')).toHaveTextContent(
      'Degraded: db / engine',
    );
    expect(screen.getByTestId('hub-card-service')).toHaveTextContent('Degraded');
  });

  it('shows the loading copy while a source is pending', async () => {
    hubHandlers();
    useHandlers(
      http.get('/v1/daemon/runtime/health', async () => {
        await delay(50);
        return HttpResponse.json({ status: 'ok', version: '0.1.0' });
      }),
    );
    renderHub();

    expect(screen.getByTestId('hub-card-service-loading')).toHaveTextContent('Loading…');
    expect(await screen.findByTestId('hub-card-service-version')).toBeInTheDocument();
  });

  it('isolates a failing source to its own card and retries it', async () => {
    hubHandlers();
    useHandlers(
      http.get('/v1/daemon/compute/modules', () =>
        HttpResponse.json(
          { success: false, error: { code: 'internal', message: 'registry offline' } },
          { status: 500 },
        ),
      ),
    );
    renderHub();

    const error = await screen.findByTestId('hub-card-modules-error');
    expect(error).toHaveTextContent('Could not load: registry offline');
    expect(screen.queryByTestId('hub-card-modules-count')).not.toBeInTheDocument();

    // The other three cards still render their live data.
    expect(await screen.findByTestId('hub-card-service-version')).toBeInTheDocument();
    expect(screen.getByTestId('hub-card-harness-counts')).toBeInTheDocument();
    expect(screen.getByTestId('hub-card-agent-name')).toBeInTheDocument();

    // Retry re-runs the failing source and recovers the card.
    server.use(
      http.get('/v1/daemon/compute/modules', () => HttpResponse.json(MODULES_RESPONSE)),
    );
    await userEvent.click(screen.getByRole('button', { name: 'Try again' }));
    expect(await screen.findByTestId('hub-card-modules-count')).toHaveTextContent(
      '2 compute modules',
    );
  });

  it('counts running sessions across cursor pages', async () => {
    hubHandlers();
    const requestedCursors: Array<string | null> = [];
    useHandlers(
      http.get('/v1/daemon/orchestration/sessions', ({ request }) => {
        const cursor = new URL(request.url).searchParams.get('cursor');
        requestedCursors.push(cursor);
        if (cursor === null) {
          return HttpResponse.json({
            items: [session('s1', 'completed'), session('s2', 'failed')],
            pagination: { limit: 100, has_more: true, next_cursor: 'page-2' },
          });
        }
        return HttpResponse.json({
          items: [
            session('s3', 'running'),
            // `_system.*` sessions stay excluded by the shared visibility filter.
            session('s4', 'running', '_system.boot'),
            session('s5', 'cancelled'),
          ],
          pagination: { limit: 100, has_more: false },
        });
      }),
    );
    renderHub();

    expect(await screen.findByTestId('hub-card-harness-counts')).toHaveTextContent(
      '3 presets · 1 running',
    );
    expect(requestedCursors).toEqual([null, 'page-2']);
  });

  it('shows the unconfigured Agent state when no profile exists', async () => {
    hubHandlers();
    window.localStorage.clear();
    renderHub();

    expect(await screen.findByTestId('hub-card-agent-name')).toHaveTextContent(
      'No agent selected',
    );
    expect(screen.getByTestId('hub-card-agent')).toHaveTextContent('Not connected');
  });

  it('keeps the Agent card loading while the profile read is pending', async () => {
    hubHandlers();
    // `Promise.withResolvers` is ES2024; this app's TS lib is ES2022.
    const getAgentProfile = vi.fn(
      () =>
        new Promise<{ name: string }>((resolve) => {
          setTimeout(() => resolve({ name: 'my-agent' }), 50);
        }),
    );
    renderHub({ desktop: profileDesktop(getAgentProfile) });

    expect(screen.getByTestId('hub-card-agent-loading')).toHaveTextContent('Loading…');
    expect(await screen.findByTestId('hub-card-agent-name')).toHaveTextContent('my-agent');
  });

  it('surfaces a failed profile read and retries it without touching the scan', async () => {
    hubHandlers();
    const getAgentProfile = vi
      .fn()
      .mockRejectedValueOnce(new Error('profile ipc offline'))
      .mockResolvedValue({ name: 'my-agent' });
    renderHub({ desktop: profileDesktop(getAgentProfile) });

    const error = await screen.findByTestId('hub-card-agent-error');
    expect(error).toHaveTextContent('Could not load: profile ipc offline');
    expect(screen.queryByTestId('hub-card-agent-name')).not.toBeInTheDocument();

    // Per-card isolation: the other sources are untouched.
    expect(await screen.findByTestId('hub-card-service-version')).toBeInTheDocument();
    expect(screen.getByTestId('hub-card-harness-counts')).toBeInTheDocument();
    expect(screen.getByTestId('hub-card-modules-count')).toBeInTheDocument();

    // Retry re-runs the profile query and recovers the card.
    await userEvent.click(screen.getByRole('button', { name: 'Try again' }));
    expect(await screen.findByTestId('hub-card-agent-name')).toHaveTextContent('my-agent');
    expect(screen.getByTestId('hub-card-agent')).toHaveTextContent('Connected');
    expect(getAgentProfile).toHaveBeenCalledTimes(2);
  });
});
