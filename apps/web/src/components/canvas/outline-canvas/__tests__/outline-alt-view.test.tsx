/**
 * OutlineAltView component — renders non-spatial chapter + timeline lists
 * (FB-C1-004, V1.108 P0 T3).
 *
 * Unit-tested in isolation with richer data than the orchestrator mock so the
 * timeline list, multiple volumes, unassigned chapters, and empty states are
 * all covered.
 */
import { describe, expect, it } from 'vitest';
import { fireEvent, screen } from '@testing-library/react';

import { renderInApp } from '@/test/test-providers';
import { OutlineAltView } from '@/components/canvas/outline-canvas/outline-alt-view';
import type { SceneBeatFixturePayload } from '@/components/canvas/outline-canvas/graph-projection';
import type { ChapterSummary, WorkOutline } from '@42ch/nexus-contracts';

function makeOutline(overrides: Partial<WorkOutline> = {}): WorkOutline {
  return {
    work_id: 'wk_alt',
    outline_revision: 1,
    volumes: [],
    timeline_events: [],
    foreshadows: [],
    chapter_titles: {},
    updated_at: '',
    ...overrides,
  };
}

function makeChapter(overrides: Partial<ChapterSummary>): ChapterSummary {
  return {
    work_id: 'wk_alt',
    chapter: 1,
    volume: 1,
    title: undefined,
    slug: undefined,
    status: 'draft',
    planned_word_count: 0,
    actual_word_count: undefined,
    outline_path: undefined,
    body_path: undefined,
    created_at: '',
    updated_at: '',
    ...overrides,
  };
}

