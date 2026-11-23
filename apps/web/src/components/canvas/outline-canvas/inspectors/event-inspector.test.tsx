/**
 * Verifies the Link Foreshadow / Unlink Foreshadow controls fire the correct
 * `timeline.patch_event` operations with the locked copy
 * ("Link Foreshadow" / "Unlink Foreshadow"), and that the bind/unbind control
 * drives `bind_world_event` / `unbind_world_event` through the same mutation
 * boundary — disabled, never hidden, when the Work has no bound World.
 */
import { describe, expect, it, vi } from 'vitest';
import { act, render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useState } from 'react';

import { TimelinePanel } from '@/components/canvas/outline-canvas/inspectors/event-inspector';

import type { TimelinePatchEventRequest, WorkOutline } from '@42ch/nexus-contracts';

const worldKbMock = vi.hoisted(() => {
  const graph = {
    entities: [
      {
        key_block_id: 'kb-evt-1',
        world_id: 'world-9',
        block_type: 'event',
        canonical_name: 'Coronation',
        status: 'confirmed',
        version: 1,
      },
      {
        key_block_id: 'kb-chr-1',
        world_id: 'world-9',
        block_type: 'character',
        canonical_name: 'Queen',
        status: 'confirmed',
        version: 1,
      },
    ],
    source_anchors: [],
    relationships: [],
  };
  const secondWorldGraph = {
    entities: [
      {
        key_block_id: 'kb-evt-7',
        world_id: 'world-11',
        block_type: 'event',
        canonical_name: 'Solar Eclipse',
        status: 'confirmed',
        version: 1,
      },
    ],
    source_anchors: [],
    relationships: [],
  };
  return {
    graph,
    secondWorldGraph,
    graphs: { 'world-9': graph, 'world-11': secondWorldGraph } as Record<string, unknown>,
  };
});

vi.mock('@/lib/canvas/use-world-kb-data', () => ({
  useWorldKbGraph: (worldId?: string) => ({
    data: worldId ? worldKbMock.graphs[worldId] : undefined,
    isLoading: false,
    isError: false,
  }),
}));

function makeOutline(overrides: Partial<WorkOutline> = {}): WorkOutline {
  return {
    work_id: 'wk_test',
    outline_revision: 3,
    volumes: [],
    timeline_events: [
      {
        event_id: 'evt_a',
        title: 'Plant the seed',
        description: 'The protagonist finds a mysterious key.',
        realizes_chapter_id: 1,
      },
      {
        event_id: 'evt_b',
        title: 'The payoff',
        description: 'The key opens the vault.',
        realizes_chapter_id: 2,
      },
    ],
    foreshadows: [],
    scenes: [],
    beats: [],
    chapter_titles: {},
    updated_at: '',
    ...overrides,
  };
}

function renderTimeline(
  outline: WorkOutline,
  onPatchTimeline = vi.fn(),
  selectedChapterId: number | null = null,
  boundWorldId?: string,
) {
  render(
    <TimelinePanel
      outline={outline}
      selectedChapterId={selectedChapterId}
      baseRevision={outline.outline_revision}
      onPatchTimeline={onPatchTimeline}
      boundWorldId={boundWorldId}
    />,
  );
  return onPatchTimeline;
}

function applyCanonicalBinding(
  outline: WorkOutline,
  request: TimelinePatchEventRequest,
): WorkOutline {
  if (request.operation === 'bind_world_event') {
    return {
      ...outline,
      timeline_events: outline.timeline_events.map((event) =>
        event.event_id === request.event_id
          ? { ...event, world_event_id: request.world_event_id }
          : event,
      ),
    };
  }
  if (request.operation === 'unbind_world_event') {
    return {
      ...outline,
      timeline_events: outline.timeline_events.map((event) =>
        event.event_id === request.event_id ? { ...event, world_event_id: undefined } : event,
      ),
    };
  }
  return outline;
}

