/**
 * `useSteerStrategy` — Steer ordering and partial-failure truthfulness
 * (V1.195 P0-T7; spec `current-host-contracts.md` §3.3 / W5, S0-3).
 *
 * Steer is two sequential calls, not one transaction: the Idea is appended to
 * the schedule's core context first, and only a committed append is followed by
 * the `resume` signal. So a refused append must send no resume at all, and a
 * refused resume must leave the appended version durable while the Steer as a
 * whole reports the refusal — never an overall success, never an automatic
 * re-append.
 *
 * These cases drive the real `BrowserClient` against msw, so the observed
 * request order, refusal codes and refresh behavior are transport-level rather
 * than mocked-hook behavior.
 */
import { http, HttpResponse } from 'msw';
import { beforeEach, describe, expect, it } from 'vitest';
import { fireEvent, screen, waitFor } from '@testing-library/react';

import { usePresetSchedules, useSteerStrategy } from '@/lib/canvas/use-strategy-data';
import { i18n } from '@/lib/i18n/config';
import { BrowserClient } from '@/lib/nexus';
import { useHandlers } from '@/test/msw-server';
import { renderInApp } from '@/test/test-providers';

const SCHEDULES_PATH = '/v1/daemon/orchestration/schedules';
const CORE_CONTEXT_PATH = `${SCHEDULES_PATH}/:scheduleId/core-context`;
const SIGNAL_PATH = `${SCHEDULES_PATH}/:scheduleId/signal`;

const IDEA = 'Add a lighthouse keeper';

/** One schedule row for the preset under test, carrying its visible version. */
function listResponse(coreContextVersion: number) {
  return {
    items: [
      {
        schedule_id: 'sch-1',
        creator_id: 'creator-1',
        preset_id: 'preset-1',
        status: 'waiting',
        label: 'Steer target',
        current_core_context_version: coreContextVersion,
        created_at: '2026-09-23T00:00:00Z',
        updated_at: '2026-09-23T00:00:00Z',
      },
    ],
    pagination: { limit: 20, has_more: false },
  };
}

/**
 * Harness exposing the Steer trigger, the mutation state and the version the
 * schedule row shows (the surface the partial-failure refresh must not leave
 * stale).
 */
function SteerHarness() {
  const schedules = usePresetSchedules('preset-1');
  const steer = useSteerStrategy();
  const state = steer.isPending
    ? 'pending'
    : steer.isSuccess
      ? 'success'
      : steer.isError
        ? 'error'
        : 'idle';
  return (
    <div>
      <button type="button" onClick={() => steer.mutate({ scheduleId: 'sch-1', idea: IDEA })}>
        Steer
      </button>
      <span data-testid="state">{state}</span>
      <span data-testid="version">{schedules.data?.[0]?.current_core_context_version ?? 'none'}</span>
    </div>
  );
}

beforeEach(async () => {
  await i18n.changeLanguage('en');
});

