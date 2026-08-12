/**
 * fork-lineage-badge — V1.162 P2 T2 read-only fork lineage chrome
 * (spec §3.3.2 promoted to real; carrier B branch-level projection).
 *
 * Coverage (the brief's four named tests):
 *   1. fork_badge_shown_for_forked_branch — the active branch carries a
 *      `fork_created` canon marker → fork-badge + parent branch + fork-point
 *      event render (from `extensions.fork_lineage`, spec §6.6.3).
 *   2. fork_badge_hidden_for_root_branch — no marker on the active branch
 *      → NO badge / NO fork chrome (nothing world-level drives it).
 *   3. fork_lineage_read_only — the chrome offers NO merge / edit /
 *      multi-branch affordance; the ONLY control is the parent hop.
 *   4. fork_lineage_parent_hop — activating the parent control reuses T1's
 *      branch-context mechanism: `activeBranchId = parent_branch_id` → the
 *      timeline-events query re-keys to the parent branch (observable via
 *      the `branch_id` param on the daemon reads).
 *
 * Mount strategy mirrors `fork-create-from-timeline-event.test.tsx` (the
 * real `TimelineCanvas` over MSW daemon handlers). The events route is
 * branch-scoped: the fork branch's store carries the canon `fork_created`
 * marker with `fork_lineage`, the root branch carries none — exactly the
 * P1 projection contract (`0-or-1` marker; root has no marker).
 */
import { http, HttpResponse } from 'msw';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, screen, waitFor, within } from '@testing-library/react';

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
const FORK_BRANCH_ID = 'fbk_fork_1';
const FORK_POINT_EVENT_ID = 'evt_fp_1';

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

