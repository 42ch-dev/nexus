import { http, HttpResponse } from 'msw';
import { beforeEach, describe, expect, it } from 'vitest';

import { renderInApp } from '@/test/test-providers';
import { useHandlers } from '@/test/msw-server';
import { BrowserClient } from '@/lib/nexus';
import { i18n } from '@/lib/i18n/config';
import { SchedulePage } from '@/pages/schedule-page';
import { act, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

const client = () => new BrowserClient();

function renderSchedule() {
  return renderInApp(<SchedulePage />, { client: client() });
}

function renderScheduleWithCreator() {
  return renderInApp(<SchedulePage />, { client: client(), activeCreatorId: 'creator-a' });
}

const SCHEDULES_EMPTY = { items: [], pagination: { limit: 20, has_more: false } };

const PRESETS = {
  user: [{ id: 'user/foo', source: 'user' }],
  system: [],
  embedded: [{ id: 'embedded/baz', source: 'embedded' }],
};

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
      http.get('/v1/daemon/orchestration/schedules', () => HttpResponse.json(SCHEDULES_EMPTY)),
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

  // ── V1.171 P2 create journey (PL-15/PL-16) ────────────────────────────────

  it('disables the create button when no active creator is selected', async () => {
    useHandlers(
      http.get('/v1/daemon/orchestration/schedules', () => HttpResponse.json(SCHEDULES_EMPTY)),
    );

    renderSchedule();

    await screen.findByText('No schedules');
    expect(screen.getByRole('button', { name: 'Create schedule' })).toBeDisabled();
  });

  it('opens the create dialog with USER + embedded presets and submits the expected payload', async () => {
    const user = userEvent.setup();
    let postedBody: unknown = null;
    useHandlers(
      http.get('/v1/daemon/orchestration/schedules', () => HttpResponse.json(SCHEDULES_EMPTY)),
      http.get('/v1/daemon/presets', () => HttpResponse.json(PRESETS)),
      http.post('/v1/daemon/orchestration/schedules', async ({ request }) => {
        postedBody = await request.json();
        return HttpResponse.json({
          schedule_id: 'sched-new',
          status: 'pending',
          core_context_version: 0,
        });
      }),
    );

    renderScheduleWithCreator();

    await screen.findByText('No schedules');
    await user.click(screen.getByRole('button', { name: 'Create schedule' }));

    await screen.findByRole('heading', { name: 'Create schedule' });
    expect(screen.getByRole('option', { name: 'user/foo' })).toBeInTheDocument();
    expect(screen.getByRole('option', { name: 'embedded/baz' })).toBeInTheDocument();

    await user.selectOptions(screen.getByLabelText('Preset'), 'user/foo');
    await user.type(screen.getByLabelText('Label (optional)'), 'Daily digest');
    await user.type(screen.getByLabelText('Seed (optional)'), 'seed text');
    await user.click(screen.getByRole('button', { name: 'Create' }));

    await waitFor(() => expect(postedBody).toEqual({
      creator_id: 'creator-a',
      preset_id: 'user/foo',
      label: 'Daily digest',
      seed: 'seed text',
    }));
    // Success toast + dialog closes.
    await screen.findByText('Schedule created');
    await waitFor(() =>
      expect(screen.queryByRole('heading', { name: 'Create schedule' })).not.toBeInTheDocument(),
    );
  });

  it('refetches the schedule list after a successful create (invalidation)', async () => {
    const user = userEvent.setup();
    let created = false;
    useHandlers(
      http.get('/v1/daemon/orchestration/schedules', () =>
        HttpResponse.json(
          created
            ? {
                items: [
                  {
                    schedule_id: 'sched-new',
                    creator_id: 'creator-a',
                    preset_id: 'user/foo',
                    status: 'pending',
                    label: 'Daily digest',
                    current_core_context_version: 0,
                    created_at: '2026-06-24T00:00:00Z',
                    updated_at: '2026-06-24T00:00:00Z',
                  },
                ],
                pagination: { limit: 20, has_more: false },
              }
            : SCHEDULES_EMPTY,
        ),
      ),
      http.get('/v1/daemon/presets', () => HttpResponse.json(PRESETS)),
      http.post('/v1/daemon/orchestration/schedules', () => {
        created = true;
        return HttpResponse.json({
          schedule_id: 'sched-new',
          status: 'pending',
          core_context_version: 0,
        });
      }),
    );

    renderScheduleWithCreator();

    await screen.findByText('No schedules');
    await user.click(screen.getByRole('button', { name: 'Create schedule' }));
    await screen.findByRole('heading', { name: 'Create schedule' });
    await user.selectOptions(screen.getByLabelText('Preset'), 'user/foo');
    await user.click(screen.getByRole('button', { name: 'Create' }));

    // Invalidation refetched the list; the new row renders.
    await screen.findByText('Daily digest');
  });

  it('surfaces a daemon 400 visibly and keeps the dialog open', async () => {
    const user = userEvent.setup();
    useHandlers(
      http.get('/v1/daemon/orchestration/schedules', () => HttpResponse.json(SCHEDULES_EMPTY)),
      http.get('/v1/daemon/presets', () => HttpResponse.json(PRESETS)),
      http.post('/v1/daemon/orchestration/schedules', () =>
        HttpResponse.json(
          { success: false, error: { code: 'bad_request', message: 'input key is reserved' } },
          { status: 400 },
        ),
      ),
    );

    renderScheduleWithCreator();

    await screen.findByText('No schedules');
    await user.click(screen.getByRole('button', { name: 'Create schedule' }));
    await screen.findByRole('heading', { name: 'Create schedule' });
    await user.selectOptions(screen.getByLabelText('Preset'), 'user/foo');
    await user.click(screen.getByRole('button', { name: 'Create' }));

    // Error toast surfaces the daemon message.
    await screen.findByText('input key is reserved');
    // Inline error keeps the dialog open — never silent.
    expect(
      screen.getByText(/Could not create the schedule\. Check the daemon message above and try again/i),
    ).toBeInTheDocument();
    expect(screen.getByRole('heading', { name: 'Create schedule' })).toBeInTheDocument();
  });

  it('requires a preset selection before submitting', async () => {
    const user = userEvent.setup();
    useHandlers(
      http.get('/v1/daemon/orchestration/schedules', () => HttpResponse.json(SCHEDULES_EMPTY)),
      http.get('/v1/daemon/presets', () => HttpResponse.json(PRESETS)),
    );

    renderScheduleWithCreator();

    await screen.findByText('No schedules');
    await user.click(screen.getByRole('button', { name: 'Create schedule' }));
    await screen.findByRole('heading', { name: 'Create schedule' });

    // Submit is disabled until a preset is chosen.
    expect(screen.getByRole('button', { name: 'Create' })).toBeDisabled();
  });
});
