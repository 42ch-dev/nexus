/**
 * fork-create-from-timeline-event — V1.162 P2 T1 fork creation flow +
 * PD-6 post-create landing (World Timeline only).
 *
 * Coverage (the brief's three named tests):
 *   1. fork_create_from_timeline_event  — pick a fork point (compute node
 *      inspector affordance) → submit → 200 → the forked branch Timeline
 *      is the active view (branch context = new branch_id).
 *   2. fork_create_bad_fork_point_surfaces_422 — daemon 422 `invalid_input`
 *      → inline dialog error; parent Timeline stays active (no branch
 *      switch).
 *   3. fork_create_post_create_landing  — PD-6: success does NOT leave the
 *      author on a parent-only toast; the active view is the forked branch
 *      Timeline (the events query re-keys to the new branch_id).
 *
 * Mount strategy mirrors `compute-events-canvas.test.tsx`: the real
 * `TimelineCanvas` over MSW daemon handlers. The events route is
 * branch-scoped (`branch_id` param → per-branch store) so the PD-6 re-key
 * proves the landing; the forks route simulates P1's create response.
 */
import { delay, http, HttpResponse, type JsonBodyType } from 'msw';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import { renderInApp } from '@/test/test-providers';
import { useHandlers } from '@/test/msw-server';
import { BrowserClient } from '@/lib/nexus';
import { i18n } from '@/lib/i18n/config';
import { TimelineCanvas } from '../timeline-canvas';

// ─── Wire fixtures (daemon contract shapes) ─────────────────────────────────

const KB_EVENT = {
  key_block_id: 'kb-coro',
  world_id: 'world-7',
  block_type: 'event',
  canonical_name: 'The coronation',
  status: 'confirmed',
  version: 1,
  body: { attributes: { occurred_at: '2026-07-01T00:00:00Z' } },
};

const ROOT_BRANCH_ID = 'fbk_root';
const FORK_BRANCH_ID = 'fbk_new_1';

const COMPUTE_EVENT = {
  id: 'evt_compute_1',
  branch_id: ROOT_BRANCH_ID,
  event_type: 'compute_result',
  status: 'canon',
  sequence_no: 2,
  title: 'Aria strikes Brann',
  summary: 'Brann takes 6 damage and staggers back.',
  affected_key_block_ids: ['kb-aria', 'kb-brann'],
  metadata: {},
  extensions: {
    compute: {
      module_id: 'basic-combat',
      module_version: '1.0.0',
      run_id: 'run_9',
      source_kind: 'direct_invoke',
    },
  },
  created_at: '2026-08-01T00:00:00Z',
};

const RUN_DETAIL = {
  run_id: 'run_9',
  status: 'applied',
  module_id: 'basic-combat',
  module_version: '1.0.0',
  world_id: 'world-7',
  invocation_params: { attacker_id: 'kb-aria', defender_id: 'kb-brann' },
  proposals: {},
  created_at: '2026-08-01T01:00:00Z',
};

const CREATE_FORK_RESPONSE = {
  branch_id: FORK_BRANCH_ID,
  parent_branch_id: ROOT_BRANCH_ID,
  forked_from_event_id: 'evt_compute_1',
  created_at: '2026-08-12T00:00:00Z',
};

// ─── MSW handler set with a mutable per-branch event store ──────────────────

