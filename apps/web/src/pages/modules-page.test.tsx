/**
 * ModulesPage render tests.
 *
 * Verifies list + detail read-only UX: the page lists installed compute modules
 * and renders a manifest detail panel when the author selects one.
 *
 * V1.147 P1 T3 adds the Run Studio journey (MSW-driven): run → proposal
 * inspector → Accept / Discard → Runs history refresh; failed runs; §6 empty
 * states; en + zh-CN copy.
 */
import { http, HttpResponse } from 'msw';
import { beforeEach, describe, expect, it } from 'vitest';
import { act, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import type { RunSummary } from '@42ch/nexus-contracts';

import { renderInApp } from '@/test/test-providers';
import { useHandlers } from '@/test/msw-server';
import { BrowserClient } from '@/lib/nexus';
import { i18n } from '@/lib/i18n/config';
import { ModulesPageBody } from '@/pages/modules-page';

const client = () => new BrowserClient();

function renderModules() {
  return renderInApp(<ModulesPageBody />, { client: client() });
}

beforeEach(async () => {
  await i18n.changeLanguage('en');
});

describe('ModulesPage', () => {
  it('renders the modules list and the basic-combat module', async () => {
    useHandlers(
      http.get('/v1/daemon/compute/modules', () =>
        HttpResponse.json({
          items: [
            {
              module_id: 'basic-combat',
              name: 'Basic Combat',
              version: '1.0.0',
              description: 'A simple combat resolution module.',
              required_key_block_types: ['unit', 'terrain'],
              battle_report_kind: 'combat-log',
            },
          ],
          has_more: false,
        }),
      ),
    );

    renderModules();

    expect(await screen.findByText('Basic Combat')).toBeInTheDocument();
    expect(screen.getByText('1.0.0')).toBeInTheDocument();
    expect(screen.getByText('A simple combat resolution module.')).toBeInTheDocument();
    expect(screen.getByText('unit')).toBeInTheDocument();
    expect(screen.getByText('terrain')).toBeInTheDocument();
    expect(screen.getByText('combat-log')).toBeInTheDocument();
  });

  it('renders the detail panel when a module is selected', async () => {
    const user = userEvent.setup();

    useHandlers(
      http.get('/v1/daemon/compute/modules', () =>
        HttpResponse.json({
          items: [
            {
              module_id: 'basic-combat',
              name: 'Basic Combat',
              version: '1.0.0',
              description: 'A simple combat resolution module.',
              required_key_block_types: ['unit'],
            },
          ],
          has_more: false,
        }),
      ),
      http.get('/v1/daemon/compute/modules/basic-combat', () =>
        HttpResponse.json({
          module_id: 'basic-combat',
          name: 'Basic Combat',
          version: '1.0.0',
          nexus_abi_version: 1,
          required_key_block_types: ['unit'],
          compute_export: 'compute',
          init_export: 'init',
          description: 'A simple combat resolution module.',
          author: 'Nexus Team',
          host_functions: ['kb_read'],
          battle_report_kind: 'combat-log',
          max_fuel: 1_000_000,
          max_memory_mib: 128,
          max_wall_time_ms: 5000,
        }),
      ),
      // The detail panel hosts the Run Studio (V1.147 P1): its Worlds list and
      // Runs history queries fire on mount and must be handled.
      http.get('/v1/daemon/narrative/worlds', () =>
        HttpResponse.json({ worlds: [] }),
      ),
      http.get('/v1/daemon/compute/runs', () =>
        HttpResponse.json({ items: [], has_more: false }),
      ),
    );

    renderModules();

    await screen.findByText('Basic Combat');
    await user.click(screen.getByRole('button', { name: 'Basic Combat' }));

    await waitFor(() => {
      expect(screen.getByText('Module manifest')).toBeInTheDocument();
    });

    expect(screen.getByText('basic-combat')).toBeInTheDocument();
    expect(screen.getByText('Nexus Team')).toBeInTheDocument();
    expect(screen.getByText('compute')).toBeInTheDocument();
    expect(screen.getByText('init')).toBeInTheDocument();
    expect(screen.getByText('kb_read')).toBeInTheDocument();
    expect(screen.getByText('1000000')).toBeInTheDocument();
    expect(screen.getByText('128')).toBeInTheDocument();
    expect(screen.getByText('5000')).toBeInTheDocument();
  });

  it('renders the empty state when no modules are installed', async () => {
    useHandlers(
      http.get('/v1/daemon/compute/modules', () =>
        HttpResponse.json({ items: [], has_more: false }),
      ),
    );

    renderModules();

    expect(await screen.findByText('No modules installed')).toBeInTheDocument();
  });

  it('renders the error state when the daemon fails', async () => {
    useHandlers(
      http.get('/v1/daemon/compute/modules', () =>
        HttpResponse.json(
          { success: false, error: { code: 'internal', message: 'boom' } },
          { status: 500 },
        ),
      ),
    );

    renderModules();

    expect(await screen.findByText('Could not load modules')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Try again' })).toBeInTheDocument();
    expect(screen.queryByText('Could not load this view')).not.toBeInTheDocument();
  });

  it('renders unavailable state when orchestration engine is down (503)', async () => {
    useHandlers(
      http.get('/v1/daemon/compute/modules', () =>
        HttpResponse.json(
          {
            success: false,
            error: { code: 'service_unavailable', message: 'engine not available' },
          },
          { status: 503 },
        ),
      ),
    );

    renderModules();

    expect(await screen.findByText('Orchestration engine not running')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Try again' })).toBeInTheDocument();
    expect(screen.queryByText('Could not load this view')).not.toBeInTheDocument();
  });

  it('renders the loading state before data resolves', async () => {
    useHandlers(
      http.get('/v1/daemon/compute/modules', async () => {
        await new Promise((resolve) => { setTimeout(resolve, 50); });
        return HttpResponse.json({ items: [], has_more: false });
      }),
    );

    renderModules();

    expect(await screen.findByText('Loading modules…')).toBeInTheDocument();
  });

  it('switches to zh-CN locale without remounting', async () => {
    useHandlers(
      http.get('/v1/daemon/compute/modules', () =>
        HttpResponse.json({
          items: [
            {
              module_id: 'basic-combat',
              name: 'Basic Combat',
              version: '1.0.0',
              required_key_block_types: ['unit'],
            },
          ],
          has_more: false,
        }),
      ),
    );

    renderModules();
    expect(await screen.findByRole('heading', { name: 'Compute Modules' })).toBeInTheDocument();

    act(() => {
      i18n.changeLanguage('zh-CN');
    });

    expect(await screen.findByRole('heading', { name: '计算模块' })).toBeInTheDocument();
  });
});

/* ── Run Studio journey (V1.147 P1 T3) ───────────────────────────────────── */

const WORLD = {
  schema_version: 1,
  world_id: 'w1',
  owner_creator_id: 'creator-1',
  title: 'The Lost City',
  slug: 'the-lost-city',
  status: 'active',
  visibility: 'private',
  time_policy: 'manual',
};

/** basic-combat manifest with an invocation schema (attacker/defender pickers). */
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

const CHARACTER_ENTITIES = [
  {
    key_block_id: 'kb-aria',
    world_id: 'w1',
    block_type: 'character',
    canonical_name: 'Aria',
    status: 'confirmed',
    version: 1,
  },
  {
    key_block_id: 'kb-brann',
    world_id: 'w1',
    block_type: 'character',
    canonical_name: 'Brann',
    status: 'confirmed',
    version: 1,
  },
];

const SUCCESS_PROPOSALS = {
  schema_version: 1,
  state_delta: [
    { op: 'sub', path: 'character.current_hp', target_key_block_id: 'kb-brann', value: { amount: 6 } },
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

const RUN_9: RunSummary = {
  run_id: 'run_9',
  status: 'succeeded',
  module_id: 'basic-combat',
  module_version: '1.0.0',
  world_id: 'w1',
  created_at: '2026-07-31T01:00:00Z',
};

/**
 * Full MSW handler set for the module-detail Run Studio journey. Keeps a
 * mutable runs store so Accept/Discard flips the row the table shows after the
 * hooks' invalidation refetches.
 */
function runStudioJourney(
  over: { failedRun?: boolean; noSchema?: boolean; kbEntities?: typeof CHARACTER_ENTITIES } = {},
) {
  const state = {
    runsStore: [] as RunSummary[],
    lastRunBody: null as unknown,
    acceptCalls: 0,
    discardCalls: 0,
  };

  const handlers = [
    http.get('/v1/daemon/compute/modules', () =>
      HttpResponse.json({
        items: [
          {
            module_id: 'basic-combat',
            name: 'Basic Combat',
            version: '1.0.0',
            description: 'A simple combat resolution module.',
            required_key_block_types: ['character'],
          },
        ],
        has_more: false,
      }),
    ),
    http.get('/v1/daemon/compute/modules/basic-combat', () =>
      HttpResponse.json(
        over.noSchema ? { ...BASIC_COMBAT_MANIFEST, schemas: undefined } : BASIC_COMBAT_MANIFEST,
      ),
    ),
    http.get('/v1/daemon/narrative/worlds', () => HttpResponse.json({ worlds: [WORLD] })),
    http.get('/v1/daemon/worlds/:worldId/kb/graph', () =>
      HttpResponse.json({
        entities: over.kbEntities ?? CHARACTER_ENTITIES,
        source_anchors: [],
        relationships: [],
      }),
    ),
    http.get('/v1/daemon/compute/runs', () =>
      HttpResponse.json({ items: state.runsStore, has_more: false }),
    ),
    http.post('/v1/daemon/compute/run', async ({ request }) => {
      state.lastRunBody = await request.json();
      if (over.failedRun) {
        state.runsStore = [...state.runsStore, { ...RUN_9, run_id: 'run_fail', status: 'failed' }];
        return HttpResponse.json({
          run_id: 'run_fail',
          status: 'failed',
          module_id: 'basic-combat',
          module_version: '1.0.0',
          error: { code: 'compute_wall_time_exceeded', message: 'module exceeded wall time' },
          created_at: '2026-07-31T01:00:00Z',
        });
      }
      state.runsStore = [...state.runsStore, RUN_9];
      return HttpResponse.json({
        run_id: RUN_9.run_id,
        status: 'succeeded',
        module_id: 'basic-combat',
        module_version: '1.0.0',
        proposals: SUCCESS_PROPOSALS,
        created_at: RUN_9.created_at,
      });
    }),
    http.post('/v1/daemon/compute/runs/:runId/accept', ({ params }) => {
      const runId = String(params.runId);
      state.acceptCalls += 1;
      state.runsStore = state.runsStore.map((r) =>
        r.run_id === runId ? { ...r, status: 'applied', updated_at: '2026-07-31T02:00:00Z' } : r,
      );
      return HttpResponse.json({
        run_id: runId,
        status: 'applied',
        applied: { state_delta_count: 1, events_created: 1, new_entries_created: 0 },
        timeline_event_ids: ['evt_0'],
      });
    }),
    http.post('/v1/daemon/compute/runs/:runId/discard', ({ params }) => {
      const runId = String(params.runId);
      state.discardCalls += 1;
      state.runsStore = state.runsStore.map((r) =>
        r.run_id === runId ? { ...r, status: 'discarded', updated_at: '2026-07-31T02:00:00Z' } : r,
      );
      return HttpResponse.json({ run_id: runId, status: 'discarded' });
    }),
    http.get('/v1/daemon/compute/runs/:runId', ({ params }) => {
      const runId = String(params.runId);
      const stored = state.runsStore.find((r) => r.run_id === runId);
      return HttpResponse.json({
        ...(stored ?? RUN_9),
        proposals: SUCCESS_PROPOSALS,
      });
    }),
  ];

  return { state, handlers };
}

/** Open the module detail and land on the Run Studio. */
async function openRunStudio(user: ReturnType<typeof userEvent.setup>) {
  renderModules();
  await screen.findByText('Basic Combat');
  await user.click(screen.getByRole('button', { name: 'Basic Combat' }));
  await screen.findByText('Run Studio');
}

/** Fill the guided form: pick a World, then both character pickers. */
async function fillBasicCombatForm(user: ReturnType<typeof userEvent.setup>) {
  await user.selectOptions(screen.getByTestId('run-studio-world'), 'w1');
  // The required-mark span (*) joins the label text, so match by regex.
  await screen.findByRole('combobox', { name: /^Attacker/ });
  await user.selectOptions(screen.getByRole('combobox', { name: /^Attacker/ }), 'kb-aria');
  await user.selectOptions(screen.getByRole('combobox', { name: /^Defender/ }), 'kb-brann');
}

describe('ModulesPage Run Studio (V1.147 P1 T3)', () => {
  it('runs a module, reviews proposals, accepts, and refreshes Runs history', async () => {
    const user = userEvent.setup();
    const { state, handlers } = runStudioJourney();
    useHandlers(...handlers);

    await openRunStudio(user);
    await fillBasicCombatForm(user);

    // Run is gated until the required pickers are filled (manifest `required`).
    await waitFor(() => expect(screen.getByTestId('run-studio-run')).toBeEnabled());
    await user.click(screen.getByTestId('run-studio-run'));

    // Inspector renders the four-part proposals from the POST response.
    await screen.findByTestId('proposal-section-report');
    expect(screen.getByText('Aria strikes Brann')).toBeInTheDocument();
    expect(state.lastRunBody).toMatchObject({
      world_id: 'w1',
      module_id: 'basic-combat',
      invocation_params: { attacker_id: 'kb-aria', defender_id: 'kb-brann' },
    });

    // Accept → toast + inspector flips to Applied via the refetched run detail.
    await user.click(screen.getByTestId('run-inspector-accept'));
    expect(await screen.findByText('Run applied')).toBeInTheDocument();
    await waitFor(() => expect(state.acceptCalls).toBe(1));
    await screen.findByTestId('run-inspector-applied-note');
    expect(screen.queryByTestId('run-inspector-accept')).not.toBeInTheDocument();

    // Runs history refreshed: the row now shows Applied.
    await waitFor(() =>
      expect(
        within(screen.getByTestId('runs-table-row-run_9')).getByText('Applied'),
      ).toBeInTheDocument(),
    );
  });

  it('discards a run only after confirmation and refreshes Runs history', async () => {
    const user = userEvent.setup();
    const { state, handlers } = runStudioJourney();
    useHandlers(...handlers);

    await openRunStudio(user);
    await fillBasicCombatForm(user);
    await user.click(screen.getByTestId('run-studio-run'));
    await screen.findByTestId('proposal-section-report');

    // Cancelling the confirm dialog keeps the run intact.
    await user.click(screen.getByTestId('run-inspector-discard'));
    await screen.findByRole('heading', { name: 'Discard this Run?' });
    await user.click(screen.getByRole('button', { name: 'Cancel' }));
    expect(state.discardCalls).toBe(0);
    expect(screen.getByTestId('run-inspector-accept')).toBeInTheDocument();

    // Confirmed discard → toast + inspector flips to Discarded + row flips.
    await user.click(screen.getByTestId('run-inspector-discard'));
    await screen.findByRole('heading', { name: 'Discard this Run?' });
    await user.click(screen.getByRole('button', { name: 'Discard Run' }));
    expect(await screen.findByText('Run discarded')).toBeInTheDocument();
    await waitFor(() => expect(state.discardCalls).toBe(1));
    await screen.findByTestId('run-inspector-discarded-note');
    await waitFor(() =>
      expect(
        within(screen.getByTestId('runs-table-row-run_9')).getByText('Discarded'),
      ).toBeInTheDocument(),
    );
  });

  it('shows the failure block for a failed run and offers no Accept/Discard', async () => {
    const user = userEvent.setup();
    const { handlers } = runStudioJourney({ failedRun: true });
    useHandlers(...handlers);

    await openRunStudio(user);
    await fillBasicCombatForm(user);
    await user.click(screen.getByTestId('run-studio-run'));

    // §6 limit copy + honest error code; World unchanged (no accept/discard CTAs).
    await screen.findByText('Run stopped (limit)');
    expect(screen.getByText(/compute_wall_time_exceeded/)).toBeInTheDocument();
    expect(screen.queryByTestId('run-inspector-accept')).not.toBeInTheDocument();
    expect(screen.queryByTestId('run-inspector-discard')).not.toBeInTheDocument();

    // The failed run stays in Runs history with the Failed label.
    await waitFor(() =>
      expect(
        within(screen.getByTestId('runs-table-row-run_fail')).getByText('Failed'),
      ).toBeInTheDocument(),
    );
  });

  it('shows the §6 empty state for an empty Runs history', async () => {
    const user = userEvent.setup();
    const { handlers } = runStudioJourney();
    useHandlers(...handlers);

    await openRunStudio(user);

    expect(await screen.findByText('No runs yet')).toBeInTheDocument();
  });

  it('shows the picker empty state when the World has no matching entries', async () => {
    const user = userEvent.setup();
    const { handlers } = runStudioJourney({ kbEntities: [] });
    useHandlers(...handlers);

    await openRunStudio(user);
    await user.selectOptions(screen.getByTestId('run-studio-world'), 'w1');

    // §6: no characters to run — the picker renders its caller-owned empty state.
    expect(await screen.findAllByText('No characters to run')).toHaveLength(2);
    expect(screen.queryByRole('combobox', { name: /^Attacker/ })).not.toBeInTheDocument();
  });

  it('shows the unusable-manifest empty state when the module has no invocation schema', async () => {
    const user = userEvent.setup();
    const { handlers } = runStudioJourney({ noSchema: true });
    useHandlers(...handlers);

    await openRunStudio(user);

    // Catalog uses the typographic apostrophe (U+2019) — match the source copy.
    expect(await screen.findByText('Can’t run this module')).toBeInTheDocument();
  });

  it('renders Run Studio copy in zh-CN', async () => {
    const user = userEvent.setup();
    const { handlers } = runStudioJourney();
    useHandlers(...handlers);

    await openRunStudio(user);

    act(() => {
      i18n.changeLanguage('zh-CN');
    });

    expect(await screen.findByText('运行工作室')).toBeInTheDocument();
    expect(screen.getByText('运行记录')).toBeInTheDocument();
    expect(screen.getByText('暂无运行')).toBeInTheDocument();
    // World label appears on the selector and the Runs table column header.
    expect((await screen.findAllByText('世界')).length).toBeGreaterThan(0);
  });
});