/** P1 carrier B — the fork branch's canon `fork_created` marker. */
const FORK_MARKER = {
  id: 'evt_fork_marker_1',
  branch_id: FORK_BRANCH_ID,
  event_type: 'fork_created',
  status: 'canon',
  sequence_no: 1,
  title: 'Branch created',
  summary: null,
  metadata: {},
  extensions: {
    fork_lineage: {
      parent_branch_id: ROOT_BRANCH_ID,
      forked_from_event_id: FORK_POINT_EVENT_ID,
      label: 'Alternate ending',
    },
  },
  created_at: '2026-08-12T00:00:00Z',
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

// ─── MSW handler set with a mutable per-branch event store ──────────────────

function forkChrome(over: { initialEvents?: unknown[] } = {}) {
  const state = {
    eventsByBranch: {
      // The fork branch store carries the canon fork_created marker (and
      // no compute events — the marker must not project as a node).
      [FORK_BRANCH_ID]: [FORK_MARKER],
      [ROOT_BRANCH_ID]: [...(over.initialEvents ?? [COMPUTE_EVENT])],
    } as Record<string, unknown[]>,
    lastEventsUrl: null as string | null,
    branchFetchCount: 0,
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
      // Mirror the daemon's filter contract: the route honors
      // `event_type` / `status` / `branch_id`. The lineage hook reads
      // `event_type=fork_created&status=canon`; the canvas reads
      // `event_type=compute_result&status=canon`. A branch whose store
      // holds only other families must return ZERO markers for the
      // fork_created read — exactly the P1 projection contract
      // (`0-or-1` marker; root has no marker).
      const eventType = url.searchParams.get('event_type');
      const status = url.searchParams.get('status');
      const branchEvents = state.eventsByBranch[branchId ?? ROOT_BRANCH_ID] ?? [];
      const items =
        eventType || status
          ? branchEvents.filter((e) => {
              const row = e as { event_type: string; status: string };
              return (
                (!eventType || row.event_type === eventType) &&
                (!status || row.status === status)
              );
            })
          : branchEvents;
      return HttpResponse.json({ items, has_more: false });
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

function renderForkChrome(over: { initialEvents?: unknown[]; branch?: string } = {}) {
  const { state, handlers } = forkChrome(over);
  useHandlers(...handlers);
  const initial =
    over.branch === undefined
      ? ['/worlds/world-7/timeline']
      : [`/worlds/world-7/timeline?branch=${over.branch}`];
  const result = renderInApp(
    <TimelineCanvas worldId="world-7" />,
    { client: new BrowserClient(), initialRouterEntries: initial },
  );
  return { ...result, state };
}

beforeEach(async () => {
  await i18n.changeLanguage('en');
});

afterEach(() => {
  vi.restoreAllMocks();
});

// ─── Tests ──────────────────────────────────────────────────────────────────

describe('TimelineCanvas fork lineage chrome (V1.162 P2 T2)', () => {
  it('fork_badge_shown_for_forked_branch — marker present → badge + parent + fork-point rendered', async () => {
    // Active branch = the fork (`?branch=fbk_fork_1` restores it from the
    // URL, the T1 mirror). Its store carries the canon fork_created marker.
    const { state } = renderForkChrome({ branch: FORK_BRANCH_ID });

    // Badge renders (marker-derived is_fork).
    const badge = await screen.findByTestId('fork-lineage-badge');
    expect(badge).toHaveTextContent('This branch is a fork');

    // Parent branch + fork-point event read-only display (fork_lineage).
    expect(within(badge).getByTestId('fork-lineage-parent')).toHaveTextContent(
      `Parent branch: ${ROOT_BRANCH_ID}`,
    );
    expect(within(badge).getByTestId('fork-lineage-fork-point')).toHaveTextContent(
      `Forked from event: ${FORK_POINT_EVENT_ID}`,
    );
    // The one-hop control is present.
    expect(within(badge).getByTestId('fork-lineage-open-parent')).toBeInTheDocument();

    // The marker itself is NOT a canvas node — branch-level lineage chrome
    // only (the marker row is consumed by the lineage hook, not projected).
    await waitFor(() => expect(state.lastEventsUrl).not.toBeNull());
    expect(
      new URL(state.lastEventsUrl!, 'http://localhost').searchParams.get('branch_id'),
    ).toBe(FORK_BRANCH_ID);
  });

  it('fork_badge_hidden_for_root_branch — no marker → no badge / no fork chrome', async () => {
    const { state } = renderForkChrome();

    // Wait for the events read (root branch) to settle, then assert the
    // chrome is absent — nothing world-level drives it.
    await waitFor(() => expect(state.lastEventsUrl).not.toBeNull());
    expect(
      new URL(state.lastEventsUrl!, 'http://localhost').searchParams.get('branch_id'),
    ).toBeNull();
    expect(screen.queryByTestId('fork-lineage-badge')).not.toBeInTheDocument();
    expect(screen.queryByText('This branch is a fork')).not.toBeInTheDocument();
    expect(screen.queryByTestId('fork-lineage-open-parent')).not.toBeInTheDocument();
  });

  it('fork_lineage_read_only — no merge/edit/multi-branch affordances present', async () => {
    renderForkChrome({ branch: FORK_BRANCH_ID });

    const badge = await screen.findByTestId('fork-lineage-badge');
    // The chrome's ONLY control is the one-hop parent hop. No merge, no
    // edit, no branch-compare affordance exists anywhere in the chrome.
    const buttons = within(badge).getAllByRole('button');
    expect(buttons).toHaveLength(1);
    expect(buttons[0]).toHaveAttribute('data-testid', 'fork-lineage-open-parent');
    expect(within(badge).queryByRole('textbox')).not.toBeInTheDocument();
    expect(within(badge).queryByRole('combobox')).not.toBeInTheDocument();
    expect(badge.textContent).not.toMatch(/merge|compare|edit/i);
  });

  it('fork_lineage_parent_hop — parent control switches active branch context to the parent Timeline', async () => {
    const { state } = renderForkChrome({ branch: FORK_BRANCH_ID });

    const badge = await screen.findByTestId('fork-lineage-badge');
    fireEvent.click(within(badge).getByTestId('fork-lineage-open-parent'));

    // activeBranchId = parent_branch_id → the timeline-events query re-keys
    // to the parent branch (the observable branch-context switch; mirrors
    // T1's PD-6 assertion). Both the canvas events query AND the lineage
    // hook read the active branch, so the daemon reads now carry the parent.
    await waitFor(() => {
      const url = new URL(state.lastEventsUrl!, 'http://localhost');
      expect(url.searchParams.get('branch_id')).toBe(ROOT_BRANCH_ID);
    });

    // The root branch has no fork marker → the fork chrome disappears (the
    // parent Timeline is not a fork).
    await waitFor(() =>
      expect(screen.queryByTestId('fork-lineage-badge')).not.toBeInTheDocument(),
    );
  });
});
