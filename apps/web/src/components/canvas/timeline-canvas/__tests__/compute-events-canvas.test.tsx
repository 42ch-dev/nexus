/**
 * TimelineCanvas — V1.147 P2 T3 compute-on-Timeline integration (AC-I3).
 *
 * Coverage:
 *   - Compute result nodes render on the Narrative layer ALONGSIDE KB event
 *     nodes (two event families, no double-render).
 *   - A World whose only Narrative content is an accepted compute Run is NOT
 *     the empty state — the node renders.
 *   - Run Module entry (toolbar) opens the shared Run Studio (Settings →
 *     Modules) with the World pre-filled; the events query is hard-filtered
 *     to `event_type=compute_result&status=canon` with no branch override
 *     (daemon resolves the World's current branch).
 *   - Compute inspector via the alt-view: compute context sections + Open
 *     Run deep link (Settings → Modules run detail).
 *   - Accept journey: Timeline entry → Run Studio (World pre-filled) → run →
 *     accept → close Settings → the Compute result node appears WITHOUT a
 *     manual refresh (accept invalidates `timeline.all`, which covers the
 *     events query; the canvas stays mounted behind the modal).
 *
 * Mount strategy mirrors `settings-shell.test.tsx`: the real
 * `SettingsModalProvider` + `SettingsModalHost` with a mini route tree so the
 * modal-driven navigation is exercised end-to-end (MSW for the daemon wire).
 */
import { http, HttpResponse } from 'msw';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { Route, Routes, useLocation } from 'react-router';

import {
  SettingsModalProvider,
  useSettingsModal,
} from '@/components/layout/settings-modal-context';
import { SettingsModalHost } from '@/components/layout/settings-modal-host';
import { SettingsModulesSection } from '@/pages/settings/settings-modules-section';
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

const CHARACTERS = [
  {
    key_block_id: 'kb-aria',
    world_id: 'world-7',
    block_type: 'character',
    canonical_name: 'Aria',
    status: 'confirmed',
    version: 1,
  },
  {
    key_block_id: 'kb-brann',
    world_id: 'world-7',
    block_type: 'character',
    canonical_name: 'Brann',
    status: 'confirmed',
    version: 1,
  },
];

const BASIC_COMBAT_MODULE = {
  module_id: 'basic-combat',
  name: 'Basic Combat',
  version: '1.0.0',
  description: 'A simple combat resolution module.',
  required_key_block_types: ['character'],
};

const BASIC_COMBAT_MANIFEST = {
  module_id: 'basic-combat',
  name: 'Basic Combat',
  version: '1.0.0',
  nexus_abi_version: 1,
  required_key_block_types: ['character'],
  compute_export: 'compute',
  init_export: 'init',
  description: 'A simple combat resolution module.',
  author: 'Nexus Team',
  schemas: {
    invocation: {
      type: 'object',
      properties: {
        attacker_id: { type: 'string' },
        defender_id: { type: 'string' },
      },
      required: ['attacker_id', 'defender_id'],
    },
  },
};

