/**
 * Hub status cards (plan v1220 T2) — live data per card, read-only surface,
 * and per-card loading / error isolation (`正在加载…` / `加载失败：{reason}`).
 */
import { beforeEach, describe, expect, it } from 'vitest';
import { delay, http, HttpResponse } from 'msw';
import { screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import { HubCards } from '@/components/developer/hub-cards';
import { i18n } from '@/lib/i18n/config';
import { BrowserClient } from '@/lib/nexus';
import { healthOk } from '@/test/handlers';
import { server, useHandlers } from '@/test/msw-server';
import { renderInApp } from '@/test/test-providers';

const CREATOR_ID = 'creator-a';

function session(sessionId: string, status: string) {
  return {
    session_id: sessionId,
    creator_id: CREATOR_ID,
    preset_id: 'narrative-weaver',
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
        pagination: { limit: 20, has_more: false },
      }),
    ),
    http.get('/v1/daemon/compute/modules', () => HttpResponse.json(MODULES_RESPONSE)),
  );
}

function renderHub() {
  return renderInApp(<HubCards />, {
    client: new BrowserClient(),
    activeCreatorId: CREATOR_ID,
  });
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
});