/**
 *
 * `usePatchTimelineEvent`'s success invalidates the outline read
 * (`use-outline-data.ts`), so the Outline canvas hands the inspector the
 * refetched outline — not the emitted request. This harness mirrors that
 * handoff: each emitted request is applied to the canonical outline, whose
 * `world_event_id` carrier then drives the asserted UI. That is what makes the
 * post-bind / post-unbind assertions a statement about the refetched state
 * rather than the mutation callback payload.
 */
function renderTimelineRefetching(initialOutline: WorkOutline, boundWorldId?: string) {
  const onPatch = vi.fn();
  function RefetchingTimeline() {
    const [outline, setOutline] = useState(initialOutline);
    return (
      <TimelinePanel
        outline={outline}
        selectedChapterId={null}
        baseRevision={outline.outline_revision}
        onPatchTimeline={(request) => {
          onPatch(request);
          setOutline((prev) => applyCanonicalBinding(prev, request));
        }}
        boundWorldId={boundWorldId}
      />
    );
  }
  render(<RefetchingTimeline />);
  return onPatch;
}

function renderTimelineWithBoundWorld(
  initialOutline: WorkOutline,
  initialBoundWorldId?: string,
  onPatch = vi.fn(),
) {
  const props = {
    outline: initialOutline,
    selectedChapterId: null,
    baseRevision: initialOutline.outline_revision,
    onPatchTimeline: onPatch,
  };
  const view = render(<TimelinePanel {...props} boundWorldId={initialBoundWorldId} />);
  function setBoundWorldId(boundWorldId?: string) {
    view.rerender(<TimelinePanel {...props} boundWorldId={boundWorldId} />);
  }
  return { onPatch, setBoundWorldId };
}

describe('TimelinePanel — foreshadow link control (FB-C1-005)', () => {
  it('shows a Link Foreshadow selector for each event when other events exist', () => {
    renderTimeline(makeOutline());

    const selectors = screen.getAllByLabelText(/Foreshadow target for/);
    expect(selectors).toHaveLength(2);

    expect(screen.getAllByRole('button', { name: 'Link' })).toHaveLength(2);
  });

  it('fires link_foreshadow op with event_id and foreshadows_event_id', async () => {
    const user = userEvent.setup();
    const onPatch = renderTimeline(makeOutline());

    const selectors = screen.getAllByLabelText(/Foreshadow target for/);
    await user.selectOptions(selectors[0], 'evt_b');

    await user.click(screen.getAllByRole('button', { name: 'Link' })[0]);

    expect(onPatch).toHaveBeenCalledTimes(1);
    const call = onPatch.mock.calls[0][0] as TimelinePatchEventRequest;
    expect(call.operation).toBe('link_foreshadow');
    expect(call.event_id).toBe('evt_a');
    expect(call.foreshadows_event_id).toBe('evt_b');
  });

  it('disables the Link Foreshadow button until a target is selected', () => {
    renderTimeline(makeOutline());

    const buttons = screen.getAllByRole('button', { name: 'Link' });
    for (const btn of buttons) {
      expect(btn).toBeDisabled();
    }
  });

  it('hides the link control when there are no linkable events (single event)', () => {
    renderTimeline(
      makeOutline({
        timeline_events: [
          { event_id: 'evt_solo', title: 'Only event', description: undefined, realizes_chapter_id: 1 },
        ],
      }),
    );

    expect(screen.queryByRole('button', { name: 'Link' })).not.toBeInTheDocument();
  });
});

