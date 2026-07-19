import { http, HttpResponse } from 'msw';
import { beforeEach, describe, expect, it } from 'vitest';

import { renderInApp } from '@/test/test-providers';
import { useHandlers } from '@/test/msw-server';
import { BrowserClient } from '@/lib/nexus';
import { i18n } from '@/lib/i18n/config';
import { SessionsPage } from '@/pages/sessions-page';
import { act, screen, waitFor } from '@testing-library/react';

const client = () => new BrowserClient();

function renderSessions() {
  return renderInApp(<SessionsPage />, { client: client() });
}

beforeEach(async () => {
  await i18n.changeLanguage('en');
});

describe('SessionsPage', () => {
  it('renders the sessions table on a successful list', async () => {
    useHandlers(
      http.get('/v1/daemon/orchestration/sessions', () =>
        HttpResponse.json({
          items: [
            {
              session_id: 'session-1',
              creator_id: 'creator-a',
              preset_id: 'preset-a',
              status: 'running',
              current_task_id: 'task-1',
            },
          ],
          pagination: { limit: 20, has_more: false },
        }),
      ),
    );

    renderSessions();

    expect(await screen.findByText('session-1')).toBeInTheDocument();
    expect(screen.getByText('Running')).toBeInTheDocument();
  });

  it('renders the empty state when there are no sessions', async () => {
    useHandlers(
      http.get('/v1/daemon/orchestration/sessions', () =>
        HttpResponse.json({ items: [], pagination: { limit: 20, has_more: false } }),
      ),
    );

    renderSessions();

    expect(await screen.findByText('No active sessions')).toBeInTheDocument();
    expect(
      screen.getByText(/Orchestration sessions will appear here when the runtime runs/i),
    ).toBeInTheDocument();
  });

  // AC-P2-1 / AD-P0-2c (V1.120 P2 / F3): daemon-internal `_system.*` boot
  // sessions must never appear — an idle daemon shows the empty state.
  it('renders the empty state when only _system.* sessions come back (defensive filter)', async () => {
    useHandlers(
      http.get('/v1/daemon/orchestration/sessions', () =>
        HttpResponse.json({
          items: [
            {
              session_id: 'sys-session-1',
              creator_id: '',
              preset_id: '_system.maintenance',
              status: 'running',
              current_task_id: null,
            },
            {
              session_id: 'sys-session-2',
              creator_id: '',
              preset_id: '_system.health',
              status: 'running',
              current_task_id: null,
            },
          ],
          pagination: { limit: 20, has_more: false },
        }),
      ),
    );

    renderSessions();

    expect(await screen.findByText('No active sessions')).toBeInTheDocument();
    expect(screen.queryByText(/_system/)).not.toBeInTheDocument();
  });

  it('hides _system.* sessions but keeps author sessions', async () => {
    useHandlers(
      http.get('/v1/daemon/orchestration/sessions', () =>
        HttpResponse.json({
          items: [
            {
              session_id: 'sys-session-1',
              creator_id: '',
              preset_id: '_system.maintenance',
              status: 'running',
              current_task_id: null,
            },
            {
              session_id: 'session-1',
              creator_id: 'creator-a',
              preset_id: 'novel-writing',
              status: 'running',
              current_task_id: 'task-1',
            },
          ],
          pagination: { limit: 20, has_more: false },
        }),
      ),
    );

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
    expect(screen.queryByText('Could not load this view')).not.toBeInTheDocument();
  });

  it('switches to zh-CN locale without remounting', async () => {
    useHandlers(
      http.get('/v1/daemon/orchestration/sessions', () =>
        HttpResponse.json({
          items: [
            {
              session_id: 'session-1',
              creator_id: 'creator-a',
              preset_id: 'preset-a',
              status: 'running',
              current_task_id: 'task-1',
            },
          ],
          pagination: { limit: 20, has_more: false },
        }),
      ),
    );

    renderSessions();
    expect(await screen.findByRole('heading', { name: 'Sessions' })).toBeInTheDocument();

    act(() => {
      i18n.changeLanguage('zh-CN');
    });

    await waitFor(() => expect(screen.getByRole('heading', { name: '会话' })).toBeInTheDocument());
  });
});
