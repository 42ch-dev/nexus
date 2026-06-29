/**
 * OutlinePage route tests — V1.75 chapter preselect (F-QC3-001).
 *
 * The chapter-page "Edit outline → Canvas" CTA links to
 * `/works/{workId}/outline?chapter={n}`. The outline page MUST consume the
 * `chapter` query param to preselect that chapter node on the canvas on mount,
 * which opens the chapter inspector. Tests cover the preselect behavior and the
 * default (no param / malformed) empty-inspector state.
 *
 * Assertions deliberately target the inspector surface only. The structure
 * panel also renders a "Select a chapter to inspect or move it between
 * volumes." helper and a `#N` mono glyph per list row, so inspector queries use
 * the inspector-only strings ("Chapter Inspector" title, "...its outline
 * metadata." empty state, "...metadata exposed on the outline canvas." copy).
 */
import { http, HttpResponse } from 'msw';
import { describe, expect, it } from 'vitest';
import { Route, Routes } from 'react-router-dom';
import { screen } from '@testing-library/react';

import { renderInApp } from '@/test/test-providers';
import { useHandlers } from '@/test/msw-server';
import { chapterSummary, chaptersList, workDetail } from '@/test/handlers';
import { BrowserClient } from '@/lib/nexus';
import { OutlinePage } from '@/pages/outline-page';

const client = () => new BrowserClient();

/** `GET /v1/local/works/:workId/outline` → minimal canonical `WorkOutline`. */
function workOutlineHandler(chapterIds: number[]) {
  return http.get('/v1/local/works/:workId/outline', () =>
    HttpResponse.json({
      work_id: 'w-123',
      outline_revision: 1,
      volumes: [{ volume_id: 1, label: 'Volume 1', chapter_ids: chapterIds }],
      timeline_events: [],
      foreshadows: [],
      chapter_titles: {},
      updated_at: '2026-06-25T00:00:00Z',
    }),
  );
}

function renderOutline(initialEntry = '/works/w-123/outline') {
  return renderInApp(
    <Routes>
      <Route path="works/:workId/outline" element={<OutlinePage />} />
    </Routes>,
    {
      client: client(),
      initialRouterEntries: [initialEntry],
    },
  );
}

describe('OutlinePage chapter preselect (V1.75 F-QC3-001)', () => {
  it('preselects the chapter from ?chapter=N and opens its inspector on mount', async () => {
    useHandlers(
      workDetail('w-123', { title: 'Test Work' }),
      chaptersList([chapterSummary(1), chapterSummary(2), chapterSummary(3)]),
      workOutlineHandler([1, 2, 3]),
    );

    renderOutline('/works/w-123/outline?chapter=2');

    // The "Chapter Inspector" card title only renders when a chapter is
    // selected (the empty state returns a bare card without this title).
    expect(await screen.findByText('Chapter Inspector')).toBeInTheDocument();
    // The inspector description is unique to the inspector ("metadata exposed
    // on the outline canvas"); its textContent carries the selected chapter's
    // `#N`, locking that the preselected node is chapter 2 (not chapter 1).
    expect(
      screen.getByText(/metadata exposed on the outline canvas/i),
    ).toHaveTextContent('#2');
    // Empty state is gone.
    expect(
      screen.queryByText(/Select a chapter to inspect its outline metadata/i),
    ).not.toBeInTheDocument();
  });

  it('does not preselect when the chapter param is absent (empty inspector state)', async () => {
    useHandlers(
      workDetail('w-123', { title: 'Test Work' }),
      chaptersList([chapterSummary(1), chapterSummary(2)]),
      workOutlineHandler([1, 2]),
    );

    renderOutline('/works/w-123/outline');

    expect(
      await screen.findByText(/Select a chapter to inspect its outline metadata/i),
    ).toBeInTheDocument();
    expect(screen.queryByText('Chapter Inspector')).not.toBeInTheDocument();
  });

  it('ignores a non-positive / malformed chapter param and leaves the inspector empty', async () => {
    useHandlers(
      workDetail('w-123', { title: 'Test Work' }),
      chaptersList([chapterSummary(1)]),
      workOutlineHandler([1]),
    );

    renderOutline('/works/w-123/outline?chapter=0');

    expect(
      await screen.findByText(/Select a chapter to inspect its outline metadata/i),
    ).toBeInTheDocument();
    expect(screen.queryByText('Chapter Inspector')).not.toBeInTheDocument();
  });
});
