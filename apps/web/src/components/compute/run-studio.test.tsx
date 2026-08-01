/**
 * RunStudio deep-link `?run=` re-sync (V1.147 PR #194 Greptile Issue 1).
 *
 * The Settings Modules section is NOT remounted when a second compute node
 * "Open Run" navigates to the same `/settings/modules?module=…&run=…` route
 * (search-params-only navigation), so `useState(initialRunId ?? null)` alone
 * would leave the previous run's inspector open. These tests pin the re-sync:
 * a new `initialRunId` switches the inspector, removing it closes the
 * inspector, and an unchanged prop never clobbers the fresh-run inspector
 * (the POST-success flow).
 */
import { http, HttpResponse } from 'msw';
import { beforeEach, describe, expect, it } from 'vitest';
import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import type { ModuleDetail } from '@42ch/nexus-contracts';

import { RunStudio } from '@/components/compute/run-studio';
import { i18n } from '@/lib/i18n/config';
import { BrowserClient } from '@/lib/nexus';
import { useHandlers } from '@/test/msw-server';
import { renderInApp } from '@/test/test-providers';

const MODULE: ModuleDetail = {
  module_id: 'basic-combat',
  name: 'Basic Combat',
  version: '1.0.0',
  nexus_abi_version: 1,
  required_key_block_types: ['character'],
  compute_export: 'compute',
  init_export: 'init',
  description: 'A simple combat resolution module.',
  author: 'Nexus Team',
  // No `required` fields → the Run button only needs a World selection.
  schemas: { invocation: { type: 'object', properties: {}, required: [] } },
};

/** Distinct inspector content per deep-linked run id. */
const RUN_TITLES: Record<string, string> = {
  'run-a': 'Run A timeline event',
  'run-b': 'Run B timeline event',
};

function runDetail(runId: string, eventTitle: string) {
  return {
    run_id: runId,
    status: 'succeeded',
    module_id: 'basic-combat',
    module_version: '1.0.0',
    world_id: 'w1',
    invocation_params: { attacker_id: 'kb-aria', defender_id: 'kb-brann' },
    proposals: {
      schema_version: 1,
      state_delta: [],
      timeline_events: [
        { title: eventTitle, summary: 'The event summary.', affected_key_block_ids: ['kb-aria'] },
      ],
      new_key_blocks: [],
      battle_report: { kind: 'combat' },
    },
    created_at: '2026-08-01T01:00:00Z',
  };
}

/** Base handlers: Worlds + Runs history (empty) + per-run detail. */
function deepLinkHandlers(worlds: unknown[] = []) {
  return [
    http.get('/v1/daemon/narrative/worlds', () => HttpResponse.json({ worlds })),
    http.get('/v1/daemon/compute/runs', () =>
      HttpResponse.json({ items: [], has_more: false }),
    ),
    http.get('/v1/daemon/compute/runs/:runId', ({ params }) => {
      const runId = String(params.runId);
      return HttpResponse.json(runDetail(runId, RUN_TITLES[runId] ?? 'Unknown run'));
    }),
  ];
}

function renderStudio(initialRunId?: string) {
  return renderInApp(<RunStudio module={MODULE} initialRunId={initialRunId} />, {
    client: new BrowserClient(),
  });
}

beforeEach(async () => {
  await i18n.changeLanguage('en');
});

describe('RunStudio deep-link `?run=` re-sync (V1.147 PR #194)', () => {
  it('switches the inspector when the deep-linked run changes without remounting', async () => {
    useHandlers(...deepLinkHandlers());

    const view = renderStudio('run-a');

    // Deep link run-a opens its inspector.
    expect(await screen.findByText('Run A timeline event')).toBeInTheDocument();
    expect(screen.getByTestId('run-inspector')).toBeInTheDocument();

    // Same route element, new `?run=` value (compute node B "Open Run") — the
    // inspector must follow the prop, not stay on the stale run.
    view.rerender(<RunStudio module={MODULE} initialRunId="run-b" />);

    expect(await screen.findByText('Run B timeline event')).toBeInTheDocument();
    expect(screen.queryByText('Run A timeline event')).not.toBeInTheDocument();
  });

  it('closes the inspector when the deep-linked run param is removed', async () => {
    useHandlers(...deepLinkHandlers());

    const view = renderStudio('run-a');
    expect(await screen.findByText('Run A timeline event')).toBeInTheDocument();

    // `?run=` removed (module switch deletes the param, or the author clears
    // it) — the open inspector must close.
    view.rerender(<RunStudio module={MODULE} initialRunId={undefined} />);

    await waitFor(() =>
      expect(screen.queryByTestId('run-inspector')).not.toBeInTheDocument(),
    );
    expect(screen.queryByText('Run A timeline event')).not.toBeInTheDocument();
  });

  it('keeps the fresh-run inspector when an unchanged deep-link prop re-renders', async () => {
    useHandlers(
      ...deepLinkHandlers([{ world_id: 'w1', title: 'Test World' }]),
      http.get('/v1/daemon/worlds/w1/kb/graph', () =>
        HttpResponse.json({ entities: [], source_anchors: [], relationships: [] }),
      ),
      http.post('/v1/daemon/compute/run', () =>
        HttpResponse.json({
          run_id: 'run_fresh',
          status: 'succeeded',
          module_id: 'basic-combat',
          module_version: '1.0.0',
          world_id: 'w1',
          proposals: {
            schema_version: 1,
            state_delta: [],
            timeline_events: [{ title: 'Fresh run event', summary: '...' }],
            new_key_blocks: [],
            battle_report: { kind: 'combat' },
          },
          created_at: '2026-08-01T02:00:00Z',
        }),
      ),
    );
    const user = userEvent.setup();

    const view = renderStudio('run-a');
    expect(await screen.findByText('Run A timeline event')).toBeInTheDocument();

    // Fresh run after the deep link: the POST-success inspector takes over.
    await waitFor(() => expect(screen.getByTestId('run-studio-world')).toBeEnabled());
    await user.selectOptions(screen.getByTestId('run-studio-world'), 'w1');
    await waitFor(() => expect(screen.getByTestId('run-studio-run')).toBeEnabled());
    await user.click(screen.getByTestId('run-studio-run'));
    expect(await screen.findByText('Fresh run event')).toBeInTheDocument();
    expect(screen.queryByText('Run A timeline event')).not.toBeInTheDocument();

    // The parent re-renders with the SAME `?run=run-a` value (runs-list
    // refresh etc.) — the sync effect must not re-open the stale deep-linked
    // run over the fresh-run inspector.
    view.rerender(<RunStudio module={MODULE} initialRunId="run-a" />);
    expect(screen.getByText('Fresh run event')).toBeInTheDocument();
    expect(screen.queryByText('Run A timeline event')).not.toBeInTheDocument();
  });
});