function forkJourney(over: { initialEvents?: unknown[] } = {}) {
  const state = {
    eventsByBranch: {
      [ROOT_BRANCH_ID]: [...(over.initialEvents ?? [COMPUTE_EVENT])],
      [FORK_BRANCH_ID]: [] as unknown[],
    } as Record<string, unknown[]>,
    lastEventsUrl: null as string | null,
    /** Events fetches that carried a `branch_id` query param. */
    branchFetchCount: 0,
    forkCalls: [] as Array<Record<string, unknown>>,
    forkStatus: 200 as number,
    forkErrorBody: null as JsonBodyType | null,
    /** W-3 — artificial latency for the POST so a pending mutation is
     * observable while the dialog stays open. */
    forkDelayMs: 0,
  };

  const handlers = [
    http.get('/v1/daemon/worlds/:worldId/kb/graph', () =>
      HttpResponse.json({
        entities: [KB_EVENT],
        source_anchors: [],
        relationships: [],
      }),
    ),
    http.get('/v1/daemon/worlds/:worldId/timeline/events', ({ request }) => {
      state.lastEventsUrl = request.url;
      const url = new URL(request.url);
      const branchId = url.searchParams.get('branch_id');
      if (branchId) state.branchFetchCount += 1;
      const items = state.eventsByBranch[branchId ?? ROOT_BRANCH_ID] ?? [];
      return HttpResponse.json({ items, has_more: false });
    }),
    http.post('/v1/daemon/worlds/:worldId/forks', async ({ request }) => {
      // Record the request BEFORE the artificial delay: the double-submit
      // test needs the count visible while the mutation is still pending.
      // (Recording after `delay` would only observe resolved mutations —
      // the W-3 false-pass window.)
      const body = (await request.json()) as Record<string, unknown>;
      state.forkCalls.push(body);
      if (state.forkDelayMs) await delay(state.forkDelayMs);
      if (state.forkStatus !== 200) {
        return HttpResponse.json(state.forkErrorBody ?? {}, { status: state.forkStatus });
      }
      return HttpResponse.json(CREATE_FORK_RESPONSE);
    }),
    http.get('/v1/daemon/works', () =>
      HttpResponse.json({ items: [], pagination: { limit: 20, has_more: false } }),
    ),
    http.get('/v1/daemon/compute/modules', () =>
      HttpResponse.json({ items: [], has_more: false }),
    ),
    http.get('/v1/daemon/narrative/worlds', () =>
      HttpResponse.json({
        worlds: [{ world_id: 'world-7', title: 'Test World' }],
      }),
    ),
    http.get('/v1/daemon/compute/runs/:runId', () =>
      HttpResponse.json(RUN_DETAIL),
    ),
    http.post('/v1/daemon/agent-host/scan', () =>
      HttpResponse.json({ agents: [] }),
    ),
  ];

  return { state, handlers };
}

function renderForkApp(over: { initialEvents?: unknown[] } = {}) {
  const { state, handlers } = forkJourney(over);
  useHandlers(...handlers);
  const result = renderInApp(
    <TimelineCanvas worldId="world-7" />,
    { client: new BrowserClient(), initialRouterEntries: ['/worlds/world-7/timeline'] },
  );
  return { ...result, state };
}

/** Select the compute node via the alt-view table → compute inspector opens. */
async function openComputeInspector(user: ReturnType<typeof userEvent.setup>) {
  await user.click(await screen.findByRole('button', { name: 'Show list view' }));
  const row = await screen.findByText('Aria strikes Brann');
  fireEvent.click(row.closest('tr')!);
  await screen.findByTestId('timeline-compute-inspector');
  const forkButton = await screen.findByTestId('compute-inspector-fork-here');
  expect(forkButton).toHaveTextContent("Branch this world's timeline from here");
}

beforeEach(async () => {
  await i18n.changeLanguage('en');
});

afterEach(() => {
  vi.restoreAllMocks();
});

// ─── Tests ──────────────────────────────────────────────────────────────────

