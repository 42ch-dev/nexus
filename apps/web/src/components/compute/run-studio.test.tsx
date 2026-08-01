/**
 * RunStudio deep-link re-sync (V1.147 PR #194 Greptile Issues 1 + 2).
 *
 * The Settings Modules section is NOT remounted when a second compute node
 * "Open Run" navigates to the same `/settings/modules?module=…&run=…` route
 * (search-params-only navigation), so `useState(initialRunId ?? null)` alone
 * would leave the previous run's inspector open. These tests pin the run
 * re-sync: a new `initialRunId` switches the inspector, removing it closes
 * the inspector, and an unchanged prop never clobbers the fresh-run inspector
 * (the POST-success flow).
 *
 * Issue 2 pins the World re-sync: a second Timeline "Run Module" entry (or
 * back/forward) with a different `?world=` must switch the World selector
 * while the studio stays mounted; an unchanged prop must never clobber a
 * World the author picked manually.
 *
 * Round 3 (Issue 3) pins invocation-state parity: a deep-link World switch
 * must reset the guided values + Advanced JSON exactly like the manual World
 * switch (a Run would otherwise submit the previous World's params against
 * the new World), while an unchanged prop must not clear the author's input.
 *
 * Round 4 (Issue 4) pins the same parity for a module switch: user-driven
 * module clicks and `?module=` deep-link switches (same mounted instance,
 * cached module detail) must reset the guided values + Advanced JSON and
 * close the previous module's run inspector, while the World selection — a
 * shared axis — survives.
 */
import { http, HttpResponse } from 'msw';
import { beforeEach, describe, expect, it } from 'vitest';
import { fireEvent, screen, waitFor } from '@testing-library/react';
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

/** Module with one guided string field — lets tests populate invocation
 * values and pin the form-reset parity (PR #194 round 3). */
const MODULE_WITH_FIELD: ModuleDetail = {
  ...MODULE,
  schemas: {
    invocation: {
      type: 'object',
      properties: { difficulty: { type: 'string' } },
      required: ['difficulty'],
    },
  },
};

/** Second module id for the module-switch reset tests (PR #194 round 4) —
 * same schema so the guided field stays mounted and the reset is asserted
 * on the cleared value, not on field removal. */
