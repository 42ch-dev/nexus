/**
 * Event inspector — foreshadow authoring controls (FB-C1-005, V1.108 P0 T4).
 *
 * Verifies the Link Foreshadow / Unlink Foreshadow controls fire the correct
 * `timeline.patch_event` operations with the locked copy
 * ("Link Foreshadow" / "Unlink Foreshadow").
 */
import { describe, expect, it, vi } from 'vitest';
import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import { TimelinePanel } from '@/components/canvas/outline-canvas/inspectors/event-inspector';

import type { TimelinePatchEventRequest, WorkOutline } from '@42ch/nexus-contracts';

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

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
    chapter_titles: {},
    updated_at: '',
    ...overrides,
  };
}

function renderTimeline(
  outline: WorkOutline,
  onPatchTimeline = vi.fn(),
  selectedChapterId: number | null = null,
) {
  render(
    <TimelinePanel
      outline={outline}
      selectedChapterId={selectedChapterId}
      baseRevision={outline.outline_revision}
      onPatchTimeline={onPatchTimeline}
    />,
  );
  return onPatchTimeline;
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe('TimelinePanel — foreshadow link control (FB-C1-005)', () => {
  it('shows a Link Foreshadow selector for each event when other events exist', () => {
    renderTimeline(makeOutline());

    // Each event row should have a "Link foreshadow to…" option.
    const selectors = screen.getAllByLabelText(/Foreshadow target for/);
    expect(selectors).toHaveLength(2);

    // The locked button copy must be present.
    expect(screen.getAllByRole('button', { name: 'Link Foreshadow' })).toHaveLength(2);
  });

  it('fires link_foreshadow op with event_id and foreshadows_event_id', async () => {
    const user = userEvent.setup();
    const onPatch = renderTimeline(makeOutline());

    // Select the target in the first event's dropdown.
    const selectors = screen.getAllByLabelText(/Foreshadow target for/);
    await user.selectOptions(selectors[0], 'evt_b');

    await user.click(screen.getAllByRole('button', { name: 'Link Foreshadow' })[0]);

    expect(onPatch).toHaveBeenCalledTimes(1);
    const call = onPatch.mock.calls[0][0] as TimelinePatchEventRequest;
    expect(call.operation).toBe('link_foreshadow');
    expect(call.event_id).toBe('evt_a');
    expect(call.foreshadows_event_id).toBe('evt_b');
  });

  it('disables the Link Foreshadow button until a target is selected', () => {
    renderTimeline(makeOutline());

    const buttons = screen.getAllByRole('button', { name: 'Link Foreshadow' });
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

    expect(screen.queryByRole('button', { name: 'Link Foreshadow' })).not.toBeInTheDocument();
  });
});

describe('TimelinePanel — foreshadow unlink control (FB-C1-005)', () => {
  it('renders existing foreshadow links with the target title', () => {
    renderTimeline(
      makeOutline({
        foreshadows: [{ source_event_id: 'evt_a', target_event_id: 'evt_b' }],
      }),
    );

    // The foreshadow chip should show the target event title.
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
    // Three events: evt_a already foreshadows evt_b; evt_c should still be
    // available but evt_b must be excluded from evt_a's dropdown.
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
    await user.click(screen.getByRole('button', { name: /Add to timeline/i }));

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

// I-QC1-004 — the attach_event_to_chapter op must send `target_chapter_id`
// (not `realizes_chapter_id`), matching the daemon/schema contract. The daemon
// handler returns `missing_target_chapter_id` if the field is absent.
describe('TimelinePanel — attach event to chapter (I-QC1-004)', () => {
  it('fires attach_event_to_chapter with target_chapter_id, not realizes_chapter_id', async () => {
    const user = userEvent.setup();
    // selectedChapterId = 5; evt_a already realizes chapter 1, so the attach
    // button should be visible for evt_a.
    const onPatch = renderTimeline(makeOutline(), vi.fn(), 5);

    // Both events show an attach button (neither realizes chapter 5). Click
    // evt_a's button to verify the payload field name.
    const attachBtns = screen.getAllByRole('button', {
      name: /Attach event to chapter 5/i,
    });
    expect(attachBtns).toHaveLength(2);
    await user.click(attachBtns[0]);

    expect(onPatch).toHaveBeenCalledTimes(1);
    const call = onPatch.mock.calls[0][0] as TimelinePatchEventRequest;
    expect(call.operation).toBe('attach_event_to_chapter');
    expect(call.target_chapter_id).toBe(5);
    // realizes_chapter_id must NOT be set on the attach op — it's only valid
    // for add_event. The daemon ignores it and returns missing_target_chapter_id.
    expect(call.realizes_chapter_id).toBeUndefined();
  });
});