describe('TimelinePanel — foreshadow unlink control (FB-C1-005)', () => {
  it('renders existing foreshadow links with the target title', () => {
    renderTimeline(
      makeOutline({
        foreshadows: [{ source_event_id: 'evt_a', target_event_id: 'evt_b' }],
      }),
    );

    expect(screen.getByText(/Foreshadows The payoff/)).toBeInTheDocument();
  });

  it('fires unlink_foreshadow op with event_id and foreshadows_event_id', async () => {
    const user = userEvent.setup();
    const onPatch = renderTimeline(
      makeOutline({
        foreshadows: [{ source_event_id: 'evt_a', target_event_id: 'evt_b' }],
      }),
    );

    const unlinkBtn = screen.getByRole('button', {
      name: /Unlink foreshadow to The payoff/i,
    });
    await user.click(unlinkBtn);

    expect(onPatch).toHaveBeenCalledTimes(1);
    const call = onPatch.mock.calls[0][0] as TimelinePatchEventRequest;
    expect(call.operation).toBe('unlink_foreshadow');
    expect(call.event_id).toBe('evt_a');
    expect(call.foreshadows_event_id).toBe('evt_b');
  });

  it('excludes already-linked targets from the link dropdown', () => {
    renderTimeline(
      makeOutline({
        timeline_events: [
          { event_id: 'evt_a', title: 'Plant', description: undefined, realizes_chapter_id: 1 },
          { event_id: 'evt_b', title: 'Payoff', description: undefined, realizes_chapter_id: 2 },
          { event_id: 'evt_c', title: 'Twist', description: undefined, realizes_chapter_id: 3 },
        ],
        foreshadows: [{ source_event_id: 'evt_a', target_event_id: 'evt_b' }],
      }),
    );

    const evt_a_select = screen.getByLabelText('Foreshadow target for Plant');
    const options = within(evt_a_select).getAllByRole('option');
    const optionValues = options.map((o) => (o as HTMLOptionElement).value);
    expect(optionValues).not.toContain('evt_b');
    expect(optionValues).toContain('evt_c');
  });
});

describe('TimelinePanel — existing behavior regression', () => {
  it('still renders the Add Event composer and fires add_event', async () => {
    const user = userEvent.setup();
    const onPatch = renderTimeline(makeOutline());

    const input = screen.getByPlaceholderText('Event title…');
    await user.type(input, 'New beat');
    await user.click(screen.getByRole('button', { name: /^Add$/i }));

    expect(onPatch).toHaveBeenCalledTimes(1);
    const call = onPatch.mock.calls[0][0] as TimelinePatchEventRequest;
    expect(call.operation).toBe('add_event');
    expect(call.title).toBe('New beat');
  });

  it('still fires remove_event', async () => {
    const user = userEvent.setup();
    const onPatch = renderTimeline(makeOutline());

    await user.click(screen.getByRole('button', { name: /Remove event Plant the seed/i }));

    const call = onPatch.mock.calls[0][0] as TimelinePatchEventRequest;
    expect(call.operation).toBe('remove_event');
    expect(call.event_id).toBe('evt_a');
  });
});

describe('TimelinePanel — attach event to chapter (I-QC1-004)', () => {
  it('fires attach_event_to_chapter with target_chapter_id, not realizes_chapter_id', async () => {
    const user = userEvent.setup();
    const onPatch = renderTimeline(makeOutline(), vi.fn(), 5);

    const attachBtns = screen.getAllByRole('button', {
      name: /Attach event to chapter 5/i,
    });
    expect(attachBtns).toHaveLength(2);
    await user.click(attachBtns[0]);

    expect(onPatch).toHaveBeenCalledTimes(1);
    const call = onPatch.mock.calls[0][0] as TimelinePatchEventRequest;
    expect(call.operation).toBe('attach_event_to_chapter');
    expect(call.target_chapter_id).toBe(5);
    expect(call.realizes_chapter_id).toBeUndefined();
  });
});

