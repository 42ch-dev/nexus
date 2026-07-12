import { http, HttpResponse } from 'msw';
import { beforeEach, describe, expect, it } from 'vitest';

import { renderInApp } from '@/test/test-providers';
import { useHandlers } from '@/test/msw-server';
import { BrowserClient } from '@/lib/nexus';
import { i18n } from '@/lib/i18n/config';
import { SchedulePage } from '@/pages/schedule-page';
import { act, screen, waitFor } from '@testing-library/react';

const client = () => new BrowserClient();

function renderSchedule() {
  return renderInApp(<SchedulePage />, { client: client() });
}

beforeEach(async () => {
  await i18n.changeLanguage('en');
});

describe('SchedulePage', () => {
  it('renders the schedule table on a successful list', async () => {
    useHandlers(
      http.get('/v1/daemon/orchestration/schedules', () =>
        HttpResponse.json({
          items: [
            {
              schedule_id: 'sched-1',
              creator_id: 'creator-a',
              preset_id: 'preset-a',
              status: 'active',
              label: 'Daily digest',
              current_core_context_version: 3,
              created_at: '2026-06-24T00:00:00Z',
              updated_at: '2026-06-24T00:00:00Z',
            },
          ],
          pagination: { limit: 20, has_more: false },
        }),
      ),
    );

    renderSchedule();

    expect(await screen.findByText('Daily digest')).toBeInTheDocument();
    expect(screen.getByRole('heading', { name: 'Schedule' })).toBeInTheDocument();
  });

  it('renders the empty state when there are no schedules', async () => {
    useHandlers(
      http.get('/v1/daemon/orchestration/schedules', () =>
        HttpResponse.json({ items: [], pagination: { limit: 20, has_more: false } }),
      ),
    );

    renderSchedule();

    expect(await screen.findByText('No schedules')).toBeInTheDocument();
    expect(
      screen.getByText(/Schedules appear here once a Work has cron roles configured/i),
    ).toBeInTheDocument();
  });

  it('renders the error state and offers retry when the daemon fails', async () => {
    useHandlers(
      http.get('/v1/daemon/orchestration/schedules', () =>
        HttpResponse.json(
          { success: false, error: { code: 'internal', message: 'boom' } },
          { status: 500 },
        ),
      ),
    );

    renderSchedule();

    expect(await screen.findByText('Could not load this view')).toBeInTheDocument();
    expect(screen.getByText(/Could not load schedules/i)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Try again' })).toBeInTheDocument();
  });

  it('switches to zh-CN locale without remounting', async () => {
    useHandlers(
      http.get('/v1/daemon/orchestration/schedules', () =>
        HttpResponse.json({
          items: [
            {
              schedule_id: 'sched-1',
              creator_id: 'creator-a',
              preset_id: 'preset-a',
              status: 'active',
              label: 'Daily digest',
              current_core_context_version: 3,
              created_at: '2026-06-24T00:00:00Z',
              updated_at: '2026-06-24T00:00:00Z',
            },
          ],
          pagination: { limit: 20, has_more: false },
        }),
      ),
    );

    renderSchedule();
    expect(await screen.findByRole('heading', { name: 'Schedule' })).toBeInTheDocument();

    act(() => {
      i18n.changeLanguage('zh-CN');
    });

    await waitFor(() => expect(screen.getByRole('heading', { name: '日程' })).toBeInTheDocument());
  });
});
