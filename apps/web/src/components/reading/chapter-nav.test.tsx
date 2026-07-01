/**
 * ChapterNav component tests (R-V179P0-QC1-002).
 *
 * Pure-presentation coverage for the prev/next chapter navigation surface:
 * link hrefs + labels, the first/last-chapter and loading placeholders, the
 * title-fallback rule, and the multi-volume chip visibility rule. The keyboard
 * shortcut behavior is covered separately in chapter-keyboard-nav.test.ts.
 */
import { screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { ChapterNav } from '@/components/reading/chapter-nav';
import { renderInApp } from '@/test/test-providers';
import type { ChapterSummary } from '@42ch/nexus-contracts';

function row(overrides: Partial<ChapterSummary> = {}): ChapterSummary {
  return {
    work_id: 'work-1',
    chapter: 1,
    volume: 1,
    planned_word_count: 0,
    status: 'draft',
    created_at: '2026-01-01T00:00:00Z',
    updated_at: '2026-01-01T00:00:00Z',
    ...overrides,
  };
}

describe('ChapterNav — prev/next links', () => {
  it('renders prev + next links with the chapter title and the volume in the href', () => {
    renderInApp(
      <ChapterNav
        workId="work-1"
        prev={row({ chapter: 2, volume: 1, title: 'The Arrival' })}
        next={row({ chapter: 4, volume: 3, title: 'The Departure' })}
        volumes={[1, 3]}
        currentVolume={1}
      />,
    );

    const prev = screen.getByRole('link', { name: /previous chapter: the arrival/i });
    const next = screen.getByRole('link', { name: /next chapter: the departure/i });
    expect(prev).toHaveAttribute('href', '/works/work-1/chapters/2?volume=1');
    expect(next).toHaveAttribute('href', '/works/work-1/chapters/4?volume=3');
  });

  it('falls back to "Chapter N" when the title is blank/missing', () => {
    renderInApp(
      <ChapterNav
        workId="work-1"
        prev={row({ chapter: 2, volume: 1, title: '   ' })}
        next={row({ chapter: 3, volume: 1 /* no title */ })}
        volumes={[1]}
      />,
    );

    expect(screen.getByRole('link', { name: /previous chapter: chapter 2/i })).toBeInTheDocument();
    expect(screen.getByRole('link', { name: /next chapter: chapter 3/i })).toBeInTheDocument();
  });

  it('URL-encodes the work id in the href', () => {
    renderInApp(
      <ChapterNav
        workId="work/1"
        prev={row({ chapter: 2, volume: 1 })}
        next={null}
        volumes={[1]}
      />,
    );

    expect(screen.getByRole('link', { name: /previous chapter/i })).toHaveAttribute(
      'href',
      '/works/work%2F1/chapters/2?volume=1',
    );
  });
});

describe('ChapterNav — boundary + loading placeholders', () => {
  it('shows the "First chapter" placeholder when there is no previous chapter', () => {
    renderInApp(
      <ChapterNav workId="work-1" prev={null} next={row({ chapter: 2, volume: 1 })} volumes={[1]} />,
    );

    expect(screen.getByLabelText('No previous chapter')).toBeInTheDocument();
    expect(screen.getByText('First chapter')).toBeInTheDocument();
  });

  it('shows the "Last chapter" placeholder when there is no next chapter', () => {
    renderInApp(
      <ChapterNav workId="work-1" prev={row({ chapter: 1, volume: 1 })} next={null} volumes={[1]} />,
    );

    expect(screen.getByLabelText('No next chapter')).toBeInTheDocument();
    expect(screen.getByText('Last chapter')).toBeInTheDocument();
  });

  it('shows a loading placeholder (not first/last) while chapters are still loading', () => {
    renderInApp(
      <ChapterNav workId="work-1" prev={null} next={null} volumes={[]} loading />,
    );

    // Two loading placeholders (prev + next side); neither boundary label shows.
    expect(screen.getAllByLabelText('Loading chapters')).toHaveLength(2);
    expect(screen.queryByText('First chapter')).not.toBeInTheDocument();
    expect(screen.queryByText('Last chapter')).not.toBeInTheDocument();
  });
});

describe('ChapterNav — volume chip', () => {
  it('shows the volume chip only when more than one volume is present', () => {
    const { rerender } = renderInApp(
      <ChapterNav
        workId="work-1"
        prev={row({ chapter: 1, volume: 1 })}
        next={row({ chapter: 2, volume: 2 })}
        volumes={[1, 2]}
        currentVolume={2}
      />,
    );

    expect(screen.getByLabelText('Volume 2')).toBeInTheDocument();
    expect(screen.getByText('Volume 2')).toBeInTheDocument();

    // Single-volume work: the chip must not render.
    rerender(
      // renderInApp returns the RTL render result; rerender stays inside the
      // same provider stack (MemoryRouter etc.) because `rerender` is bound to
      // the same container.
      <ChapterNav
        workId="work-1"
        prev={null}
        next={row({ chapter: 2, volume: 1 })}
        volumes={[1]}
      />,
    );

    expect(screen.queryByText(/^Volume \d+$/)).not.toBeInTheDocument();
  });

  it('falls back the chip label to 1 when currentVolume is undefined', () => {
    renderInApp(
      <ChapterNav
        workId="work-1"
        prev={row({ chapter: 1, volume: 1 })}
        next={row({ chapter: 2, volume: 2 })}
        volumes={[1, 2]}
        // currentVolume omitted on purpose — exercises the optional-prop default.
      />,
    );

    expect(screen.getByLabelText('Volume 1')).toBeInTheDocument();
  });
});