describe('TimelinePanel — World-event binding authoring (V1.200 DR-26)', () => {
  it('offers only the bound World’s event entities in the picker', () => {
    renderTimeline(makeOutline(), vi.fn(), null, 'world-9');

    const picker = screen.getByLabelText('World event for Plant the seed');
    const optionValues = within(picker)
      .getAllByRole('option')
      .map((option) => (option as HTMLOptionElement).value);
    expect(optionValues).toEqual(['', 'kb-evt-1']);
    expect(
      screen.getByText(
        'The picker covers the first 500 World entities. Enter an event ID to bind any valid event.',
      ),
    ).toBeInTheDocument();
  });

  it('submits a manually entered event ID through the existing binding mutation path', async () => {
    const user = userEvent.setup();
    const onPatch = renderTimeline(makeOutline(), vi.fn(), null, 'world-9');

    await user.type(
      screen.getByLabelText('World event ID for Plant the seed'),
      'kb-outside-picker',
    );
    await user.click(screen.getAllByRole('button', { name: 'Add ID' })[0]);

    expect(onPatch).toHaveBeenCalledTimes(1);
    expect(onPatch.mock.calls[0][0]).toMatchObject({
      operation: 'bind_world_event',
      event_id: 'evt_a',
      world_event_id: 'kb-outside-picker',
    });
  });

  it('keeps the typed manual event ID until the write succeeds (a refusal keeps the draft)', async () => {
    const user = userEvent.setup();
    const successCallbacks: Array<() => void> = [];
    const onPatch = vi.fn(
      (_request: TimelinePatchEventRequest, onSuccess?: () => void) => {
        if (onSuccess) successCallbacks.push(onSuccess);
      },
    );
    renderTimeline(makeOutline(), onPatch, null, 'world-9');

    await user.type(
      screen.getByLabelText('World event ID for Plant the seed'),
      'kb-outside-picker',
    );
    await user.click(screen.getAllByRole('button', { name: 'Add ID' })[0]);

    expect(onPatch).toHaveBeenCalledTimes(1);
    const refused = screen.getByLabelText('World event ID for Plant the seed');
    expect(refused).toHaveValue('kb-outside-picker');
    expect(screen.getAllByRole('button', { name: 'Add ID' })[0]).not.toBeDisabled();

    act(() => successCallbacks[0]());
    expect(refused).toHaveValue('');
    expect(screen.getAllByRole('button', { name: 'Add ID' })[0]).toBeDisabled();
  });

  it('keeps a manual ID edited while the bind is in flight (002/R4)', async () => {
    const user = userEvent.setup();
    const successCallbacks: Array<() => void> = [];
    const onPatch = vi.fn(
      (_request: TimelinePatchEventRequest, onSuccess?: () => void) => {
        if (onSuccess) successCallbacks.push(onSuccess);
      },
    );
    renderTimeline(makeOutline(), onPatch, null, 'world-9');

    await user.type(
      screen.getByLabelText('World event ID for Plant the seed'),
      'kb-outside-picker',
    );
    await user.click(screen.getAllByRole('button', { name: 'Add ID' })[0]);
    expect(onPatch).toHaveBeenCalledTimes(1);

    const field = screen.getByLabelText('World event ID for Plant the seed');
    await user.clear(field);
    await user.type(field, 'kb-newer-id');

    act(() => successCallbacks[0]());
    expect(screen.getByLabelText('World event ID for Plant the seed')).toHaveValue('kb-newer-id');
    expect(screen.getAllByRole('button', { name: 'Add ID' })[0]).not.toBeDisabled();
  });

  it('keeps a manual ID retyped to the issued value while the bind is in flight (002/R4)', async () => {
    const user = userEvent.setup();
    const successCallbacks: Array<() => void> = [];
    const onPatch = vi.fn(
      (_request: TimelinePatchEventRequest, onSuccess?: () => void) => {
        if (onSuccess) successCallbacks.push(onSuccess);
      },
    );
    renderTimeline(makeOutline(), onPatch, null, 'world-9');

    const field = screen.getByLabelText('World event ID for Plant the seed');
    await user.type(field, 'kb-outside-picker');
    await user.click(screen.getAllByRole('button', { name: 'Add ID' })[0]);
    expect(onPatch).toHaveBeenCalledTimes(1);

    await user.clear(field);
    await user.type(field, 'kb-other-id');
    await user.clear(field);
    await user.type(field, 'kb-outside-picker');

    act(() => successCallbacks[0]());
    expect(screen.getByLabelText('World event ID for Plant the seed')).toHaveValue(
      'kb-outside-picker',
    );
  });

  it('keeps a manual ID entered during an in-flight picker bind of the same ID (002/R4)', async () => {
    const user = userEvent.setup();
    const successCallbacks: Array<() => void> = [];
    const onPatch = vi.fn(
      (_request: TimelinePatchEventRequest, onSuccess?: () => void) => {
        if (onSuccess) successCallbacks.push(onSuccess);
      },
    );
    renderTimeline(makeOutline(), onPatch, null, 'world-9');

    const picker = screen.getByLabelText('World event for Plant the seed');
    await user.selectOptions(picker, 'kb-evt-1');
    await user.click(screen.getAllByRole('button', { name: 'Bind' })[0]);
    expect(onPatch).toHaveBeenCalledTimes(1);

    await user.type(screen.getByLabelText('World event ID for Plant the seed'), 'kb-evt-1');

    act(() => successCallbacks[0]());
    expect(screen.getByLabelText('World event ID for Plant the seed')).toHaveValue('kb-evt-1');
    expect(screen.getByLabelText('World event for Plant the seed')).toHaveValue('');
  });

  // A draft that already sat in the *other* control is not part of this
  // request; the picker bind's success must leave the pre-existing manual
  // draft alone (it is not a draft authored during the flight).
  it('keeps a pre-existing manual draft when an unrelated picker bind succeeds (002/R4)', async () => {
    const user = userEvent.setup();
    const successCallbacks: Array<() => void> = [];
    const onPatch = vi.fn(
      (_request: TimelinePatchEventRequest, onSuccess?: () => void) => {
        if (onSuccess) successCallbacks.push(onSuccess);
      },
    );
    renderTimeline(makeOutline(), onPatch, null, 'world-9');

    await user.selectOptions(
      screen.getByLabelText('World event for Plant the seed'),
      'kb-evt-1',
    );
    await user.type(
      screen.getByLabelText('World event ID for Plant the seed'),
      'kb-manual-id',
    );

    await user.click(screen.getAllByRole('button', { name: 'Bind' })[0]);
    expect(onPatch).toHaveBeenCalledTimes(1);

    act(() => successCallbacks[0]());
    expect(screen.getByLabelText('World event for Plant the seed')).toHaveValue('');
    expect(screen.getByLabelText('World event ID for Plant the seed')).toHaveValue(
      'kb-manual-id',
    );
  });

  it('keeps the picked World event until the write succeeds', async () => {
    const user = userEvent.setup();
    const successCallbacks: Array<() => void> = [];
    const onPatch = vi.fn(
      (_request: TimelinePatchEventRequest, onSuccess?: () => void) => {
        if (onSuccess) successCallbacks.push(onSuccess);
      },
    );
    renderTimeline(makeOutline(), onPatch, null, 'world-9');

    const picker = screen.getByLabelText('World event for Plant the seed');
    await user.selectOptions(picker, 'kb-evt-1');
    await user.click(screen.getAllByRole('button', { name: 'Bind' })[0]);

    expect(onPatch).toHaveBeenCalledTimes(1);
    expect(picker).toHaveValue('kb-evt-1');

    act(() => successCallbacks[0]());
    expect(picker).toHaveValue('');
  });

  it('fires bind_world_event and renders the refetched canonical binding (projected referent + Unbind)', async () => {
    const user = userEvent.setup();
    const onPatch = renderTimelineRefetching(makeOutline(), 'world-9');

    await user.selectOptions(screen.getByLabelText('World event for Plant the seed'), 'kb-evt-1');
    await user.click(screen.getAllByRole('button', { name: 'Bind' })[0]);

    expect(onPatch).toHaveBeenCalledTimes(1);
    const call = onPatch.mock.calls[0][0] as TimelinePatchEventRequest;
    expect(call.operation).toBe('bind_world_event');
    expect(call.event_id).toBe('evt_a');
    expect(call.world_event_id).toBe('kb-evt-1');

    expect(screen.getByText('World event: Coronation')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Unbind' })).toBeInTheDocument();
    expect(screen.queryByLabelText('World event for Plant the seed')).not.toBeInTheDocument();
    expect(screen.getByLabelText('World event for The payoff')).toBeInTheDocument();
  });

  it('keeps the Bind button inert until a World event is selected', () => {
    renderTimeline(makeOutline(), vi.fn(), null, 'world-9');

    for (const btn of screen.getAllByRole('button', { name: 'Bind' })) {
      expect(btn).toBeDisabled();
    }
  });

  it('clears the projected binding with unbind_world_event and renders the refetched event-only picker + Bind', async () => {
    const user = userEvent.setup();
    const onPatch = renderTimelineRefetching(
      makeOutline({
        timeline_events: [
          {
            event_id: 'evt_a',
            title: 'Plant the seed',
            realizes_chapter_id: 1,
            world_event_id: 'kb-evt-1',
          },
          { event_id: 'evt_b', title: 'The payoff', realizes_chapter_id: 2 },
        ],
      }),
      'world-9',
    );

    expect(screen.getByText('World event: Coronation')).toBeInTheDocument();
    expect(screen.getByLabelText('World event for The payoff')).toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: 'Unbind' }));

    expect(onPatch).toHaveBeenCalledTimes(1);
    const call = onPatch.mock.calls[0][0] as TimelinePatchEventRequest;
    expect(call.operation).toBe('unbind_world_event');
    expect(call.event_id).toBe('evt_a');

    expect(screen.queryByText('World event: Coronation')).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Unbind' })).not.toBeInTheDocument();
    expect(screen.getByLabelText('World event for Plant the seed')).toBeInTheDocument();
    expect(screen.getAllByRole('button', { name: 'Bind' })).toHaveLength(2);
  });

  it('disables — never hides — the binding control when the Work has no bound World', () => {
    renderTimeline(makeOutline());

    expect(screen.getByText('Bind a World to this Work to bind World events.')).toBeInTheDocument();
    expect(screen.getByLabelText('World event for Plant the seed')).toBeDisabled();

    const bindButtons = screen.getAllByRole('button', { name: 'Bind' });
    expect(bindButtons).toHaveLength(2);
    for (const btn of bindButtons) {
      expect(btn).toBeDisabled();
    }
  });
});

