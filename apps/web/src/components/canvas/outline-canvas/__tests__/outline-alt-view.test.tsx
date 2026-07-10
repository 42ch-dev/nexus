/**
 * OutlineAltView component — renders non-spatial chapter + timeline lists
 * (FB-C1-004, V1.108 P0 T3).
 *
 * Unit-tested in isolation with richer data than the orchestrator mock so the
 * timeline list, multiple volumes, unassigned chapters, and empty states are
 * all covered.
 */
import { describe, expect, it } from 'vitest';
import { render, screen } from '@testing-library/react';

import { OutlineAltView } from '@/components/canvas/outline-canvas/outline-alt-view';
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

    render(<OutlineAltView outline={outline} chapters={chapters} />);

    expect(screen.getByText('Act One')).toBeInTheDocument();
    expect(screen.getByText('Act Two')).toBeInTheDocument();
    expect(screen.getByText('The Beginning')).toBeInTheDocument();
    expect(screen.getByText('The Middle')).toBeInTheDocument();
    expect(screen.getByText('The End')).toBeInTheDocument();
    // Status badges render (lowercased from replace /_/g).
    expect(screen.getByText('draft')).toBeInTheDocument();
    expect(screen.getByText('finalized')).toBeInTheDocument();
    expect(screen.getByText('published')).toBeInTheDocument();
  });

  it('renders unassigned chapters in a separate bucket', () => {
    const outline = makeOutline({
      volumes: [{ volume_id: 1, label: 'Act One', chapter_ids: [1] }],
    });
    const chapters = [
      makeChapter({ chapter: 1 }),
      makeChapter({ chapter: 2, volume: 1 }), // not referenced by any volume
    ];

    render(<OutlineAltView outline={outline} chapters={chapters} />);

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

    render(<OutlineAltView outline={outline} chapters={[]} />);

    expect(screen.getByText('Inciting Incident')).toBeInTheDocument();
    expect(screen.getByText('The call to adventure')).toBeInTheDocument();
    expect(screen.getByText('Realizes chapter 1')).toBeInTheDocument();
    expect(screen.getByText('Climax')).toBeInTheDocument();
  });

  it('shows honest empty messages when no data', () => {
    const outline = makeOutline();

    render(<OutlineAltView outline={outline} chapters={[]} />);

    expect(screen.getByText('No chapters yet.')).toBeInTheDocument();
    expect(screen.getByText('No timeline events yet.')).toBeInTheDocument();
  });
});
