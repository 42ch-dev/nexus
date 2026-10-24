/**
 * `SessionsPage` (plan v221-caps-sessions-rail T2) — the sessions rail as
 * cards.
 *
 * Each card is one link to the session's run view: short id as the title,
 * status pill, then Preset / Creator. `current_task_id` is deliberately not
 * rendered (the run view owns it). The list keeps the server's F-F1 order; the
 * empty, error and engine-unavailable branches stay distinct.
 */
import { http, HttpResponse } from 'msw';
import { beforeEach, describe, expect, it } from 'vitest';

import { SessionsPage } from '@/components/developer/sessions-page';
import { i18n } from '@/lib/i18n/config';
import { BrowserClient } from '@/lib/nexus';
import { useHandlers } from '@/test/msw-server';
import { renderInApp } from '@/test/test-providers';
import { act, screen, waitFor, within } from '@testing-library/react';

const client = () => new BrowserClient();

function renderSessions() {
  return renderInApp(<SessionsPage />, { client: client() });
}

interface SessionItem {
  session_id: string;
  creator_id: string;
  preset_id: string;
  status: string;
  current_task_id?: string;
}

function listSessions(items: SessionItem[]) {
  useHandlers(
    http.get('/v1/daemon/orchestration/sessions', () =>
      HttpResponse.json({ items, pagination: { limit: 20, has_more: false } }),
    ),
  );
}

const ONE_SESSION: SessionItem[] = [
  {
    session_id: 'session-1',
    creator_id: 'creator-a',
    preset_id: 'preset-a',
    status: 'running',
    current_task_id: 'task-1',
  },
];

beforeEach(async () => {
  await i18n.changeLanguage('en');
});