describe('TimelinePanel — bound World transition lifecycle (V1.201 002/R3)', () => {
  it('preserves the authoring drafts and adopts the bound World when the World resolves after first paint', async () => {
    const user = userEvent.setup();
    const { setBoundWorldId } = renderTimelineWithBoundWorld(makeOutline());

    const pickerBeforeBind = screen.getByLabelText('World event for Plant the seed');
    expect(pickerBeforeBind).toBeDisabled();
    expect(
      within(pickerBeforeBind)
        .getAllByRole('option')
        .map((option) => (option as HTMLOptionElement).value),
    ).toEqual(['']);

    await user.type(screen.getByPlaceholderText('Event title…'), 'Draft beat');
    await user.type(screen.getByPlaceholderText('Description (optional)…'), 'Draft note');
    await user.selectOptions(
      screen.getByLabelText('Foreshadow target for Plant the seed'),
      'evt_b',
    );

    const addButton = screen.getByRole('button', { name: /^Add$/i });

    setBoundWorldId('world-9');

    expect(screen.getByRole('button', { name: /^Add$/i })).toBe(addButton);
    expect(screen.getByPlaceholderText('Event title…')).toHaveValue('Draft beat');
    expect(screen.getByPlaceholderText('Description (optional)…')).toHaveValue('Draft note');
    expect(screen.getByLabelText('Foreshadow target for Plant the seed')).toHaveValue('evt_b');

    const pickerAfterBind = screen.getByLabelText('World event for Plant the seed');
    expect(pickerAfterBind).not.toBeDisabled();
    expect(
      within(pickerAfterBind)
        .getAllByRole('option')
        .map((option) => (option as HTMLOptionElement).value),
    ).toEqual(['', 'kb-evt-1']);
  });

  it('keeps the typed bind draft and re-projects the picker when the bound World changes', async () => {
    const user = userEvent.setup();
    const { setBoundWorldId } = renderTimelineWithBoundWorld(makeOutline(), 'world-9');

    await user.type(
      screen.getByLabelText('World event ID for Plant the seed'),
      'kb-manual-id',
    );

    setBoundWorldId('world-11');

    expect(screen.getByLabelText('World event ID for Plant the seed')).toHaveValue('kb-manual-id');
    const picker = screen.getByLabelText('World event for Plant the seed');
    expect(
      within(picker)
        .getAllByRole('option')
        .map((option) => (option as HTMLOptionElement).value),
    ).toEqual(['', 'kb-evt-7']);
    expect(within(picker).queryByRole('option', { name: 'Coronation' })).not.toBeInTheDocument();
  });
});