const MODULE_OTHER: ModuleDetail = {
  ...MODULE_WITH_FIELD,
  module_id: 'skill-check',
  name: 'Skill Check',
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

function renderStudio(initialRunId?: string, initialWorldId?: string, module: ModuleDetail = MODULE) {
  return renderInApp(
    <RunStudio
      module={module}
      initialRunId={initialRunId}
      initialWorldId={initialWorldId}
    />,
    { client: new BrowserClient() },
  );
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

describe('RunStudio deep-link `?world=` re-sync (V1.147 PR #194)', () => {
  /** Worlds list + Runs history + per-run detail + World KB graph. */
  function worldSyncHandlers(worlds: unknown[]) {
    return [
      ...deepLinkHandlers(worlds),
      http.get('/v1/daemon/worlds/:worldId/kb/graph', () =>
        HttpResponse.json({ entities: [], source_anchors: [], relationships: [] }),
      ),
    ];
  }

  it('switches the World selector when the deep-linked world changes without remounting', async () => {
    useHandlers(
      ...worldSyncHandlers([
        { world_id: 'w1', title: 'World One' },
        { world_id: 'w2', title: 'World Two' },
      ]),
    );

    const view = renderStudio('run-a', 'w1');

    // Deep link `?world=w1` pre-fills the selector.
    await waitFor(() => expect(screen.getByTestId('run-studio-world')).toHaveValue('w1'));

    // Same route element, new `?world=` value (second Timeline Run Module
    // entry) — the selector must follow the prop, not stay on the stale
    // World (a Run would otherwise execute against the wrong World).
    view.rerender(<RunStudio module={MODULE} initialRunId="run-a" initialWorldId="w2" />);

    await waitFor(() => expect(screen.getByTestId('run-studio-world')).toHaveValue('w2'));
  });

  it('does not clobber a manually picked World when an unchanged deep-link prop re-renders', async () => {
    const user = userEvent.setup();
    useHandlers(
      ...worldSyncHandlers([
        { world_id: 'w1', title: 'World One' },
        { world_id: 'w2', title: 'World Two' },
      ]),
    );

    const view = renderStudio(undefined, 'w1');
    await waitFor(() => expect(screen.getByTestId('run-studio-world')).toHaveValue('w1'));

    // The author picks a different World manually.
    await user.selectOptions(screen.getByTestId('run-studio-world'), 'w2');
    await waitFor(() => expect(screen.getByTestId('run-studio-world')).toHaveValue('w2'));

    // The parent re-renders with the SAME `?world=w1` value (runs-list
    // refresh etc.) — the sync effect must not force the selector back.
    view.rerender(<RunStudio module={MODULE} initialRunId={undefined} initialWorldId="w1" />);
    expect(screen.getByTestId('run-studio-world')).toHaveValue('w2');
  });

  it('resets the World selector when the deep-linked world param is removed', async () => {
    useHandlers(...worldSyncHandlers([{ world_id: 'w1', title: 'World One' }]));

    const view = renderStudio('run-a', 'w1');
    await waitFor(() => expect(screen.getByTestId('run-studio-world')).toHaveValue('w1'));

    // `?world=` removed (param deleted, browser back) — the selector resets
    // to the empty placeholder, mirroring the round-1 `?run=` removal close.
    view.rerender(<RunStudio module={MODULE} initialRunId="run-a" initialWorldId={undefined} />);

    await waitFor(() => expect(screen.getByTestId('run-studio-world')).toHaveValue(''));
  });

  it('clears invocation form state when the deep-linked world changes (round-3 parity with the manual switch)', async () => {
    const user = userEvent.setup();
    useHandlers(
      ...worldSyncHandlers([
        { world_id: 'w1', title: 'World One' },
        { world_id: 'w2', title: 'World Two' },
      ]),
    );

    const view = renderStudio(undefined, 'w1', MODULE_WITH_FIELD);
    await waitFor(() => expect(screen.getByTestId('run-studio-world')).toHaveValue('w1'));

    // Populate the guided form AND the Advanced JSON escape hatch.
    const field = await screen.findByTestId('run-form-field-difficulty');
    await user.type(field, 'hard');
    expect(field).toHaveValue('hard');

    await user.click(screen.getByText('Advanced: edit invocation JSON'));
    const jsonArea = screen.getByTestId('run-studio-json-textarea');
    fireEvent.change(jsonArea, { target: { value: '{"difficulty":"hard"}' } });
    expect(jsonArea).toHaveValue('{"difficulty":"hard"}');
    expect(screen.getByTestId('run-studio-run')).toBeEnabled();

    // Same route element, new `?world=` value — the invocation state must
    // reset exactly like the manual World switch (the old World's params must
    // never be submitted against the new World).
    view.rerender(<RunStudio module={MODULE_WITH_FIELD} initialWorldId="w2" />);

    await waitFor(() => expect(screen.getByTestId('run-studio-world')).toHaveValue('w2'));
    const clearedField = await screen.findByTestId('run-form-field-difficulty');
    expect(clearedField).toHaveValue('');
    expect(screen.getByTestId('run-studio-json-textarea')).toHaveValue('{}');
    // `jsonDirty` is reset too: enablement falls back to the guided required
    // gate, which is unsatisfied after the reset.
    expect(screen.getByTestId('run-studio-run')).toBeDisabled();
  });

  it('does not clobber populated invocation values when an unchanged deep-link prop re-renders (round-3 parity)', async () => {
    const user = userEvent.setup();
    useHandlers(
      ...worldSyncHandlers([
        { world_id: 'w1', title: 'World One' },
        { world_id: 'w2', title: 'World Two' },
      ]),
    );

    const view = renderStudio(undefined, 'w1', MODULE_WITH_FIELD);
    await waitFor(() => expect(screen.getByTestId('run-studio-world')).toHaveValue('w1'));

    // Manual World switch, then populate the guided form for the new World.
    await user.selectOptions(screen.getByTestId('run-studio-world'), 'w2');
    const field = await screen.findByTestId('run-form-field-difficulty');
    await user.type(field, 'hard');
    expect(field).toHaveValue('hard');

    // The parent re-renders with the SAME `?world=w1` value (runs-list
    // refresh etc.) — the sync effect must neither force the selector back
    // nor clear the author's values.
    view.rerender(<RunStudio module={MODULE_WITH_FIELD} initialWorldId="w1" />);
    expect(screen.getByTestId('run-studio-world')).toHaveValue('w2');
    expect(screen.getByTestId('run-form-field-difficulty')).toHaveValue('hard');
  });
});

describe('RunStudio module-switch re-sync (V1.147 PR #194 round 4)', () => {
  /** Worlds list + Runs history + per-run detail + World KB graph. */
  function moduleSwitchHandlers(worlds: unknown[]) {
    return [
      ...deepLinkHandlers(worlds),
      http.get('/v1/daemon/worlds/:worldId/kb/graph', () =>
        HttpResponse.json({ entities: [], source_anchors: [], relationships: [] }),
      ),
    ];
  }

  it('clears guided invocation values on a module switch (user-driven click, Run re-gated)', async () => {
    const user = userEvent.setup();
    useHandlers(...moduleSwitchHandlers([{ world_id: 'w1', title: 'World One' }]));

    const view = renderStudio(undefined, 'w1', MODULE_WITH_FIELD);
    await waitFor(() => expect(screen.getByTestId('run-studio-world')).toHaveValue('w1'));

    // Populate the guided form for the first module.
    const field = await screen.findByTestId('run-form-field-difficulty');
    await user.type(field, 'hard');
    expect(field).toHaveValue('hard');
    expect(screen.getByTestId('run-studio-run')).toBeEnabled();

    // The author clicks another module (same route element; the new module's
    // detail is cached so the studio stays mounted) — the previous module's
    // params must not be submittable against the new module.
    view.rerender(<RunStudio module={MODULE_OTHER} initialWorldId="w1" />);

    await waitFor(() =>
      expect(screen.getByTestId('run-form-field-difficulty')).toHaveValue(''),
    );
    expect(screen.getByTestId('run-studio-run')).toBeDisabled();
    // The World selection is a shared axis — the module switch must not reset it.
    expect(screen.getByTestId('run-studio-world')).toHaveValue('w1');
  });

  it('resets populated Advanced JSON on a deep-link module switch (same mount)', async () => {
    const user = userEvent.setup();
    useHandlers(...moduleSwitchHandlers([{ world_id: 'w1', title: 'World One' }]));

    const view = renderStudio(undefined, 'w1', MODULE_WITH_FIELD);
    await waitFor(() => expect(screen.getByTestId('run-studio-world')).toHaveValue('w1'));

    // Populate the Advanced JSON escape hatch (enablement comes from the
    // valid JSON object, not the guided `required` gate).
    await user.click(screen.getByText('Advanced: edit invocation JSON'));
    const jsonArea = screen.getByTestId('run-studio-json-textarea');
    fireEvent.change(jsonArea, { target: { value: '{"difficulty":"hard"}' } });
    expect(jsonArea).toHaveValue('{"difficulty":"hard"}');
    expect(screen.getByTestId('run-studio-run')).toBeEnabled();

    // `?module=` deep-link switch (second Timeline "Run Module" entry on
    // another module — search-params-only navigation, studio stays mounted).
    view.rerender(<RunStudio module={MODULE_OTHER} initialWorldId="w1" />);

    await waitFor(() =>
      expect(screen.getByTestId('run-studio-json-textarea')).toHaveValue('{}'),
    );
    // `jsonDirty` is reset: enablement falls back to the guided gate, which
    // is unsatisfied after the reset.
    expect(screen.getByTestId('run-studio-run')).toBeDisabled();
  });

  it('closes the previous module fresh-run inspector on a module switch', async () => {
    const user = userEvent.setup();
    useHandlers(
      ...moduleSwitchHandlers([{ world_id: 'w1', title: 'World One' }]),
      http.post('/v1/daemon/compute/run', () =>
        HttpResponse.json({
          run_id: 'run_fresh_2',
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
          created_at: '2026-08-01T03:00:00Z',
        }),
      ),
    );

    const view = renderStudio(undefined, 'w1', MODULE);
    await waitFor(() => expect(screen.getByTestId('run-studio-world')).toBeEnabled());

    // Fresh run with NO `?run=` in the URL: the inspector renders from
    // `latestRun`, which the run re-sync effect never sees.
    await user.click(screen.getByTestId('run-studio-run'));
    expect(await screen.findByText('Fresh run event')).toBeInTheDocument();
    expect(screen.getByTestId('run-inspector')).toBeInTheDocument();

    // Module switch (user click): the stale fresh-run inspector from the
    // previous module must close even though `initialRunId` never changed.
    view.rerender(<RunStudio module={MODULE_OTHER} initialWorldId="w1" />);

    await waitFor(() =>
      expect(screen.queryByTestId('run-inspector')).not.toBeInTheDocument(),
    );
    expect(screen.queryByText('Fresh run event')).not.toBeInTheDocument();
  });
});