describe('SessionsPage', () => {
  it('keeps the un-paginated response order (F-F1) across the cards', async () => {
    // Deliberately not alphabetical: the card order must mirror `items`.
    listSessions([
      { session_id: 'sess-mid', creator_id: 'creator-b', preset_id: 'preset-b', status: 'completed' },
      { session_id: 'sess-last', creator_id: 'creator-c', preset_id: 'preset-c', status: 'running' },
      { session_id: 'sess-first', creator_id: 'creator-a', preset_id: 'preset-a', status: 'failed' },
    ]);

    renderSessions();

    const links = await screen.findAllByRole('link');
    expect(links.map((link) => link.getAttribute('href'))).toEqual([
      '/sessions/sess-mid',
      '/sessions/sess-last',
      '/sessions/sess-first',
    ]);
    expect(links.map((link) => link.getAttribute('aria-label'))).toEqual([
      'Open run view for session sess-mid',
      'Open run view for session sess-last',
      'Open run view for session sess-first',
    ]);
  });

  it('renders one card per session: short id, status pill, preset, creator', async () => {
    listSessions(ONE_SESSION);

    renderSessions();

    const link = await screen.findByRole('link', {
      name: 'Open run view for session session-1',
    });
    const card = within(link);

    expect(card.getByText('session-1')).toBeInTheDocument();
    expect(card.getByText('Running')).toBeInTheDocument();
    expect(card.getByText('Preset')).toBeInTheDocument();
    expect(card.getByText('preset-a')).toBeInTheDocument();
    expect(card.getByText('Creator')).toBeInTheDocument();
    expect(card.getByText('creator-a')).toBeInTheDocument();

    // Dropped from the table: the transient in-flight pointer.
    expect(screen.queryByText(/Current task/i)).not.toBeInTheDocument();
    expect(screen.queryByText('task-1')).not.toBeInTheDocument();
    // The table chrome (column headers) is gone with the table.
    expect(screen.queryByRole('table')).not.toBeInTheDocument();
  });

  it('maps running/completed to the green pill and failed to the red one', async () => {
    listSessions([
      { session_id: 'sess-running', creator_id: 'creator-a', preset_id: 'preset-a', status: 'running' },
      { session_id: 'sess-done', creator_id: 'creator-a', preset_id: 'preset-a', status: 'completed' },
      { session_id: 'sess-failed', creator_id: 'creator-a', preset_id: 'preset-a', status: 'failed' },
    ]);

    renderSessions();

    const pill = async (shortId: string, label: string) => {
      const link = await screen.findByRole('link', {
        name: `Open run view for session ${shortId}`,
      });
      return within(link).getByText(label);
    };

    expect((await pill('sess-running', 'Running')).className).toContain(
      'nexus-ui-badge-soft-running-bg',
    );
    expect((await pill('sess-done', 'Completed')).className).toContain(
      'nexus-ui-badge-soft-running-bg',
    );
    expect((await pill('sess-failed', 'Failed')).className).toContain(
      'nexus-ui-badge-soft-error-bg',
    );
  });

  // v1.201 P1 T3 (compass D7): the acceptance-bearing observation surface — the
  // whole card carries the run id in the path and its accessible name.
  it('opens the run view from the card through its single link', async () => {
    listSessions(ONE_SESSION);

    renderSessions();

    const link = await screen.findByRole('link', {
      name: 'Open run view for session session-1',
    });
    expect(link).toHaveAttribute('href', '/sessions/session-1');
    // One navigation affordance, no nested interactive content.
    expect(within(link).queryAllByRole('link')).toHaveLength(0);
    expect(within(link).queryAllByRole('button')).toHaveLength(0);
  });

  it('renders the zero-session empty state', async () => {
    listSessions([]);

    renderSessions();

    expect(await screen.findByText('No sessions yet')).toBeInTheDocument();
    expect(
      screen.getByText('Sessions appear here when a preset run starts.'),
    ).toBeInTheDocument();
    expect(screen.queryAllByRole('link')).toHaveLength(0);
  });

  // AC-P2-1 / AD-P0-2c (V1.120 P2 / F3): daemon-internal `_system.*` boot
  // sessions must never appear — an idle daemon shows the empty state.
  it('renders the empty state when only _system.* sessions come back (defensive filter)', async () => {
    listSessions([
      {
        session_id: 'sys-session-1',
        creator_id: '',
        preset_id: '_system.maintenance',
        status: 'running',
      },
      { session_id: 'sys-session-2', creator_id: '', preset_id: '_system.health', status: 'running' },
    ]);

    renderSessions();

    expect(await screen.findByText('No sessions yet')).toBeInTheDocument();
    expect(screen.queryByText(/_system/)).not.toBeInTheDocument();
  });

  it('hides _system.* sessions but keeps author sessions', async () => {
    listSessions([
      {
        session_id: 'sys-session-1',
        creator_id: '',
        preset_id: '_system.maintenance',
        status: 'running',
      },
      {
        session_id: 'session-1',
        creator_id: 'creator-a',
        preset_id: 'novel-writing',
        status: 'running',
        current_task_id: 'task-1',
      },
    ]);

    renderSessions();

    expect(await screen.findByText('session-1')).toBeInTheDocument();
    expect(screen.getByText('novel-writing')).toBeInTheDocument();
    expect(screen.queryByText(/_system/)).not.toBeInTheDocument();
    expect(screen.queryByText('sys-session-1')).not.toBeInTheDocument();
  });

  it('renders the error state and offers retry when the daemon fails', async () => {
    useHandlers(
      http.get('/v1/daemon/orchestration/sessions', () =>
        HttpResponse.json(
          { success: false, error: { code: 'internal', message: 'boom' } },
          { status: 500 },
        ),
      ),
    );

    renderSessions();

    expect(await screen.findByText('Could not load sessions')).toBeInTheDocument();
    expect(screen.getByText(/Could not load orchestration sessions/i)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Try again' })).toBeInTheDocument();
    // Error and empty stay distinct branches.
    expect(screen.queryByText('No sessions yet')).not.toBeInTheDocument();
    expect(screen.queryByText('Could not load this view')).not.toBeInTheDocument();
  });

  it('renders unavailable state when orchestration engine is down (503)', async () => {
    useHandlers(
      http.get('/v1/daemon/orchestration/sessions', () =>
        HttpResponse.json(
          {
            success: false,
            error: { code: 'service_unavailable', message: 'engine not available' },
          },
          { status: 503 },
        ),
      ),
    );

    renderSessions();

    expect(await screen.findByText('Orchestration engine not running')).toBeInTheDocument();
    expect(screen.getByText(/Start the daemon orchestration engine/i)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Try again' })).toBeInTheDocument();
    expect(screen.queryByText('No sessions yet')).not.toBeInTheDocument();
    expect(screen.queryByText('Could not load this view')).not.toBeInTheDocument();
  });

  it('switches to zh-CN locale without remounting', async () => {
    listSessions(ONE_SESSION);

    renderSessions();
    expect(await screen.findByRole('heading', { name: 'Sessions' })).toBeInTheDocument();

    act(() => {
      i18n.changeLanguage('zh-CN');
    });

    await waitFor(() => expect(screen.getByRole('heading', { name: '会话' })).toBeInTheDocument());
    expect(screen.getByText('预设')).toBeInTheDocument();
    expect(screen.getByText('创作者')).toBeInTheDocument();
  });
});