const COMPUTE_EVENT = {
  id: 'evt_compute_1',
  branch_id: 'fbk_root',
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

const SUCCESS_PROPOSALS = {
  schema_version: 1,
  state_delta: [
    {
      op: 'sub',
      path: 'character.current_hp',
      target_key_block_id: 'kb-brann',
      value: { amount: 6 },
    },
  ],
  timeline_events: [
    {
      title: 'Aria strikes Brann',
      summary: 'Brann takes 6 damage and staggers back.',
      affected_key_block_ids: ['kb-aria', 'kb-brann'],
    },
  ],
  new_key_blocks: [],
  battle_report: { kind: 'combat', damage: 6 },
};

// ─── Mini app (modal + routes) ──────────────────────────────────────────────

function ModalAppRoutes() {
  const location = useLocation();
  const { open, backgroundLocation } = useSettingsModal();
  const routesLocation = open ? backgroundLocation : location;
  return (
    <Routes location={routesLocation}>
      <Route
        path="worlds/:worldId/timeline"
        element={<TimelineCanvas worldId="world-7" />}
      />
      <Route path="settings/modules" element={<SettingsModulesSection />} />
      <Route path="*" element={<div data-testid="fallback-route" />} />
    </Routes>
  );
}

/** Programmatic Settings close (the Radix dialog has no visible X in tests). */
function SettingsCloseButton() {
  const { open, requestClose } = useSettingsModal();
  if (!open) return null;
  return (
    <button type="button" onClick={() => requestClose()} data-testid="close-settings">
      close settings
    </button>
  );
}

/**
 * Full MSW handler set. `state.eventsStore` is mutable: the accept handler
 * appends the compute_result event (simulating the daemon's Accept write), so
 * the canvas events query refetch picks it up through the `timeline.all`
 * invalidation.
 */
function computeTimelineJourney(over: { initialEvents?: unknown[]; kbEntities?: unknown[] } = {}) {
  const state = {
    eventsStore: [...(over.initialEvents ?? [])],
    lastEventsUrl: null as string | null,
    eventsFetchCount: 0,
    runsStore: [] as unknown[],
    acceptCalls: 0,
  };

  const handlers = [
    http.get('/v1/daemon/worlds/:worldId/kb/graph', () =>
      HttpResponse.json({
        entities: over.kbEntities ?? [KB_EVENT, ...CHARACTERS],
        source_anchors: [],
        relationships: [],
      }),
    ),
    http.get('/v1/daemon/worlds/:worldId/timeline/events', ({ request }) => {
      state.eventsFetchCount += 1;
      // V1.162 P2 T2 — the fork-lineage chrome reads the SAME route with
      // its own `event_type=fork_created&limit=1` filter (separate hook).
      // The contract assertions below target the canvas's compute_result
      // projection query, so only that family updates `lastEventsUrl`;
      // the fork_created read keeps its own filter and must not overwrite
      // the slot the assertions inspect.
      const url = new URL(request.url);
      if (url.searchParams.get('event_type') === 'compute_result') {
        state.lastEventsUrl = request.url;
      }
      // Page the store by the daemon's limit/cursor contract so
      // `has_more: true` + a second page can be exercised (review F1
      // regression: the canvas auto-fetches remaining pages).
      const limit = Number(url.searchParams.get('limit')) || 100;
      const cursorRaw = url.searchParams.get('cursor');
      const cursor = cursorRaw ? Number(cursorRaw) : 0;
      const items = state.eventsStore.slice(cursor, cursor + limit);
      const nextCursor = cursor + limit;
      const hasMore = nextCursor < state.eventsStore.length;
      return HttpResponse.json({
        items,
        has_more: hasMore,
        next_cursor: hasMore ? String(nextCursor) : undefined,
      });
    }),
    http.get('/v1/daemon/works', () =>
      HttpResponse.json({ items: [], pagination: { limit: 20, has_more: false } }),
    ),
    http.get('/v1/daemon/compute/modules', () =>
      HttpResponse.json({ items: [BASIC_COMBAT_MODULE], has_more: false }),
    ),
    http.get('/v1/daemon/compute/modules/basic-combat', () =>
      HttpResponse.json(BASIC_COMBAT_MANIFEST),
    ),
    http.get('/v1/daemon/narrative/worlds', () =>
      HttpResponse.json({
        worlds: [{ world_id: 'world-7', title: 'Test World' }],
      }),
    ),
    http.get('/v1/daemon/compute/runs', () =>
      HttpResponse.json({ items: state.runsStore, has_more: false }),
    ),
    http.post('/v1/daemon/compute/run', async ({ request }) => {
      const body = (await request.json()) as { world_id: string };
      if (body.world_id !== 'world-7') {
        return HttpResponse.json(
          { success: false, error: { code: 'world_not_found', message: 'no' } },
          { status: 404 },
        );
      }
      return HttpResponse.json({
        run_id: 'run_9',
        status: 'succeeded',
        module_id: 'basic-combat',
        module_version: '1.0.0',
        proposals: SUCCESS_PROPOSALS,
        created_at: '2026-08-01T01:00:00Z',
      });
    }),
    http.post('/v1/daemon/compute/runs/:runId/accept', ({ params }) => {
      state.acceptCalls += 1;
      expect(String(params.runId)).toBe('run_9');
      // The daemon appends the compute_result event on Accept.
      state.eventsStore = [COMPUTE_EVENT];
      return HttpResponse.json({
        run_id: 'run_9',
        status: 'applied',
        applied: { state_delta_count: 1, events_created: 1, new_entries_created: 0 },
        timeline_event_ids: ['evt_compute_1'],
      });
    }),
    http.get('/v1/daemon/compute/runs/:runId', () =>
      HttpResponse.json({
        run_id: 'run_9',
        status: 'applied',
        module_id: 'basic-combat',
        module_version: '1.0.0',
        world_id: 'world-7',
        invocation_params: { attacker_id: 'kb-aria', defender_id: 'kb-brann' },
        proposals: SUCCESS_PROPOSALS,
        created_at: '2026-08-01T01:00:00Z',
      }),
    ),
    // F3 (review) — the app-under-test fires a registry scan (Settings shell /
    // agent-host polling); serve it so MSW stops logging unhandled-request
    // noise in the journey test.
    http.post('/v1/daemon/agent-host/scan', () =>
      HttpResponse.json({ agents: [] }),
    ),
  ];

  return { state, handlers };
}

function renderTimelineApp(over: { initialEvents?: unknown[]; kbEntities?: unknown[] } = {}) {
  const { state, handlers } = computeTimelineJourney(over);
  useHandlers(...handlers);
  const result = renderInApp(
    <SettingsModalProvider>
      <ModalAppRoutes />
      <SettingsModalHost />
      <SettingsCloseButton />
    </SettingsModalProvider>,
    { client: new BrowserClient(), initialRouterEntries: ['/worlds/world-7/timeline'] },
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

describe('TimelineCanvas compute-on-Timeline (V1.147 P2 T3)', () => {
  it('renders KB event nodes and Compute result nodes as distinct families (no double-render)', async () => {
    renderTimelineApp({ initialEvents: [COMPUTE_EVENT] });

    // KB author event (rendered in the node card + the NLE band label).
    await screen.findAllByText('The coronation');
    // Compute result node (merged log event family).
    const computeNode = await screen.findByTestId('compute-result-node-chrome');
    expect(within(computeNode).getByText('Aria strikes Brann')).toBeInTheDocument();
    expect(within(computeNode).getByText('Compute result')).toBeInTheDocument();
    expect(within(computeNode).getByText('From module run')).toBeInTheDocument();
    // Module display name resolved from the registry.
    expect(within(computeNode).getByText(/Basic Combat/)).toBeInTheDocument();
  });

  it('does NOT show the empty state when the only Narrative content is a compute event', async () => {
    renderTimelineApp({ initialEvents: [COMPUTE_EVENT], kbEntities: [] });

    await screen.findByTestId('compute-result-node-chrome');
    expect(screen.queryByTestId('timeline-empty-state')).not.toBeInTheDocument();
  });

  it('events query is branch-scoped by the daemon (no branch_id) and hard-filtered to canon compute_result', async () => {
    const { state } = renderTimelineApp({ initialEvents: [COMPUTE_EVENT] });
    await screen.findByTestId('compute-result-node-chrome');

    await waitFor(() => expect(state.lastEventsUrl).not.toBeNull());
    const url = new URL(state.lastEventsUrl!, 'http://localhost');
    expect(url.searchParams.get('event_type')).toBe('compute_result');
    expect(url.searchParams.get('status')).toBe('canon');
    expect(url.searchParams.get('branch_id')).toBeNull();
  });

  it('auto-fetches remaining events pages — nodes from page 2 render (review F1)', async () => {
    // 105 events → page 1 = 100 (limit), page 2 = 5 (has_more: true on
    // page 1). The last event renders only if the canvas wired
    // `fetchNextPage` for its snapshot projection.
    const events = Array.from({ length: 105 }, (_, i) => ({
      ...COMPUTE_EVENT,
      id: `evt_page_${i}`,
      title: `Event #${i + 1}`,
    }));
    const { container, state } = renderTimelineApp({ initialEvents: events });

    // The last page's event renders as a canvas node — RF wraps each node in
    // `.react-flow__node[data-id=…]`. Scope to the canvas node instead of a
    // global unique-text lookup: the same event title also renders in the NLE
    // band clip (the band projects the same compute nodes), so a bare
    // `findByText` would match two elements.
    await waitFor(() =>
      expect(
        container.querySelector('.react-flow__node[data-id="compute:evt_page_104"]'),
      ).not.toBeNull(),
    );
    await waitFor(() => expect(state.eventsFetchCount).toBeGreaterThanOrEqual(2));
  });

  it('shows an honest cap note when the compute-event projection hits the 500 ceiling (V1.147 P3 T2)', async () => {
    // 520 events → pages 1–5 fetch exactly 500; page 5 still reports
    // `has_more`, so the projection is capped and the canvas must say so
    // instead of silently dropping the older events.
    const events = Array.from({ length: 520 }, (_, i) => ({
      ...COMPUTE_EVENT,
      id: `evt_cap_${i}`,
      title: `Cap event #${i + 1}`,
    }));
    renderTimelineApp({ initialEvents: events });

    await waitFor(
      () =>
        expect(screen.getByTestId('timeline-compute-projection-cap-note')).toBeInTheDocument(),
      { timeout: 10_000 },
    );
    expect(screen.getByText(/first 500 compute events/i)).toBeInTheDocument();
  }, 15_000);

  it('Run Module entry opens the shared Run Studio with the World pre-filled', async () => {
    const user = userEvent.setup();
    renderTimelineApp();

    await user.click(await screen.findByTestId('timeline-run-module-entry'));

    // Settings modal opened on the Modules section.
    await screen.findByTestId('settings-modal-body');
    await user.click(await screen.findByRole('button', { name: 'Basic Combat' }));
    await screen.findByTestId('run-studio');

    // World pre-filled from `?world=world-7` (behavior spec §1 P2).
    await waitFor(() =>
      expect(screen.getByTestId('run-studio-world')).toHaveValue('world-7'),
    );
  });

  it('compute inspector (alt-view) shows compute context and Open Run deep-links to the Run detail', async () => {
    const user = userEvent.setup();
    renderTimelineApp({ initialEvents: [COMPUTE_EVENT] });

    // Flip to the sortable list (alt view) — accessible row selection.
    await user.click(await screen.findByRole('button', { name: 'Show list view' }));

    // Select the compute row → compute inspector opens.
    const row = await screen.findByText('Aria strikes Brann');
    fireEvent.click(row.closest('tr')!);
    await screen.findByTestId('timeline-compute-inspector');
    expect(screen.getByTestId('compute-inspector-section-module')).toBeInTheDocument();
    expect(screen.getByText('From module run')).toBeInTheDocument();

    // Open Run → Settings modal on the module detail with the run inspector
    // deep-linked (run detail query resolves from `?run=run_9`).
    await user.click(screen.getByTestId('compute-inspector-open-run'));
    await screen.findByTestId('settings-modal-body');
    await screen.findByTestId('run-studio');
    await screen.findByTestId('run-inspector');
  });

  it('AC-I3 journey: Timeline → Run Studio (World pre-filled) → run → accept → node appears without refresh', async () => {
    const user = userEvent.setup();
    const { state } = renderTimelineApp();

    // 1. Entry: Run Module from the Timeline toolbar.
    await user.click(await screen.findByTestId('timeline-run-module-entry'));
    await screen.findByTestId('settings-modal-body');
    await user.click(await screen.findByRole('button', { name: 'Basic Combat' }));
    await screen.findByTestId('run-studio');

    // 2. World pre-filled (no manual pick).
    await waitFor(() =>
      expect(screen.getByTestId('run-studio-world')).toHaveValue('world-7'),
    );

    // 3. Fill the guided form + run.
    await screen.findByRole('combobox', { name: /^Attacker/ });
    await user.selectOptions(
      screen.getByRole('combobox', { name: /^Attacker/ }),
      'kb-aria',
    );
    await user.selectOptions(
      screen.getByRole('combobox', { name: /^Defender/ }),
      'kb-brann',
    );
    await waitFor(() => expect(screen.getByTestId('run-studio-run')).toBeEnabled());
    await user.click(screen.getByTestId('run-studio-run'));
    await screen.findByTestId('proposal-section-report');

    // 4. Accept — the daemon appends the compute_result event; the canvas
    // (still mounted behind the modal) refetches via the timeline.all
    // invalidation WITHOUT any manual refresh.
    await user.click(screen.getByTestId('run-inspector-accept'));
    await waitFor(() => expect(state.acceptCalls).toBe(1));
    await screen.findByTestId('run-inspector-applied-note');

    // 5. Close Settings → the Timeline is still mounted; the Compute result
    // node is already there. (fireEvent: the Radix dialog overlay sits above
    // the helper button, so userEvent's pointer check refuses it.)
    fireEvent.click(screen.getByTestId('close-settings'));
    const computeNode = await screen.findByTestId('compute-result-node-chrome');
    expect(within(computeNode).getByText('Aria strikes Brann')).toBeInTheDocument();
    expect(within(computeNode).getByText('Compute result')).toBeInTheDocument();
  });
});
