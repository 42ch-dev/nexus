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

const WORKS_EMPTY = { items: [], pagination: { limit: 20, has_more: false } };

/**
 * The page also renders the per-Work cron section, which fetches
 * `GET /v1/daemon/works`. Tests that do not exercise the works section must
 * register a works handler themselves (msw `server.use` prepends, so a
 * default registered here would shadow a test's own works handler).
 */
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
      http.get('/v1/daemon/works', () => HttpResponse.json(WORKS_EMPTY)),
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
      http.get('/v1/daemon/works', () => HttpResponse.json(WORKS_EMPTY)),
      http.get('/v1/daemon/orchestration/schedules', () => HttpResponse.json(SCHEDULES_EMPTY)),
    );

    renderSchedule();

    expect(await screen.findByText('No schedules')).toBeInTheDocument();
    expect(
      screen.getByText(/Create a schedule to queue a preset run on the daemon/i),
    ).toBeInTheDocument();
  });

  it('renders the error state and offers retry when the daemon fails', async () => {
    useHandlers(
      http.get('/v1/daemon/works', () => HttpResponse.json(WORKS_EMPTY)),
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
      http.get('/v1/daemon/works', () => HttpResponse.json(WORKS_EMPTY)),
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
      http.get('/v1/daemon/works', () => HttpResponse.json(WORKS_EMPTY)),
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
      http.get('/v1/daemon/works', () => HttpResponse.json(WORKS_EMPTY)),
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
      http.get('/v1/daemon/works', () => HttpResponse.json(WORKS_EMPTY)),
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
      http.get('/v1/daemon/works', () => HttpResponse.json(WORKS_EMPTY)),
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
      http.get('/v1/daemon/works', () => HttpResponse.json(WORKS_EMPTY)),
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

  // ── V1.171 P2 edit journey (PL-16/AR-29) ─────────────────────────────────

  const SCHEDULE_ROW = {
    schedule_id: 'sched-1',
    creator_id: 'creator-a',
    preset_id: 'preset-a',
    status: 'active',
    label: 'Daily digest',
    current_core_context_version: 3,
    created_at: '2026-06-24T00:00:00Z',
    updated_at: '2026-06-24T00:00:00Z',
  };

  const WORK_ROW = {
    work_id: 'work-1',
    title: 'My Novel',
    status: 'active',
    intake_status: 'active',
    primary_preset_id: 'novel-writing',
    updated_at: '2026-06-24T00:00:00Z',
  };

  const CRON_DEFAULTS = {
    tz: 'UTC',
    roles: {
      brainstorm: { cron: '0 3,9,15,21 * * *', enabled: true },
      write: { cron: '0 4,10,16,22 * * *', enabled: true },
      review: { cron: '0,30 * * * *', enabled: true },
    },
    is_default: true,
  };

  it('edits a schedule label via PATCH and refetches the list (round-trip)', async () => {
    const user = userEvent.setup();
    let patchedBody: unknown = null;
    let label = 'Daily digest';
    useHandlers(
      http.get('/v1/daemon/works', () => HttpResponse.json(WORKS_EMPTY)),
      http.get('/v1/daemon/presets', () => HttpResponse.json(PRESETS)),
      http.get('/v1/daemon/orchestration/schedules', () =>
        HttpResponse.json({
          items: [{ ...SCHEDULE_ROW, label }],
          pagination: { limit: 20, has_more: false },
        }),
      ),
      http.patch('/v1/daemon/orchestration/schedules/sched-1', async ({ request }) => {
        patchedBody = await request.json();
        label = 'Renamed digest';
        return HttpResponse.json({ ...SCHEDULE_ROW, label });
      }),
    );

    renderScheduleWithCreator();

    await screen.findByText('Daily digest');
    await user.click(screen.getByRole('button', { name: 'Edit label for schedule sched-1' }));

    await screen.findByRole('heading', { name: 'Edit schedule label' });
    const input = screen.getByLabelText('Label');
    await user.clear(input);
    await user.type(input, 'Renamed digest');
    await user.click(screen.getByRole('button', { name: 'Save' }));

    await waitFor(() =>
      expect(patchedBody).toEqual({ label: 'Renamed digest' }),
    );
    // Invalidation refetched the list; the new label renders.
    await screen.findByText('Renamed digest');
  });

  it('clears a schedule label to NULL via an empty PATCH label', async () => {
    const user = userEvent.setup();
    let patchedBody: unknown = null;
    let label: string | null = 'Daily digest';
    useHandlers(
      http.get('/v1/daemon/works', () => HttpResponse.json(WORKS_EMPTY)),
      http.get('/v1/daemon/presets', () => HttpResponse.json(PRESETS)),
      http.get('/v1/daemon/orchestration/schedules', () =>
        HttpResponse.json({
          items: [{ ...SCHEDULE_ROW, label }],
          pagination: { limit: 20, has_more: false },
        }),
      ),
      http.patch('/v1/daemon/orchestration/schedules/sched-1', async ({ request }) => {
        patchedBody = await request.json();
        label = null;
        return HttpResponse.json({ ...SCHEDULE_ROW, label });
      }),
    );

    renderScheduleWithCreator();

    await screen.findByText('Daily digest');
    await user.click(screen.getByRole('button', { name: 'Edit label for schedule sched-1' }));

    await screen.findByRole('heading', { name: 'Edit schedule label' });
    const input = screen.getByLabelText('Label');
    await user.clear(input);
    await user.click(screen.getByRole('button', { name: 'Save' }));

    // Empty input → `label: ""` → daemon clears to NULL.
    await waitFor(() => expect(patchedBody).toEqual({ label: '' }));
    // The cleared row renders the dash placeholder.
    await waitFor(() => expect(screen.queryByText('Daily digest')).not.toBeInTheDocument());
  });

  it('surfaces a daemon 400 visibly and keeps the label dialog open', async () => {
    const user = userEvent.setup();
    useHandlers(
      http.get('/v1/daemon/works', () => HttpResponse.json(WORKS_EMPTY)),
      http.get('/v1/daemon/presets', () => HttpResponse.json(PRESETS)),
      http.get('/v1/daemon/orchestration/schedules', () =>
        HttpResponse.json({
          items: [SCHEDULE_ROW],
          pagination: { limit: 20, has_more: false },
        }),
      ),
      http.patch('/v1/daemon/orchestration/schedules/sched-1', () =>
        HttpResponse.json(
          { success: false, error: { code: 'invalid_label', message: 'label exceeds maximum length' } },
          { status: 400 },
        ),
      ),
    );

    renderScheduleWithCreator();

    await screen.findByText('Daily digest');
    await user.click(screen.getByRole('button', { name: 'Edit label for schedule sched-1' }));
    await screen.findByRole('heading', { name: 'Edit schedule label' });

    const input = screen.getByLabelText('Label');
    await user.clear(input);
    await user.type(input, 'x'.repeat(600));
    await user.click(screen.getByRole('button', { name: 'Save' }));

    // Error toast surfaces the daemon message.
    await screen.findByText('label exceeds maximum length');
    // Inline error keeps the dialog open — never silent.
    expect(
      screen.getByText(/Could not update the label\. Check the daemon message above and try again/i),
    ).toBeInTheDocument();
    expect(screen.getByRole('heading', { name: 'Edit schedule label' })).toBeInTheDocument();
  });

  it('shows the using-defaults marker when the Work cron config is unset', async () => {
    useHandlers(http.get('/v1/daemon/presets', () => HttpResponse.json(PRESETS)));
    useHandlers(
      http.get('/v1/daemon/orchestration/schedules', () => HttpResponse.json(SCHEDULES_EMPTY)),
      http.get('/v1/daemon/works', () =>
        HttpResponse.json({ items: [WORK_ROW], pagination: { limit: 20, has_more: false } }),
      ),
      http.get('/v1/daemon/works/work-1/cron', () => HttpResponse.json(CRON_DEFAULTS)),
    );

    renderScheduleWithCreator();

    await screen.findByText('My Novel');
    await userEvent.setup().click(screen.getByRole('button', { name: 'Edit cron for Work work-1' }));

    await screen.findByRole('heading', { name: 'Edit Work cron' });
    expect(screen.getByTestId('work-cron-defaults-marker')).toHaveTextContent(/Using defaults/i);
    // Honesty: the expression is shown, no next-run time is computed.
    expect(screen.getByDisplayValue('0 3,9,15,21 * * *')).toBeInTheDocument();
    expect(screen.queryByText(/next run|next fire/i)).not.toBeInTheDocument();
  });

  it('sends the GET config as the CAS pre-image on save', async () => {
    const user = userEvent.setup();
    let putBody: unknown = null;
    useHandlers(
      http.get('/v1/daemon/presets', () => HttpResponse.json(PRESETS)),
      http.get('/v1/daemon/orchestration/schedules', () => HttpResponse.json(SCHEDULES_EMPTY)),
      http.get('/v1/daemon/works', () =>
        HttpResponse.json({ items: [WORK_ROW], pagination: { limit: 20, has_more: false } }),
      ),
      http.get('/v1/daemon/works/work-1/cron', () => HttpResponse.json(CRON_DEFAULTS)),
      http.put('/v1/daemon/works/work-1/cron', async ({ request }) => {
        putBody = await request.json();
        return HttpResponse.json({ ...CRON_DEFAULTS, is_default: false });
      }),
    );

    renderScheduleWithCreator();

    await screen.findByText('My Novel');
    await user.click(screen.getByRole('button', { name: 'Edit cron for Work work-1' }));
    await screen.findByRole('heading', { name: 'Edit Work cron' });

    await user.click(screen.getByRole('button', { name: 'Save' }));

    await waitFor(() =>
      expect(putBody).toEqual({
        tz: 'UTC',
        roles: CRON_DEFAULTS.roles,
        // Unset config → empty-string pre-image ("must currently be unset").
        expected_current_json: '',
      }),
    );
  });

  it('surfaces a 409 CAS conflict with a reload prompt', async () => {
    const user = userEvent.setup();
    useHandlers(
      http.get('/v1/daemon/presets', () => HttpResponse.json(PRESETS)),
      http.get('/v1/daemon/orchestration/schedules', () => HttpResponse.json(SCHEDULES_EMPTY)),
      http.get('/v1/daemon/works', () =>
        HttpResponse.json({ items: [WORK_ROW], pagination: { limit: 20, has_more: false } }),
      ),
      http.get('/v1/daemon/works/work-1/cron', () => HttpResponse.json(CRON_DEFAULTS)),
      http.put('/v1/daemon/works/work-1/cron', () =>
        HttpResponse.json(
          { success: false, error: { code: 'conflict', message: 'schedule_json changed by another writer' } },
          { status: 409 },
        ),
      ),
    );

    renderScheduleWithCreator();

    await screen.findByText('My Novel');
    await user.click(screen.getByRole('button', { name: 'Edit cron for Work work-1' }));
    await screen.findByRole('heading', { name: 'Edit Work cron' });

    await user.click(screen.getByRole('button', { name: 'Save' }));

    // Visible conflict alert with a reload CTA; the dialog stays open.
    const conflict = await screen.findByTestId('work-cron-conflict');
    expect(conflict).toHaveTextContent(/Config changed elsewhere/i);
    expect(screen.getByRole('button', { name: 'Reload latest' })).toBeInTheDocument();
    expect(screen.getByRole('heading', { name: 'Edit Work cron' })).toBeInTheDocument();
  });

  it('surfaces an invalid-cron 400 with the stable code visibly', async () => {
    const user = userEvent.setup();
    useHandlers(
      http.get('/v1/daemon/presets', () => HttpResponse.json(PRESETS)),
      http.get('/v1/daemon/orchestration/schedules', () => HttpResponse.json(SCHEDULES_EMPTY)),
      http.get('/v1/daemon/works', () =>
        HttpResponse.json({ items: [WORK_ROW], pagination: { limit: 20, has_more: false } }),
      ),
      http.get('/v1/daemon/works/work-1/cron', () => HttpResponse.json(CRON_DEFAULTS)),
      http.put('/v1/daemon/works/work-1/cron', () =>
        HttpResponse.json(
          {
            success: false,
            error: {
              code: 'bad_request',
              message: '[E_CRON_INVALID_EXPR] invalid cron expression: \'not a cron\'',
            },
          },
          { status: 400 },
        ),
      ),
    );

    renderScheduleWithCreator();

    await screen.findByText('My Novel');
    await user.click(screen.getByRole('button', { name: 'Edit cron for Work work-1' }));
    await screen.findByRole('heading', { name: 'Edit Work cron' });

    const brainstorm = screen.getByLabelText('Brainstorm');
    await user.clear(brainstorm);
    await user.type(brainstorm, 'not a cron');
    await user.click(screen.getByRole('button', { name: 'Save' }));

    // The daemon message (with its stable code) surfaces inline.
    const error = await screen.findByTestId('work-cron-error');
    expect(error).toHaveTextContent('E_CRON_INVALID_EXPR');
    expect(error).toHaveTextContent('invalid cron expression');
  });

  // ── V1.171 P2 delete journey (PL-15/AR-31) ────────────────────────────────

  it('deletes a schedule after confirmation: DELETE called, list invalidated, row gone', async () => {
    const user = userEvent.setup();
    let deleted = false;
    let deletedId: string | null = null;
    useHandlers(
      http.get('/v1/daemon/works', () => HttpResponse.json(WORKS_EMPTY)),
      http.get('/v1/daemon/presets', () => HttpResponse.json(PRESETS)),
      http.get('/v1/daemon/orchestration/schedules', () =>
        HttpResponse.json({
          items: deleted ? [] : [SCHEDULE_ROW],
          pagination: { limit: 20, has_more: false },
        }),
      ),
      http.delete('/v1/daemon/orchestration/schedules/sched-1', ({ request }) => {
        deleted = true;
        deletedId = new URL(request.url).pathname;
        return HttpResponse.json({ deleted: true });
      }),
    );

    renderScheduleWithCreator();

    await screen.findByText('Daily digest');
    await user.click(screen.getByRole('button', { name: 'Delete schedule sched-1' }));

    // Confirmation dialog names the schedule.
    await screen.findByRole('heading', { name: 'Delete "Daily digest"' });
    expect(screen.getByText(/This schedule will be removed from the daemon/i)).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: /^Delete$/ }));

    // DELETE called against the right id.
    await waitFor(() => expect(deleted).toBe(true));
    expect(deletedId).toBe('/v1/daemon/orchestration/schedules/sched-1');
    // Invalidation refetched the list; the row is gone and the dialog closed.
    await waitFor(() => expect(screen.queryByText('Daily digest')).not.toBeInTheDocument());
    await waitFor(() =>
      expect(screen.queryByRole('heading', { name: 'Delete "Daily digest"' })).not.toBeInTheDocument(),
    );
  });

  it('surfaces a non-terminal 4xx visibly and keeps the confirm dialog open', async () => {
    const user = userEvent.setup();
    let deleted = false;
    useHandlers(
      http.get('/v1/daemon/works', () => HttpResponse.json(WORKS_EMPTY)),
      http.get('/v1/daemon/presets', () => HttpResponse.json(PRESETS)),
      http.get('/v1/daemon/orchestration/schedules', () =>
        HttpResponse.json({
          items: [SCHEDULE_ROW],
          pagination: { limit: 20, has_more: false },
        }),
      ),
      http.delete('/v1/daemon/orchestration/schedules/sched-1', () => {
        deleted = true;
        return HttpResponse.json(
          {
            success: false,
            error: {
              code: 'not_found',
              message: 'schedule sched-1 not found',
            },
          },
          { status: 404 },
        );
      }),
    );

    renderScheduleWithCreator();

    await screen.findByText('Daily digest');
    await user.click(screen.getByRole('button', { name: 'Delete schedule sched-1' }));
    await screen.findByRole('heading', { name: 'Delete "Daily digest"' });
    await user.click(screen.getByRole('button', { name: /^Delete$/ }));

    // The daemon message surfaces visibly (error toast) — never silent.
    await screen.findByText('schedule sched-1 not found');
    // Inline error keeps the dialog open.
    expect(
      screen.getByText(/Could not delete the schedule\. Check the daemon message above and try again/i),
    ).toBeInTheDocument();
    expect(screen.getByRole('heading', { name: 'Delete "Daily digest"' })).toBeInTheDocument();
    expect(deleted).toBe(true);
  });

  it('cancel closes the confirm dialog without calling DELETE', async () => {
    const user = userEvent.setup();
    let deleted = false;
    useHandlers(
      http.get('/v1/daemon/works', () => HttpResponse.json(WORKS_EMPTY)),
      http.get('/v1/daemon/presets', () => HttpResponse.json(PRESETS)),
      http.get('/v1/daemon/orchestration/schedules', () =>
        HttpResponse.json({
          items: [SCHEDULE_ROW],
          pagination: { limit: 20, has_more: false },
        }),
      ),
      http.delete('/v1/daemon/orchestration/schedules/sched-1', () => {
        deleted = true;
        return HttpResponse.json({ deleted: true });
      }),
    );

    renderScheduleWithCreator();

    await screen.findByText('Daily digest');
    await user.click(screen.getByRole('button', { name: 'Delete schedule sched-1' }));
    await screen.findByRole('heading', { name: 'Delete "Daily digest"' });

    await user.click(screen.getByRole('button', { name: 'Cancel' }));

    // Dialog closes; the row stays; DELETE was never fired.
    await waitFor(() =>
      expect(screen.queryByRole('heading', { name: 'Delete "Daily digest"' })).not.toBeInTheDocument(),
    );
    expect(screen.getByText('Daily digest')).toBeInTheDocument();
    expect(deleted).toBe(false);
  });

  // ── V1.171 P2 T4 honesty sweep (PL-17 / AR-30) ──────────────────────────

  it('shows the schedule list honestly: last-updated column, no next-run clock, no firing-cadence promise (PL-17/AR-30)', async () => {
    useHandlers(
      http.get('/v1/daemon/works', () => HttpResponse.json(WORKS_EMPTY)),
      http.get('/v1/daemon/orchestration/schedules', () =>
        HttpResponse.json({
          items: [SCHEDULE_ROW],
          pagination: { limit: 20, has_more: false },
        }),
      ),
    );

    renderScheduleWithCreator();

    await screen.findByText('Daily digest');
    // Lifecycle fields surface honestly: status badge + relative last-updated.
    expect(screen.getByText(/Updated/i)).toBeInTheDocument();
    // No fabricated next-run / next-fire anywhere on the page.
    expect(screen.queryByText(/next run/i)).not.toBeInTheDocument();
    expect(screen.queryByText(/next fire/i)).not.toBeInTheDocument();
    expect(screen.queryByText(/fires every/i)).not.toBeInTheDocument();
    expect(screen.queryByText(/cadence/i)).not.toBeInTheDocument();
    // The page copy names the two surfaces it actually shows: scheduled runs + cron roles.
    expect(
      screen.getByText(/Scheduled preset runs and per-Work cron roles, with status and last update/i),
    ).toBeInTheDocument();
  });

  it('never promises firing cadence on the empty list either (AR-30)', async () => {
    useHandlers(
      http.get('/v1/daemon/works', () => HttpResponse.json(WORKS_EMPTY)),
      http.get('/v1/daemon/orchestration/schedules', () => HttpResponse.json(SCHEDULES_EMPTY)),
    );

    renderScheduleWithCreator();

    await screen.findByText('No schedules');
    expect(screen.queryByText(/next run/i)).not.toBeInTheDocument();
    expect(screen.queryByText(/next fire/i)).not.toBeInTheDocument();
    expect(screen.queryByText(/fires every/i)).not.toBeInTheDocument();
    expect(screen.queryByText(/cadence/i)).not.toBeInTheDocument();
  });
});