// The Work can be re-bound while the write is in flight, and the drafts
// survive that transition (R3), so the completion of a former World's bind
// must not consume a draft now shown under the newly bound World; a picker
// selection of the former World names an entity of that World and cannot carry
// into the new one.
describe('TimelinePanel — bound World rebind race (V1.201 002/FX-B)', () => {
  it('drops a former World picker selection across a World switch and keeps the manual draft', async () => {
    const user = userEvent.setup();
    const { setBoundWorldId } = renderTimelineWithBoundWorld(makeOutline(), 'world-9');

    await user.selectOptions(
      screen.getByLabelText('World event for Plant the seed'),
      'kb-evt-1',
    );
    await user.type(
      screen.getByLabelText('World event ID for Plant the seed'),
      'kb-manual-id',
    );

    setBoundWorldId('world-11');

    const picker = screen.getByLabelText('World event for Plant the seed');
    expect(picker).toHaveValue('');
    expect(
      within(picker)
        .getAllByRole('option')
        .map((option) => (option as HTMLOptionElement).value),
    ).toEqual(['', 'kb-evt-7']);
    expect(screen.getAllByRole('button', { name: 'Bind' })[0]).toBeDisabled();
    expect(screen.getByLabelText('World event ID for Plant the seed')).toHaveValue('kb-manual-id');
  });

  it('keeps the draft retained in the newly bound World when a former World bind lands', async () => {
    const user = userEvent.setup();
    const successCallbacks: Array<() => void> = [];
    const onPatch = vi.fn(
      (_request: TimelinePatchEventRequest, onSuccess?: () => void) => {
        if (onSuccess) successCallbacks.push(onSuccess);
      },
    );
    const { setBoundWorldId } = renderTimelineWithBoundWorld(makeOutline(), 'world-9', onPatch);

    await user.type(
      screen.getByLabelText('World event ID for Plant the seed'),
      'kb-manual-id',
    );
    await user.click(screen.getAllByRole('button', { name: 'Add ID' })[0]);
    expect(onPatch).toHaveBeenCalledTimes(1);

    setBoundWorldId('world-11');
    expect(screen.getByLabelText('World event ID for Plant the seed')).toHaveValue('kb-manual-id');

    act(() => successCallbacks[0]());
    expect(screen.getByLabelText('World event ID for Plant the seed')).toHaveValue('kb-manual-id');
  });

  it('keeps a picker draft authored in the newly bound World when a former World bind lands', async () => {
    const user = userEvent.setup();
    const successCallbacks: Array<() => void> = [];
    const onPatch = vi.fn(
      (_request: TimelinePatchEventRequest, onSuccess?: () => void) => {
        if (onSuccess) successCallbacks.push(onSuccess);
      },
    );
    const { setBoundWorldId } = renderTimelineWithBoundWorld(makeOutline(), 'world-9', onPatch);

    await user.selectOptions(
      screen.getByLabelText('World event for Plant the seed'),
      'kb-evt-1',
    );
    await user.click(screen.getAllByRole('button', { name: 'Bind' })[0]);
    expect(onPatch).toHaveBeenCalledTimes(1);

    setBoundWorldId('world-11');
    await user.selectOptions(
      screen.getByLabelText('World event for Plant the seed'),
      'kb-evt-7',
    );

    act(() => successCallbacks[0]());
    expect(screen.getByLabelText('World event for Plant the seed')).toHaveValue('kb-evt-7');
  });
});