describe('TimelineCanvas fork creation (V1.162 P2 T1)', () => {
  it('fork_create_from_timeline_event — pick fork-point → submit → 200 → forked branch Timeline is the active view', async () => {
    const user = userEvent.setup();
    const { state } = renderForkApp();

    // Pick the fork point: compute inspector → "Branch this world's
    // timeline from here" → fork-create dialog.
    await openComputeInspector(user);
    await user.click(screen.getByTestId('compute-inspector-fork-here'));
    await screen.findByTestId('fork-create-submit');
    expect(screen.getByTestId('fork-create-fork-point')).toHaveTextContent(
      'Aria strikes Brann',
    );

    // Optional label, then submit.
    await user.type(screen.getByLabelText(/Branch label/i), 'Alternate ending');
    await user.click(screen.getByTestId('fork-create-submit'));

    // Request shape: parent = the current branch context (root), fork point
    // = the picked event, label forwarded.
    await waitFor(() => expect(state.forkCalls).toHaveLength(1));
    expect(state.forkCalls[0]).toEqual({
      parent_branch_id: ROOT_BRANCH_ID,
      forked_from_event_id: 'evt_compute_1',
      label: 'Alternate ending',
    });

    // PD-6: the active branch context becomes the new branch — the events
    // query re-keys to the forked branch.
    await waitFor(() => {
      const url = new URL(state.lastEventsUrl!, 'http://localhost');
      expect(url.searchParams.get('branch_id')).toBe(FORK_BRANCH_ID);
    });
    // The parent branch's compute row is gone — the view is the forked
    // branch Timeline (KB graph remains; the fork branch has no compute
    // events yet). The alt-view table reflects the active branch's nodes.
    await waitFor(() =>
      expect(screen.queryByText('Aria strikes Brann')).not.toBeInTheDocument(),
    );
    expect(screen.getByText('The coronation')).toBeInTheDocument();
    // Brief success notice (PD-6).
    expect(await screen.findByText('Branch created')).toBeInTheDocument();
  });

  it('fork_create_bad_fork_point_surfaces_422 — inline error; parent Timeline remains active', async () => {
    const user = userEvent.setup();
    const { state } = renderForkApp();
    state.forkStatus = 422;
    state.forkErrorBody = {
      success: false,
      error: {
        code: 'invalid_input',
        message: 'fork point not found on parent branch',
        details: { fork_point: 'fork point not found on parent branch' },
      },
    };

    await openComputeInspector(user);
    await user.click(screen.getByTestId('compute-inspector-fork-here'));
    await screen.findByTestId('fork-create-submit');
    await user.click(screen.getByTestId('fork-create-submit'));

    // Inline 422 error; the dialog stays open (author can pick another
    // fork point).
    await screen.findByTestId('fork-create-dialog-error');
    expect(screen.getByTestId('fork-create-dialog-error')).toHaveTextContent(
      /could not branch from this event/i,
    );
    expect(screen.getByTestId('fork-create-submit')).toBeInTheDocument();

    // No branch switch: every events fetch stayed on the root branch.
    await waitFor(() => expect(state.lastEventsUrl).not.toBeNull());
    expect(state.branchFetchCount).toBe(0);
    expect(
      new URL(state.lastEventsUrl!, 'http://localhost').searchParams.get('branch_id'),
    ).toBeNull();
    expect(screen.queryByText('Branch created')).not.toBeInTheDocument();

    // Closing the dialog leaves the parent Timeline intact — the compute
    // row is still there (the alt-view table reflects the active branch's
    // nodes; the Radix dialog hid the background from the a11y tree).
    await user.click(screen.getByRole('button', { name: 'Cancel' }));
    expect(screen.getByText('Aria strikes Brann')).toBeInTheDocument();
  });

  it('fork_create_post_create_landing — PD-6: success enters the forked Timeline, not a parent-only toast', async () => {
    const user = userEvent.setup();
    const { state } = renderForkApp();

    await openComputeInspector(user);
    await user.click(screen.getByTestId('compute-inspector-fork-here'));
    await screen.findByTestId('fork-create-submit');
    await user.click(screen.getByTestId('fork-create-submit'));

    // The success notice appears…
    expect(await screen.findByText('Branch created')).toBeInTheDocument();
    // …but is NOT the only outcome: the active view is the forked branch
    // Timeline (branch context switched to the response branch_id — the
    // events query re-keyed). A toast-only-on-parent implementation fails
    // this assertion (lastEventsUrl would still carry no branch_id).
    await waitFor(() => {
      const url = new URL(state.lastEventsUrl!, 'http://localhost');
      expect(url.searchParams.get('branch_id')).toBe(FORK_BRANCH_ID);
    });
    await waitFor(() =>
      expect(screen.queryByText('Aria strikes Brann')).not.toBeInTheDocument(),
    );
  });

  it('fork_create_rapid_double_submit_single_post — double submit fires exactly ONE POST (W-3)', async () => {
    const user = userEvent.setup();
    const { state } = renderForkApp();
    // Keep the POST in flight so the mutation stays pending while the
    // dialog is open (the double-submit window the QC flagged).
    state.forkDelayMs = 100;

    await openComputeInspector(user);
    await user.click(screen.getByTestId('compute-inspector-fork-here'));
    await screen.findByTestId('fork-create-submit');

    // Submit once — the mutation is now pending: `forkCalls` is recorded
    // before the MSW delay, so `waitFor` resolves WHILE the POST is still
    // in flight and the dialog is still open.
    const form = screen.getByTestId('fork-create-submit').closest('form')!;
    fireEvent.submit(form);
    await waitFor(() => expect(state.forkCalls).toHaveLength(1));

    // A second submit attempt while pending — the implicit-Enter /
    // pre-render double-click vector that bypasses the disabled button.
    // The in-handler `isPending` guard must swallow it: exactly ONE POST
    // (duplicate fork branches are the W-3 bug).
    fireEvent.submit(form);
    // Settle window > the 100 ms MSW delay: a buggy second POST would be
    // recorded (before its own delay) inside this window, so a missing
    // guard now fails the length-1 assertion.
    await new Promise((resolve) => {
      setTimeout(resolve, 150);
    });
    expect(state.forkCalls).toHaveLength(1);

    // The single POST carries the correct shape…
    expect(state.forkCalls[0]).toEqual({
      parent_branch_id: ROOT_BRANCH_ID,
      forked_from_event_id: 'evt_compute_1',
    });
    // …and the flow still lands on the forked branch (PD-6).
    expect(await screen.findByText('Branch created')).toBeInTheDocument();
    await waitFor(() => {
      const url = new URL(state.lastEventsUrl!, 'http://localhost');
      expect(url.searchParams.get('branch_id')).toBe(FORK_BRANCH_ID);
    });
  });
});