describe('OutlineAltView', () => {
  it('renders chapters grouped by volume with status badges', () => {
    const outline = makeOutline({
      volumes: [
        { volume_id: 1, label: 'Act One', chapter_ids: [1, 2] },
        { volume_id: 2, label: 'Act Two', chapter_ids: [3] },
      ],
      chapter_titles: { '1': 'The Beginning', '2': 'The Middle', '3': 'The End' },
    });
    const chapters = [
      makeChapter({ chapter: 1, status: 'draft' }),
      makeChapter({ chapter: 2, status: 'finalized' }),
      makeChapter({ chapter: 3, status: 'published', volume: 2 }),
    ];

    renderInApp(<OutlineAltView outline={outline} chapters={chapters} />);

    expect(screen.getByText('Act One')).toBeInTheDocument();
    expect(screen.getByText('Act Two')).toBeInTheDocument();
    expect(screen.getByText('The Beginning')).toBeInTheDocument();
    expect(screen.getByText('The Middle')).toBeInTheDocument();
    expect(screen.getByText('The End')).toBeInTheDocument();
    // Status badges render as translated labels.
    expect(screen.getByText('Draft')).toBeInTheDocument();
    expect(screen.getByText('Finalized')).toBeInTheDocument();
    expect(screen.getByText('Published')).toBeInTheDocument();
  });

  it('renders unassigned chapters in a separate bucket', () => {
    const outline = makeOutline({
      volumes: [{ volume_id: 1, label: 'Act One', chapter_ids: [1] }],
    });
    const chapters = [
      makeChapter({ chapter: 1 }),
      makeChapter({ chapter: 2, volume: 1 }), // not referenced by any volume
    ];

    renderInApp(<OutlineAltView outline={outline} chapters={chapters} />);

    expect(screen.getByText('Unassigned')).toBeInTheDocument();
    expect(screen.getByText('#2')).toBeInTheDocument();
  });

  it('renders timeline events with descriptions and realized chapters', () => {
    const outline = makeOutline({
      timeline_events: [
        {
          event_id: 'evt1',
          title: 'Inciting Incident',
          description: 'The call to adventure',
          realizes_chapter_id: 1,
        },
        {
          event_id: 'evt2',
          title: 'Climax',
          description: undefined,
          realizes_chapter_id: undefined,
        },
      ],
    });

    renderInApp(<OutlineAltView outline={outline} chapters={[]} />);

    expect(screen.getByText('Inciting Incident')).toBeInTheDocument();
    expect(screen.getByText('The call to adventure')).toBeInTheDocument();
    expect(screen.getByText('Realizes chapter 1')).toBeInTheDocument();
    expect(screen.getByText('Climax')).toBeInTheDocument();
  });

  it('shows honest empty messages when no data', () => {
    const outline = makeOutline();

    renderInApp(<OutlineAltView outline={outline} chapters={[]} />);

    expect(screen.getByText('No chapters yet.')).toBeInTheDocument();
    expect(screen.getByText('No timeline events yet.')).toBeInTheDocument();
  });

  // -------------------------------------------------------------------------
  // V1.109 C2 T3 — Scene/Beat nested rows (FB-C2-003)
  // -------------------------------------------------------------------------

  it('renders Scene rows nested under their parent chapter with a Scene type badge', () => {
    const outline = makeOutline({
      volumes: [{ volume_id: 1, label: 'Act One', chapter_ids: [1] }],
    });
    const chapters = [makeChapter({ chapter: 1 })];
    const fixture: SceneBeatFixturePayload = {
      scenes: [
        { sceneId: 's1', chapterId: 1, title: 'The Arrival', status: 'drafted' },
        { sceneId: 's2', chapterId: 1, title: 'The Departure', status: null },
      ],
      beats: [],
    };

    renderInApp(<OutlineAltView outline={outline} chapters={chapters} sceneBeatFixture={fixture} />);

    expect(screen.getByText('The Arrival')).toBeInTheDocument();
    expect(screen.getByText('The Departure')).toBeInTheDocument();
    // Two Scene type badges (one per scene row).
    expect(screen.getAllByText('Scene')).toHaveLength(2);
  });

  it('renders Beat rows nested under their parent Scene (Scene->Beat nesting)', () => {
    const outline = makeOutline({
      volumes: [{ volume_id: 1, label: 'Act One', chapter_ids: [1] }],
    });
    const chapters = [makeChapter({ chapter: 1 })];
    const fixture: SceneBeatFixturePayload = {
      scenes: [{ sceneId: 's1', chapterId: 1, title: 'The Arrival', status: null }],
      beats: [
        { beatId: 'b1', sceneId: 's1', title: 'Turn: the call', status: null },
        { beatId: 'b2', sceneId: 's1', title: 'The rebutal', status: null },
      ],
    };

    renderInApp(<OutlineAltView outline={outline} chapters={chapters} sceneBeatFixture={fixture} />);

    expect(screen.getByText('Turn: the call')).toBeInTheDocument();
    expect(screen.getByText('The rebutal')).toBeInTheDocument();
    // One Scene badge + two Beat badges.
    expect(screen.getAllByText('Scene')).toHaveLength(1);
    expect(screen.getAllByText('Beat')).toHaveLength(2);
  });

  it('shows the empty-under-chapter helper when a chapter has zero scenes', () => {
    const outline = makeOutline({
      volumes: [{ volume_id: 1, label: 'Act One', chapter_ids: [1, 2] }],
    });
    const chapters = [makeChapter({ chapter: 1 }), makeChapter({ chapter: 2 })];
    // Only chapter 1 has a scene; chapter 2 has zero.
    const fixture: SceneBeatFixturePayload = {
      scenes: [{ sceneId: 's1', chapterId: 1, title: 'The Arrival', status: null }],
      beats: [],
    };

    renderInApp(<OutlineAltView outline={outline} chapters={chapters} sceneBeatFixture={fixture} />);

    // Locked empty-under-chapter copy appears once (chapter 2 only).
    expect(screen.getAllByText('No scenes in this chapter yet.')).toHaveLength(1);
  });

  it('does not show the empty-under-chapter helper when no fixture is passed (real Works)', () => {
    const outline = makeOutline({
      volumes: [{ volume_id: 1, label: 'Act One', chapter_ids: [1] }],
    });
    const chapters = [makeChapter({ chapter: 1 })];

    renderInApp(<OutlineAltView outline={outline} chapters={chapters} />);

    expect(screen.queryByText('No scenes in this chapter yet.')).not.toBeInTheDocument();
  });

  it('renders Scene rows for unassigned chapters too', () => {
    const outline = makeOutline({
      volumes: [{ volume_id: 1, label: 'Act One', chapter_ids: [1] }],
    });
    const chapters = [
      makeChapter({ chapter: 1 }),
      makeChapter({ chapter: 2, volume: 1 }), // unassigned
    ];
    const fixture: SceneBeatFixturePayload = {
      scenes: [{ sceneId: 's2', chapterId: 2, title: 'Loose thread', status: null }],
      beats: [],
    };

    renderInApp(<OutlineAltView outline={outline} chapters={chapters} sceneBeatFixture={fixture} />);

    expect(screen.getByText('Loose thread')).toBeInTheDocument();
    expect(screen.getAllByText('Scene')).toHaveLength(1);
  });
});

