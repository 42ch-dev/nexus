import { http, HttpResponse } from 'msw';
import { beforeEach, describe, expect, it } from 'vitest';
import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import { StrategiesPage } from '@/pages/strategies-page';
import { renderInApp } from '@/test/test-providers';
import { useHandlers } from '@/test/msw-server';
import { BrowserClient } from '@/lib/nexus';
import type { PresetProfileLanes } from '@/lib/nexus';
import { i18n } from '@/lib/i18n/config';
import { act } from '@testing-library/react';

function makeClient(): BrowserClient {
  return new BrowserClient();
}

function renderStrategies() {
  return renderInApp(<StrategiesPage />, { client: makeClient(), activeCreatorId: 'creator-a' });
}

/**
 * Realistic per-source lane defaults (P0 lane honesty — W-003/F-002): every
 * resolvable preset reports `wallClock` + `direct`; embedded presets add
 * `session`; only works-cron role presets report `cron`. Tests override per
 * preset where the lane itself is under test.
 */
const EMBEDDED_LANES: PresetProfileLanes = {
  cron: false,
  wallClock: true,
  session: true,
  direct: true,
};

const USER_LANES: PresetProfileLanes = {
  cron: false,
  wallClock: true,
  session: false,
  direct: true,
};

function profileHandler(
  id: string,
  lanes: PresetProfileLanes = id.startsWith('user/') ? USER_LANES : EMBEDDED_LANES,
) {
  return http.get(`/v1/daemon/orchestration/presets/${encodeURIComponent(id)}/profile`, () =>
    HttpResponse.json({
      id,
      version: 1,
      sourceHash: 'a'.repeat(64),
      lanes,
      states: [],
    }),
  );
}

function profileHandlers(ids: string[]): ReturnType<typeof profileHandler>[] {
  return ids.map((id) => profileHandler(id));
}

beforeEach(async () => {
  await i18n.changeLanguage('en');
});

