/**
 * Harness catalog surface (plan v1220 T2).
 *
 * Live read-only aggregation over existing daemon endpoints: preset rows with
 * list facts, strategy binding (`states.length > 0`), per-preset running-session
 * counts, and the single explicit loading / error / empty states. The profile
 * bar has its own query state, so a catalog failure never blanks it.
 */
import { beforeEach, describe, expect, it } from 'vitest';
import { delay, http, HttpResponse } from 'msw';
import { screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useLocation } from 'react-router';

import { HarnessPage } from '@/components/harness/harness-page';
import { i18n } from '@/lib/i18n/config';
import { BrowserClient } from '@/lib/nexus';
import { useHandlers } from '@/test/msw-server';
import { renderInApp } from '@/test/test-providers';

function presetBody() {
  return {
    user: [{ id: 'user/one', source: 'user', run_intents: ['write'] }],
    embedded: [{ id: 'embedded/two', source: 'embedded' }],
    system: [
      { id: 'sys/three', source: 'system' },
      { id: '_system.hidden', source: 'system' },
    ],
  };
}

function profileHandler(id: string, states: unknown[]) {
  return http.get(
    `/v1/daemon/orchestration/presets/${encodeURIComponent(id)}/profile`,
    () =>
      HttpResponse.json({
        id,
        version: 1,
        sourceHash: 'a'.repeat(64),
        lanes: { cron: false, wallClock: true, session: true, direct: true },
        states,
      }),
  );
}

function session(presetId: string, status: string) {
  return {
    session_id: `${presetId}-${status}-${Math.random()}`,
    creator_id: 'creator-a',
    preset_id: presetId,
    status,
  };
}

function sessionsHandler(items: unknown[]) {
  return http.get('/v1/daemon/orchestration/sessions', () =>
    HttpResponse.json({
      items,
      pagination: { limit: 100, has_more: false, next_cursor: null },
    }),
  );
}

function LocationProbe() {
  const location = useLocation();
  return <span data-testid="location">{location.pathname}</span>;
}

function renderHarness() {
  return renderInApp(
    <>
      <HarnessPage />
      <LocationProbe />
    </>,
    { client: new BrowserClient(), activeCreatorId: 'creator-a', initialRouterEntries: ['/strategies'] },
  );
}

beforeEach(async () => {
  window.localStorage.clear();
  // LocaleProvider resolves from this preference; without it jsdom's en-US wins.
  window.localStorage.setItem('nexus-web-locale', 'zh-CN');
  await i18n.changeLanguage('zh-CN');
});

describe('HarnessPage', () => {
  it('lists presets with facts, binding state, and running-session state', async () => {
    useHandlers(
      http.get('/v1/daemon/presets', () => HttpResponse.json(presetBody())),
      profileHandler('user/one', [{ id: 's1', terminal: true }]),
      profileHandler('embedded/two', []),
      profileHandler('sys/three', [{ id: 'a', terminal: false }]),
      sessionsHandler([
        session('user/one', 'running'),
        session('user/one', 'waiting_for_input'),
        session('embedded/two', 'completed'),
      ]),
      http.get('/v1/daemon/agent-host/profile', () =>
        HttpResponse.json({ profile: { name: 'claude-code' } }),
      ),
    );

    renderHarness();

    expect(await screen.findByTestId('preset-row-user/one')).toBeInTheDocument();
    // `_system.*` internals are hidden; user/embedded/system rows stay.
    expect(screen.queryByTestId('preset-row-_system.hidden')).not.toBeInTheDocument();
    expect(screen.getByTestId('preset-row-embedded/two')).toBeInTheDocument();
    expect(screen.getByTestId('preset-row-sys/three')).toBeInTheDocument();

    expect(screen.getByTestId('preset-facts-user/one')).toHaveTextContent('用户 · write');
    expect(await screen.findByTestId('preset-binding-user/one')).toHaveTextContent(
      'strategy 已绑定',
    );
    expect(screen.getByTestId('preset-binding-embedded/two')).toHaveTextContent('未绑定');
    expect(screen.getByTestId('preset-running-user/one')).toHaveTextContent('running ×2');
    expect(screen.getByTestId('preset-running-embedded/two')).toHaveTextContent('idle');

    // The profile bar reads the same service profile.
    expect(await screen.findByTestId('harness-profile-current')).toHaveTextContent(
      '当前：claude-code',
    );

    // Canvas and profile drill-downs stay reachable from a row.
    await userEvent.click(screen.getByRole('button', { name: '在 canvas 中打开 user/one' }));
    expect(screen.getByTestId('location')).toHaveTextContent('/strategies/user%2Fone');
  });

  it('shows a single loading state while presets load', async () => {
    useHandlers(
      http.get('/v1/daemon/presets', async () => {
        await delay(50);
        return HttpResponse.json(presetBody());
      }),
      sessionsHandler([]),
    );

    renderHarness();

    expect(screen.getByTestId('harness-catalog-loading')).toHaveTextContent('正在加载…');
    expect(await screen.findByTestId('preset-row-user/one')).toBeInTheDocument();
  });

  it('renders the preset error inline and retries it without hiding the profile bar', async () => {
    let calls = 0;
    useHandlers(
      http.get('/v1/daemon/presets', () => {
        calls += 1;
        return calls === 1
          ? HttpResponse.json(
              { success: false, error: { code: 'internal', message: 'preset store offline' } },
              { status: 500 },
            )
          : HttpResponse.json(presetBody());
      }),
      profileHandler('user/one', []),
      profileHandler('embedded/two', []),
      profileHandler('sys/three', []),
      sessionsHandler([]),
      http.get('/v1/daemon/agent-host/profile', () =>
        HttpResponse.json({ profile: { name: 'claude-code' } }),
      ),
    );

    renderHarness();

    const error = await screen.findByTestId('harness-catalog-error');
    expect(error).toHaveTextContent('加载失败：preset store offline');
    // The profile bar is a separate source and keeps rendering.
    expect(await screen.findByTestId('harness-profile-current')).toHaveTextContent(
      '当前：claude-code',
    );

    await userEvent.click(screen.getByRole('button', { name: '重试' }));
    expect(await screen.findByTestId('preset-row-user/one')).toBeInTheDocument();
  });

  it('shows the empty state when no presets exist', async () => {
    useHandlers(
      http.get('/v1/daemon/presets', () =>
        HttpResponse.json({ user: [], embedded: [], system: [] }),
      ),
      sessionsHandler([]),
    );

    renderHarness();

    expect(await screen.findByText('暂无 preset')).toBeInTheDocument();
  });

  it('keeps the catalog rows when only the session aggregate fails', async () => {
    useHandlers(
      http.get('/v1/daemon/presets', () => HttpResponse.json(presetBody())),
      profileHandler('user/one', []),
      profileHandler('embedded/two', []),
      profileHandler('sys/three', []),
      http.get('/v1/daemon/orchestration/sessions', () =>
        HttpResponse.json(
          { success: false, error: { code: 'internal', message: 'sessions offline' } },
          { status: 500 },
        ),
      ),
    );

    renderHarness();

    expect(await screen.findByTestId('preset-row-user/one')).toBeInTheDocument();
    expect(screen.getByTestId('preset-running-user/one')).toHaveTextContent('idle');
  });
});
