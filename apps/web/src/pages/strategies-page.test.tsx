import { http, HttpResponse } from 'msw';
import { beforeEach, describe, expect, it } from 'vitest';
import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import { StrategiesPage } from '@/pages/strategies-page';
import { renderInApp } from '@/test/test-providers';
import { useHandlers } from '@/test/msw-server';
import { BrowserClient } from '@/lib/nexus';
import { i18n } from '@/lib/i18n/config';
import { act } from '@testing-library/react';

function makeClient(): BrowserClient {
  return new BrowserClient();
}

function renderStrategies() {
  return renderInApp(<StrategiesPage />, { client: makeClient(), activeCreatorId: 'creator-a' });
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
    );

    renderStrategies();

    await screen.findByRole('heading', { name: 'User presets' });
    expect(screen.getByText('user/foo')).toBeInTheDocument();
    expect(screen.getByText('system/bar')).toBeInTheDocument();
    expect(screen.getByText('embedded/baz')).toBeInTheDocument();

    expect(screen.getByText('write')).toBeInTheDocument();
    expect(screen.getByText('edit')).toBeInTheDocument();
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
      http.post('/v1/daemon/presets/user%2Ffoo:reload', () => {
        reloaded = true;
        return HttpResponse.json({ id: 'user/foo', source: 'user' });
      }),
    );

    renderStrategies();

    const row = await screen.findByText('user/foo');
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

    await waitFor(() => expect(screen.getByText('Could not load presets.')).toBeInTheDocument());
    expect(screen.getByRole('button', { name: 'Try again' })).toBeInTheDocument();
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
    );

    renderStrategies();
    await screen.findByRole('heading', { name: 'User presets' });

    act(() => {
      i18n.changeLanguage('zh-CN');
    });

    await waitFor(() => expect(screen.getByRole('heading', { name: '用户预设' })).toBeInTheDocument());
  });
});
