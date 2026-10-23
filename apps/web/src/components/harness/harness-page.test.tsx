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
import { screen, waitFor, within } from '@testing-library/react';
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

/** Always-more pages: exercises the bounded walk's truncated (lower-bound) path. */
function truncatedSessionsHandler(items: unknown[]) {
  return http.get('/v1/daemon/orchestration/sessions', () =>
    HttpResponse.json({
      items,
      pagination: { limit: 100, has_more: true, next_cursor: 'more' },
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

  it('marks a failed profile read as unknown binding, never "not bound"', async () => {
    useHandlers(
      http.get('/v1/daemon/presets', () => HttpResponse.json(presetBody())),
      http.get(/\/v1\/daemon\/orchestration\/presets\/.+\/profile$/, () =>
        HttpResponse.json(
          { success: false, error: { code: 'internal', message: 'profile offline' } },
          { status: 500 },
        ),
      ),
      sessionsHandler([]),
    );

    renderHarness();

    expect(await screen.findByTestId('preset-binding-user/one')).toHaveTextContent(
      '绑定状态未知',
    );
    expect(screen.getByTestId('preset-binding-user/one')).not.toHaveTextContent('未绑定');
  });

  it('reloads the row so manifest changes reach the catalog', async () => {
    let listCalls = 0;
    useHandlers(
      http.get('/v1/daemon/presets', () => {
        listCalls += 1;
        // The reload re-reads the manifest: the refetched catalog carries the
        // new list facts, which is what the row must show.
        return HttpResponse.json(
          listCalls === 1
            ? presetBody()
            : {
                ...presetBody(),
                user: [{ id: 'user/one', source: 'user', run_intents: ['review'] }],
              },
        );
      }),
      profileHandler('user/one', []),
      profileHandler('embedded/two', []),
      profileHandler('sys/three', []),
      sessionsHandler([]),
      http.post(/\/v1\/daemon\/presets\/.*:reload$/, () =>
        HttpResponse.json({ preset_id: 'user/one', states: [] }),
      ),
    );

    renderHarness();
    await screen.findByTestId('preset-row-user/one');
    expect(screen.getByTestId('preset-facts-user/one')).toHaveTextContent('用户 · write');

    await userEvent.click(screen.getByTestId('preset-reload-user/one'));

    await waitFor(() =>
      expect(screen.getByTestId('preset-facts-user/one')).toHaveTextContent('用户 · review'),
    );
  });

  it('deletes a user preset and the row leaves the catalog', async () => {
    let deleted = false;
    useHandlers(
      http.get('/v1/daemon/presets', () =>
        HttpResponse.json(deleted ? { ...presetBody(), user: [] } : presetBody()),
      ),
      profileHandler('user/one', []),
      profileHandler('embedded/two', []),
      profileHandler('sys/three', []),
      sessionsHandler([]),
      http.delete(/\/v1\/daemon\/presets\/.*$/, () => {
        deleted = true;
        return new HttpResponse(null, { status: 204 });
      }),
    );

    renderHarness();
    await screen.findByTestId('preset-row-user/one');

    // Delete is offered on user presets only.
    expect(screen.getByTestId('preset-delete-user/one')).toBeInTheDocument();
    expect(screen.queryByTestId('preset-delete-embedded/two')).not.toBeInTheDocument();
    expect(screen.queryByTestId('preset-delete-sys/three')).not.toBeInTheDocument();

    await userEvent.click(screen.getByTestId('preset-delete-user/one'));
    const confirm = await screen.findByRole('dialog');
    await userEvent.click(within(confirm).getByRole('button', { name: '删除' }));

    await waitFor(() =>
      expect(screen.queryByTestId('preset-row-user/one')).not.toBeInTheDocument(),
    );
    // The rest of the catalog survives the delete.
    expect(screen.getByTestId('preset-row-embedded/two')).toBeInTheDocument();
    expect(screen.getByTestId('preset-row-sys/three')).toBeInTheDocument();
  });

  it('validates the row it was opened from', async () => {
    const validatedPaths: unknown[] = [];
    useHandlers(
      http.get('/v1/daemon/presets', () => HttpResponse.json(presetBody())),
      profileHandler('user/one', []),
      profileHandler('embedded/two', []),
      profileHandler('sys/three', []),
      sessionsHandler([]),
      http.get(/\/v1\/daemon\/presets\/[^/]+$/, ({ request }) => {
        const id = decodeURIComponent(request.url.split('/').pop() ?? '');
        return HttpResponse.json({
          id,
          source: 'user',
          path: `/tmp/nexus42/presets/${id}/preset.yaml`,
          yaml: 'revision: 1',
        });
      }),
      http.post('/v1/daemon/presets:validate', async ({ request }) => {
        const body = (await request.json()) as { path: string };
        validatedPaths.push(body.path);
        return HttpResponse.json({ valid: true, errors: [], warnings: [] });
      }),
    );

    renderHarness();
    await screen.findByTestId('preset-row-user/one');

    // Validate exists only where a manifest file does: user and system rows.
    expect(screen.getByTestId('preset-validate-user/one')).toBeInTheDocument();
    expect(screen.getByTestId('preset-validate-sys/three')).toBeInTheDocument();
    expect(screen.queryByTestId('preset-validate-embedded/two')).not.toBeInTheDocument();

    await userEvent.click(screen.getByTestId('preset-validate-user/one'));

    // The row button names THAT preset: the form arrives pre-filled with its
    // own manifest path (still editable) instead of a blank disk path.
    const dialog = await screen.findByRole('dialog', { name: '验证预设' });
    const pathField = within(dialog).getByRole('textbox');
    expect(pathField).toHaveValue('/tmp/nexus42/presets/user/one/preset.yaml');

    await userEvent.click(within(dialog).getByRole('button', { name: '验证' }));

    await waitFor(() =>
      expect(validatedPaths).toEqual(['/tmp/nexus42/presets/user/one/preset.yaml']),
    );
    expect(await within(dialog).findByText('预设有效')).toBeInTheDocument();
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
    // A failed source must not assert a positive state: no `idle`, but an
    // explicit unknown badge.
    const badge = screen.getByTestId('preset-running-user/one');
    expect(badge).toHaveTextContent('运行状态未知');
    expect(badge).not.toHaveTextContent('idle');
  });

  it('marks a truncated running count as a lower bound (N+)', async () => {
    useHandlers(
      http.get('/v1/daemon/presets', () => HttpResponse.json(presetBody())),
      profileHandler('user/one', []),
      profileHandler('embedded/two', []),
      profileHandler('sys/three', []),
      // 2 running rows per page × the 10-page cap → 20+ trusted as a lower bound.
      truncatedSessionsHandler([session('user/one', 'running'), session('user/one', 'running')]),
    );

    renderHarness();

    await waitFor(() =>
      expect(screen.getByTestId('preset-running-user/one')).toHaveTextContent(
        'running ×20+',
      ),
    );
  });

  it('never reports idle for a truncated walk with zero running sessions', async () => {
    useHandlers(
      http.get('/v1/daemon/presets', () => HttpResponse.json(presetBody())),
      profileHandler('user/one', []),
      profileHandler('embedded/two', []),
      profileHandler('sys/three', []),
      truncatedSessionsHandler([]),
    );

    renderHarness();

    await waitFor(() => {
      const badge = screen.getByTestId('preset-running-user/one');
      expect(badge).toHaveTextContent('running ×0+');
      expect(badge).not.toHaveTextContent('idle');
    });
  });
});