describe('useSteerStrategy — append before resume (W5 / S0-3)', () => {
  it('appends the Idea, then signals resume, and only then counts as success', async () => {
    const order: string[] = [];
    let appendBody: unknown = null;
    let signalBody: unknown = null;
    let committedVersion = 0;
    useHandlers(
      http.get(SCHEDULES_PATH, () => HttpResponse.json(listResponse(committedVersion))),
      http.patch(CORE_CONTEXT_PATH, async ({ request }) => {
        order.push('append');
        appendBody = await request.json();
        committedVersion = 1;
        return HttpResponse.json({ new_version: 1 });
      }),
      http.post(SIGNAL_PATH, async ({ request }) => {
        order.push('resume');
        signalBody = await request.json();
        return HttpResponse.json({ schedule_id: 'sch-1', status: 'running' });
      }),
    );

    renderInApp(<SteerHarness />, { client: new BrowserClient() });
    await waitFor(() => expect(screen.getByTestId('version')).toHaveTextContent('0'));

    fireEvent.click(screen.getByRole('button', { name: 'Steer' }));

    await waitFor(() => expect(screen.getByTestId('state')).toHaveTextContent('success'));
    // The append is the first request; the resume rides on a committed append.
    expect(order).toEqual(['append', 'resume']);
    expect(appendBody).toEqual({ op: 'append', body: IDEA });
    expect(signalBody).toEqual({ signal: 'resume' });
    await waitFor(() => expect(screen.getByTestId('version')).toHaveTextContent('1'));
    expect(await screen.findByText('Idea sent to Preset')).toBeInTheDocument();
  });

  it('sends no resume when the append is refused (a failed append is a failed Steer)', async () => {
    const order: string[] = [];
    useHandlers(
      http.get(SCHEDULES_PATH, () => HttpResponse.json(listResponse(0))),
      http.patch(CORE_CONTEXT_PATH, () => {
        order.push('append');
        return HttpResponse.json(
          { success: false, error: { code: 'invalid_input', message: 'append refused' } },
          { status: 422 },
        );
      }),
      http.post(SIGNAL_PATH, () => {
        order.push('resume');
        return HttpResponse.json({ schedule_id: 'sch-1', status: 'running' });
      }),
    );

    renderInApp(<SteerHarness />, { client: new BrowserClient() });
    await waitFor(() => expect(screen.getByTestId('version')).toHaveTextContent('0'));

    fireEvent.click(screen.getByRole('button', { name: 'Steer' }));

    await waitFor(() => expect(screen.getByTestId('state')).toHaveTextContent('error'));
    expect(order).toEqual(['append']);
    expect(screen.queryByText('Idea sent to Preset')).not.toBeInTheDocument();
    expect(await screen.findByText('Could not steer Harness')).toBeInTheDocument();
  });

  it('keeps the committed append and reports the refused resume instead of success', async () => {
    const order: string[] = [];
    let committedVersion = 0;
    useHandlers(
      http.get(SCHEDULES_PATH, () => HttpResponse.json(listResponse(committedVersion))),
      http.patch(CORE_CONTEXT_PATH, () => {
        order.push('append');
        committedVersion = 2;
        return HttpResponse.json({ new_version: 2 });
      }),
      http.post(SIGNAL_PATH, () => {
        order.push('resume');
        // The exact wire body the service returns for a lawful manual-wait
        // refusal: status 409, public `code` is the generic class, and the
        // coded conflict rides in `details.wire_code`.
        return HttpResponse.json(
          {
            success: false,
            error: {
              code: 'invalid_input',
              message: 'run refuses the signal: run is terminal or not in a signalable state',
              details: { wire_code: 'workflow_state_conflict' },
            },
          },
          { status: 409 },
        );
      }),
    );

    renderInApp(<SteerHarness />, { client: new BrowserClient() });
    await waitFor(() => expect(screen.getByTestId('version')).toHaveTextContent('0'));

    fireEvent.click(screen.getByRole('button', { name: 'Steer' }));

    await waitFor(() => expect(screen.getByTestId('state')).toHaveTextContent('error'));
    // Exactly one append and one resume: the durable append is never replayed.
    expect(order).toEqual(['append', 'resume']);
    // A refused resume is never an overall success.
    expect(screen.queryByText('Idea sent to Preset')).not.toBeInTheDocument();
    // The lawful manual-wait conflict is reported as a conflict, naming the
    // version the append actually committed.
    expect(
      await screen.findByText(
        (content) => content.includes('core context version 2') && content.includes('state conflict'),
      ),
    ).toBeInTheDocument();
    // The visible row is refreshed, so the committed version is not stale.
    await waitFor(() => expect(screen.getByTestId('version')).toHaveTextContent('2'));
  });

  it('reports a non-conflict resume refusal as a refusal, not as success', async () => {
    const order: string[] = [];
    useHandlers(
      http.get(SCHEDULES_PATH, () => HttpResponse.json(listResponse(0))),
      http.patch(CORE_CONTEXT_PATH, () => {
        order.push('append');
        return HttpResponse.json({ new_version: 1 });
      }),
      http.post(SIGNAL_PATH, () => {
        order.push('resume');
        return HttpResponse.json(
          { success: false, error: { code: 'service_unavailable', message: 'engine closing' } },
          { status: 503 },
        );
      }),
    );

    renderInApp(<SteerHarness />, { client: new BrowserClient() });
    await waitFor(() => expect(screen.getByTestId('version')).toHaveTextContent('0'));

    fireEvent.click(screen.getByRole('button', { name: 'Steer' }));

    await waitFor(() => expect(screen.getByTestId('state')).toHaveTextContent('error'));
    expect(order).toEqual(['append', 'resume']);
    expect(screen.queryByText('Idea sent to Preset')).not.toBeInTheDocument();
    expect(
      await screen.findByText(
        (content) => content.includes('core context version 1') && !content.includes('state conflict'),
      ),
    ).toBeInTheDocument();
  });
});