// ---------------------------------------------------------------------------
// V1.115 T1 — Alt-view sort controls (R-V1108P0QC1-M001)
//
// Locked sort columns: chapters = number + volume; timeline events = event
// time. Sort is client-side and ephemeral (useState, not persisted). The
// default (volume ascending) reproduces the historical grouped rendering, so
// the tests above remain valid untouched.
// -------------------------------------------------------------------------

describe('OutlineAltView sort controls', () => {
  it('sorts the chapter list by Number ascending, then toggles to descending', () => {
    // Volume declares chapters out of numeric order (3, 1, 2).
    const outline = makeOutline({
      volumes: [{ volume_id: 1, label: 'Act One', chapter_ids: [3, 1, 2] }],
      chapter_titles: { '1': 'Alpha', '2': 'Beta', '3': 'Gamma' },
    });
    const chapters = [
      makeChapter({ chapter: 1 }),
      makeChapter({ chapter: 2 }),
      makeChapter({ chapter: 3 }),
    ];

    renderInApp(<OutlineAltView outline={outline} chapters={chapters} />);

    // Default (volume ascending) keeps declaration order: 3, 1, 2.
    expect(chapterNumberOrder()).toEqual(['#3', '#1', '#2']);

    // Activate Number sort → flat list ascending 1, 2, 3.
    fireEvent.click(screen.getByRole('button', { name: /^Number/ }));
    expect(chapterNumberOrder()).toEqual(['#1', '#2', '#3']);

    // Toggle the same column → descending 3, 2, 1.
    fireEvent.click(screen.getByRole('button', { name: /^Number/ }));
    expect(chapterNumberOrder()).toEqual(['#3', '#2', '#1']);
  });

  it('sorts the chapter list by Volume descending (reverses volume groups)', () => {
    const outline = makeOutline({
      volumes: [
        { volume_id: 1, label: 'Act One', chapter_ids: [1] },
        { volume_id: 2, label: 'Act Two', chapter_ids: [2] },
      ],
    });
    const chapters = [makeChapter({ chapter: 1 }), makeChapter({ chapter: 2, volume: 2 })];

    renderInApp(<OutlineAltView outline={outline} chapters={chapters} />);

    // Default: Act One before Act Two.
    expect(volumeGroupOrder()).toEqual(['Act One', 'Act Two']);

    // Toggle Volume (active ascending → descending): groups reversed.
    fireEvent.click(screen.getByRole('button', { name: /^Volume/ }));
    expect(volumeGroupOrder()).toEqual(['Act Two', 'Act One']);
  });

  it('keeps chapter sort state ephemeral (fresh mount resets to default)', () => {
    const outline = makeOutline({
      volumes: [{ volume_id: 1, label: 'Act One', chapter_ids: [2, 1] }],
      chapter_titles: { '1': 'Alpha', '2': 'Beta' },
    });
    const chapters = [makeChapter({ chapter: 1 }), makeChapter({ chapter: 2 })];

    const { unmount } = renderInApp(<OutlineAltView outline={outline} chapters={chapters} />);
    // Sort to Number ascending: 1, 2.
    fireEvent.click(screen.getByRole('button', { name: /^Number/ }));
    expect(chapterNumberOrder()).toEqual(['#1', '#2']);

    // Tear down and remount — no persisted sort, back to default declaration order 2, 1.
    unmount();
    renderInApp(<OutlineAltView outline={outline} chapters={chapters} />);
    expect(chapterNumberOrder()).toEqual(['#2', '#1']);
  });

  it('does not mutate the outline prop (alt-view reflects underlying graph)', () => {
    const outline = makeOutline({
      volumes: [{ volume_id: 1, label: 'Act One', chapter_ids: [3, 1, 2] }],
      timeline_events: [
        { event_id: 'e1', title: 'First' },
        { event_id: 'e2', title: 'Second' },
      ],
    });
    const chapters = [makeChapter({ chapter: 1 }), makeChapter({ chapter: 2 }), makeChapter({ chapter: 3 })];
    const volumeChapterIdsBefore = outline.volumes.map((v) => v.chapter_ids.slice());
    const eventIdsBefore = outline.timeline_events.map((e) => e.event_id);

    renderInApp(<OutlineAltView outline={outline} chapters={chapters} />);
    fireEvent.click(screen.getByRole('button', { name: /^Number/ }));
    fireEvent.click(screen.getByRole('button', { name: /^Event time/ }));

    // Client-side sort reorders the view only; the source graph is untouched.
    expect(outline.volumes.map((v) => v.chapter_ids.slice())).toEqual(volumeChapterIdsBefore);
    expect(outline.timeline_events.map((e) => e.event_id)).toEqual(eventIdsBefore);
    // Every chapter is still represented after sorting.
    expect(chapterNumberOrder().sort()).toEqual(['#1', '#2', '#3']);
  });

  it('sorts the timeline event list by event time and toggles order', () => {
    const outline = makeOutline({
      timeline_events: [
        { event_id: 'e1', title: 'Inciting Incident' },
        { event_id: 'e2', title: 'Climax' },
      ],
    });

    renderInApp(<OutlineAltView outline={outline} chapters={[]} />);

    // Default ascending = declared order: Inciting Incident before Climax.
    expect(textIndexOf('Inciting Incident')).toBeLessThan(textIndexOf('Climax'));

    // Toggle event time → descending reverses the timeline.
    fireEvent.click(screen.getByRole('button', { name: /^Event time/ }));
    expect(textIndexOf('Climax')).toBeLessThan(textIndexOf('Inciting Incident'));
  });

  it('keeps timeline sort state ephemeral (fresh mount resets to ascending)', () => {
    const outline = makeOutline({
      timeline_events: [
        { event_id: 'e1', title: 'Inciting Incident' },
        { event_id: 'e2', title: 'Climax' },
      ],
    });

    const { unmount } = renderInApp(<OutlineAltView outline={outline} chapters={[]} />);
    fireEvent.click(screen.getByRole('button', { name: /^Event time/ }));
    expect(textIndexOf('Climax')).toBeLessThan(textIndexOf('Inciting Incident')); // desc

    unmount();
    renderInApp(<OutlineAltView outline={outline} chapters={[]} />);
    // Fresh mount → ascending (declared order) again.
    expect(textIndexOf('Inciting Incident')).toBeLessThan(textIndexOf('Climax'));
  });
});

/** Chapter-number spans (`#N`) in document order — the sort discriminator. */
function chapterNumberOrder(): string[] {
  return screen.getAllByText(/^#\d+$/).map((el) => el.textContent ?? '');
}

/** Volume group header labels in document order. */
function volumeGroupOrder(): string[] {
  return screen.getAllByText(/^Act (One|Two)$/).map((el) => el.textContent ?? '');
}

/** First document position of a needle string — used for timeline order checks. */
function textIndexOf(needle: string): number {
  return (document.body.textContent ?? '').indexOf(needle);
}