describe('StrategiesPage', () => {
  it('renders presets grouped by source', async () => {
    useHandlers(
      http.get('/v1/daemon/presets', () =>
        HttpResponse.json({
          user: [{ id: 'user/foo', source: 'user', run_intents: ['write'] }],
          system: [{ id: 'system/bar', source: 'system' }],
          embedded: [{ id: 'embedded/baz', source: 'embedded', run_intents: ['edit'] }],
        }),
      ),
      ...profileHandlers(['user/foo', 'embedded/baz']),
    );

    renderStrategies();

    await screen.findByRole('heading', { name: 'User presets' });
    // The strategy catalog (V1.171 P1) lists user + embedded rows too, so
    // ids appear in both the catalog card and the manager group cards.
    const userGroup = within(screen.getByTestId('preset-group-user'));
    const systemGroup = within(screen.getByTestId('preset-group-system'));
    const embeddedGroup = within(screen.getByTestId('preset-group-embedded'));
    const catalog = within(screen.getByTestId('strategy-catalog'));

    expect(userGroup.getByText('user/foo')).toBeInTheDocument();
    expect(systemGroup.getByText('system/bar')).toBeInTheDocument();
    expect(embeddedGroup.getByText('embedded/baz')).toBeInTheDocument();
    expect(catalog.getByText('user/foo')).toBeInTheDocument();
    expect(catalog.getByText('embedded/baz')).toBeInTheDocument();

    expect(userGroup.getByText('write')).toBeInTheDocument();
    expect(embeddedGroup.getByText('edit')).toBeInTheDocument();
  });

  it('renders empty states for each group when no presets exist', async () => {
    useHandlers(http.get('/v1/daemon/presets', () => HttpResponse.json({ user: [], system: [], embedded: [] })));

    renderStrategies();

    await waitFor(() => expect(screen.getByText('No user presets yet. Scaffold one to start.')).toBeInTheDocument());
    expect(screen.getByText('No system presets discovered.')).toBeInTheDocument();
    expect(screen.getByText('No embedded presets.')).toBeInTheDocument();
  });

  it('reloads a preset when its Reload button is clicked', async () => {
    const user = userEvent.setup();
    let reloaded = false;

    useHandlers(
      http.get('/v1/daemon/presets', () =>
        HttpResponse.json({
          user: [{ id: 'user/foo', source: 'user' }],
          system: [],
          embedded: [],
        }),
      ),
      ...profileHandlers(['user/foo']),
      http.post('/v1/daemon/presets/user%2Ffoo:reload', () => {
        reloaded = true;
        return HttpResponse.json({ id: 'user/foo', source: 'user' });
      }),
    );

    renderStrategies();

    const row = await within(await screen.findByTestId('preset-group-user')).findByText('user/foo');
    const reloadButton = within(row.closest('li')!).getByRole('button', { name: 'Reload' });
    await user.click(reloadButton);

    await waitFor(() => expect(reloaded).toBe(true));
  });

  it('renders the error state and retry when presets fail to load', async () => {
    useHandlers(
      http.get('/v1/daemon/presets', () =>
        HttpResponse.json({ error: { code: 'internal', message: 'boom' } }, { status: 500 }),
      ),
    );

    renderStrategies();

    await waitFor(() => expect(screen.getByText('Could not load presets')).toBeInTheDocument());
    expect(screen.getByRole('button', { name: 'Try again' })).toBeInTheDocument();
    expect(screen.queryByText('Could not load this view')).not.toBeInTheDocument();
  });

  it('renders unavailable state when orchestration engine is down (503)', async () => {
    useHandlers(
      http.get('/v1/daemon/presets', () =>
        HttpResponse.json(
          {
            success: false,
            error: { code: 'service_unavailable', message: 'engine not available' },
          },
          { status: 503 },
        ),
      ),
    );

    renderStrategies();

    await waitFor(() =>
      expect(screen.getByText('Orchestration engine not running')).toBeInTheDocument(),
    );
    expect(screen.getByRole('button', { name: 'Try again' })).toBeInTheDocument();
    expect(screen.queryByText('Could not load this view')).not.toBeInTheDocument();
  });

  it('switches to zh-CN locale without remounting', async () => {
    useHandlers(
      http.get('/v1/daemon/presets', () =>
        HttpResponse.json({
          user: [{ id: 'user/foo', source: 'user' }],
          system: [],
          embedded: [],
        }),
      ),
      ...profileHandlers(['user/foo']),
    );

    renderStrategies();
    await screen.findByRole('heading', { name: 'User presets' });

    act(() => {
      i18n.changeLanguage('zh-CN');
    });

    await waitFor(() => expect(screen.getByRole('heading', { name: '用户预设' })).toBeInTheDocument());
  });

  it('filters _system.* ids out of the System presets section (AC-P0-3)', async () => {
    useHandlers(
      http.get('/v1/daemon/presets', () =>
        HttpResponse.json({
          user: [],
          system: [
            { id: '_system.maintenance', source: 'system' },
            { id: 'author-strategy', source: 'system' },
          ],
          embedded: [],
        }),
      ),
    );

    renderStrategies();

    await screen.findByRole('heading', { name: 'System presets' });
    expect(screen.queryByText('_system.maintenance')).not.toBeInTheDocument();
    expect(screen.getByText('author-strategy')).toBeInTheDocument();
  });

  it('removes the header Validate button and exposes Validate per row (AC-P0-4)', async () => {
    useHandlers(
      http.get('/v1/daemon/presets', () =>
        HttpResponse.json({
          user: [{ id: 'user/foo', source: 'user' }],
          system: [{ id: 'system/bar', source: 'system' }],
          embedded: [{ id: 'embedded/baz', source: 'embedded' }],
        }),
      ),
      ...profileHandlers(['user/foo', 'embedded/baz']),
    );

    renderStrategies();

    await within(await screen.findByTestId('preset-group-user')).findByText('user/foo');
    // One Validate button per manager row, none in the header (header would
    // add a 4th); catalog rows carry no manager actions.
    expect(screen.getAllByRole('button', { name: 'Validate' })).toHaveLength(3);

    const userRow = within(await screen.findByTestId('preset-group-user')).getByText('user/foo');
    expect(within(userRow.closest('li')!).getByRole('button', { name: 'Validate' })).toBeInTheDocument();
  });

  it('opens the existing Validate dialog when a row Validate button is clicked (AC-P0-4)', async () => {
    const user = userEvent.setup();
    useHandlers(
      http.get('/v1/daemon/presets', () =>
        HttpResponse.json({
          user: [{ id: 'user/foo', source: 'user' }],
          system: [],
          embedded: [],
        }),
      ),
      ...profileHandlers(['user/foo']),
    );

    renderStrategies();

    const userRow = await within(await screen.findByTestId('preset-group-user')).findByText('user/foo');
    await user.click(within(userRow.closest('li')!).getByRole('button', { name: 'Validate' }));

    await screen.findByRole('heading', { name: 'Validate Preset' });
  });

  it('shows Delete only on user preset rows (AC-P0-5)', async () => {
    useHandlers(
      http.get('/v1/daemon/presets', () =>
        HttpResponse.json({
          user: [{ id: 'user/foo', source: 'user' }],
          system: [{ id: 'system/bar', source: 'system' }],
          embedded: [{ id: 'embedded/baz', source: 'embedded' }],
        }),
      ),
      ...profileHandlers(['user/foo', 'embedded/baz']),
    );

    renderStrategies();
    await within(await screen.findByTestId('preset-group-user')).findByText('user/foo');

    const userLi = within(await screen.findByTestId('preset-group-user')).getByText('user/foo').closest('li')!;
    const systemLi = within(await screen.findByTestId('preset-group-system')).getByText('system/bar').closest('li')!;
    const embeddedLi = within(await screen.findByTestId('preset-group-embedded')).getByText('embedded/baz').closest('li')!;

    expect(within(userLi).getByRole('button', { name: 'Delete preset user/foo' })).toBeInTheDocument();
    expect(within(systemLi).queryByRole('button', { name: /Delete/i })).not.toBeInTheDocument();
    expect(within(embeddedLi).queryByRole('button', { name: /Delete/i })).not.toBeInTheDocument();
  });

  it('confirms delete, calls deletePreset, and refreshes the list (AC-P0-5)', async () => {
    const user = userEvent.setup();
    let deleted = false;
    useHandlers(
      http.get('/v1/daemon/presets', () =>
        HttpResponse.json({
          user: deleted ? [] : [{ id: 'user/foo', source: 'user' }],
          system: [],
          embedded: [],
        }),
      ),
      ...profileHandlers(['user/foo']),
      http.delete('/v1/daemon/presets/user%2Ffoo', () => {
        deleted = true;
        return new HttpResponse(null, { status: 204 });
      }),
    );

    renderStrategies();

    const userRow = await within(await screen.findByTestId('preset-group-user')).findByText('user/foo');
    await user.click(
      within(userRow.closest('li')!).getByRole('button', { name: 'Delete preset user/foo' }),
    );

    // Confirm dialog title names the preset.
    await screen.findByRole('heading', { name: 'Delete "user/foo"' });
    await user.click(screen.getByRole('button', { name: /^Delete$/ }));

    await waitFor(() => expect(deleted).toBe(true));
    // Invalidation refetched the list; the row is gone from both the
    // catalog and the manager group.
    await waitFor(() => expect(screen.queryByText('user/foo')).not.toBeInTheDocument());
  });
});
