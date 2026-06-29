/**
 * ChapterPage render tests — V1.75 Canvas-Pivot morph (A5).
 *
 * The V1.65 outline editor is retired. This page is now a read-only body view
 * + "Edit outline → Canvas" CTA. Tests cover: the CTA points at the outline
 * canvas with the chapter preselect, the body read-only render + frontmatter
 * strip, Copy Path, the body error/retry state, and the right-click context
 * menu (kept verbatim from V1.65). Outline-editor / save / soft-concurrency /
 * protected-edit assertions were removed with the editor.
 */
import { http, HttpResponse } from 'msw';
import { describe, expect, it, vi } from 'vitest';
import { Route, Routes } from 'react-router-dom';

import { renderInApp } from '@/test/test-providers';
import { useHandlers } from '@/test/msw-server';
import { BrowserClient } from '@/lib/nexus';
import { ChapterPage } from '@/pages/chapter-page';
import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

const client = () => new BrowserClient();

function renderChapter(workId = 'w-123', chapter = 1) {
  return renderInApp(
    <Routes>
      <Route path="works/:workId/chapters/:chapter" element={<ChapterPage />} />
    </Routes>,
    {
      client: client(),
      initialRouterEntries: [`/works/${workId}/chapters/${chapter}`],
    },
  );
}

function chapterDetail(status: string) {
  return http.get('/v1/local/works/:workId/chapters/:n', ({ params }) =>
    HttpResponse.json({
      work_id: params.workId,
      chapter: Number(params.n),
      volume: 1,
      slug: 'ch01',
      planned_word_count: 4000,
      status,
      can_edit_structure: true,
      body_read_only: true,
      created_at: '2026-06-25T00:00:00Z',
      updated_at: '2026-06-25T00:00:00Z',
    }),
  );
}

function bodyHandler(content = 'Body prose.', frontmatter?: Record<string, unknown>) {
  return http.get('/v1/local/works/:workId/chapters/:n/body', () =>
    HttpResponse.json({
      work_id: 'w-123',
      chapter: 1,
      volume: 1,
      body_path: 'Works/WRK/Stories/ch01-ch01.md',
      content,
      frontmatter: frontmatter ?? { status: 'draft' },
      read_only: true,
      updated_at: '2026-06-25T00:00:00Z',
    }),
  );
}

describe('ChapterPage (V1.75 read-only morph)', () => {
  it('renders the canvas redirect CTA pointing at the outline canvas with the chapter preselect', async () => {
    useHandlers(chapterDetail('not_started'), bodyHandler());

    renderChapter();
    const cta = await screen.findByRole('link', {
      name: /Edit outline for Chapter 1 on the outline canvas/i,
    });
    expect(cta).toHaveAttribute('href', '/works/w-123/outline?chapter=1');
  });

  it('renders the chapter header (number + status badge + back link)', async () => {
    useHandlers(chapterDetail('draft'), bodyHandler());

    renderChapter();
    expect(await screen.findByText('Chapter 1')).toBeInTheDocument();
    expect(screen.getByRole('link', { name: /Back to Chapters/i })).toBeInTheDocument();
  });

  it('renders the body read-only and strips frontmatter', async () => {
    useHandlers(
      chapterDetail('draft'),
      bodyHandler('---\nstatus: draft\n---\n\nBody prose.', { status: 'draft' }),
    );

    renderChapter();
    expect(await screen.findByText('Body prose.')).toBeInTheDocument();
    expect(screen.queryByText('---')).not.toBeInTheDocument();
    expect(screen.getByText(/Works\/WRK\/Stories\/ch01-ch01\.md/)).toBeInTheDocument();
  });

  it('copies the body path via the Copy Path button', async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.assign(navigator, { clipboard: { writeText } });

    useHandlers(chapterDetail('draft'), bodyHandler());

    renderChapter();
    await screen.findByText('Body prose.');
    await userEvent.click(screen.getByText('Copy Path'));
    expect(writeText).toHaveBeenCalledWith('Works/WRK/Stories/ch01-ch01.md');
  });

  it('shows the body error state and retry action', async () => {
    useHandlers(
      chapterDetail('draft'),
      http.get('/v1/local/works/:workId/chapters/:n/body', () =>
        HttpResponse.json(
          { success: false, error: { code: 'internal', message: 'boom' } },
          { status: 500 },
        ),
      ),
    );

    renderChapter();
    expect(await screen.findByText(/Could not load the chapter body/i)).toBeInTheDocument();
  });

  it('opens the context menu on right-click and copies the path', async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.assign(navigator, { clipboard: { writeText } });

    useHandlers(chapterDetail('draft'), bodyHandler());

    renderChapter();
    await screen.findByText('Body prose.');
    const bodyRegion = screen.getByRole('region', { name: 'Chapter body' });
    await userEvent.pointer([{ keys: '[MouseRight]', target: bodyRegion }]);
    expect(await screen.findByRole('menu')).toBeInTheDocument();
    await userEvent.click(screen.getByRole('menuitem', { name: /Copy Path/i }));
    expect(writeText).toHaveBeenCalledWith('Works/WRK/Stories/ch01-ch01.md');
  });

  it('closes the context menu with Escape and does not leak keydown listeners', async () => {
    const addListener = vi.spyOn(window, 'addEventListener');
    const removeListener = vi.spyOn(window, 'removeEventListener');

    useHandlers(chapterDetail('draft'), bodyHandler());

    renderChapter();
    await screen.findByText('Body prose.');
    const bodyRegion = screen.getByRole('region', { name: 'Chapter body' });

    addListener.mockClear();
    removeListener.mockClear();

    await userEvent.pointer([{ keys: '[MouseRight]', target: bodyRegion }]);
    expect(await screen.findByRole('menu')).toBeInTheDocument();

    await userEvent.keyboard('{Escape}');
    await waitFor(() => expect(screen.queryByRole('menu')).not.toBeInTheDocument());

    await userEvent.pointer([{ keys: '[MouseRight]', target: bodyRegion }]);
    expect(await screen.findByRole('menu')).toBeInTheDocument();
    await userEvent.keyboard('{Escape}');
    await waitFor(() => expect(screen.queryByRole('menu')).not.toBeInTheDocument());

    const keydownAdds = addListener.mock.calls.filter(([type]) => type === 'keydown').length;
    const keydownRemoves = removeListener.mock.calls.filter(([type]) => type === 'keydown').length;
    expect(keydownAdds).toBe(2);
    expect(keydownRemoves).toBe(2);

    addListener.mockRestore();
    removeListener.mockRestore();
  });

  it('does not render any TipTap editor surface (V1.65 editor retired)', async () => {
    useHandlers(chapterDetail('not_started'), bodyHandler());

    renderChapter();
    await screen.findByText('Body prose.');
    // No outline editor textbox, no save/reset buttons, no tabs.
    expect(screen.queryByLabelText('Outline editor')).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /Save Outline/i })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /^Reset$/i })).not.toBeInTheDocument();
    expect(screen.queryByRole('tab')).not.toBeInTheDocument();
  });
});
