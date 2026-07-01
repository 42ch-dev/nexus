/**
 * ChapterPage render tests — V1.79 Author Reflection (Track A / P0).
 *
 * Covers the promoted reading surface: the V1.75-pivot residuals (canvas
 * redirect CTA, body read-only render + frontmatter strip, Copy Path, body
 * right-click context menu) are preserved verbatim, and the V1.79 additions
 * (chapter/volume navigation, session-only reading progress, in-context
 * maturation indicators) render from existing read-only data. No write route
 * is exercised or asserted.
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

/** Work detail carrying a world_id so the KB density hook resolves a World. */
function workDetailHandler(worldId = 'world-1') {
  return http.get('/v1/local/works/:workId', ({ params }) =>
    HttpResponse.json({
      work_id: params.workId,
      status: 'active',
      title: 'Galaxy Novel',
      long_term_goal: '',
      initial_idea: '',
      intake_status: 'completed',
      world_id: worldId,
      inspiration_log: [],
      primary_preset_id: 'preset-1',
      schedule_ids: [],
      created_at: '2026-06-01T00:00:00Z',
      updated_at: '2026-06-25T00:00:00Z',
      current_stage: 'drafting',
      stage_status: 'active',
      current_chapter: 1,
      auto_chain_enabled: false,
      auto_chain_interrupted: false,
      auto_review_master_on_timeout: false,
    }),
  );
}

/** Chapter list for neighbor resolution (3 chapters → prev/next on chapter 2). */
function chaptersListHandler() {
  return http.get('/v1/local/works/:workId/chapters', () =>
    HttpResponse.json({
      items: [
        { work_id: 'w-123', chapter: 1, volume: 1, slug: 'ch01', planned_word_count: 4000, status: 'draft', created_at: '2026-06-25T00:00:00Z', updated_at: '2026-06-25T00:00:00Z' },
        { work_id: 'w-123', chapter: 2, volume: 1, slug: 'ch02', planned_word_count: 4000, status: 'outlined', created_at: '2026-06-25T00:00:00Z', updated_at: '2026-06-25T00:00:00Z' },
        { work_id: 'w-123', chapter: 3, volume: 1, slug: 'ch03', planned_word_count: 4000, status: 'not_started', created_at: '2026-06-25T00:00:00Z', updated_at: '2026-06-25T00:00:00Z' },
      ],
      pagination: { limit: 200, has_more: false },
    }),
  );
}

/** Open (non-terminal) findings for a chapter — 2 rows for the count assertion. */
function openFindingsHandler(chapter = 1, count = 2) {
  return http.get('/v1/local/works/:workId/findings', ({ request }) => {
    const url = new URL(request.url);
    if (url.searchParams.get('chapter') !== String(chapter)) {
      return HttpResponse.json({ items: [], pagination: { limit: 200, has_more: false } });
    }
    const items = Array.from({ length: count }, (_, i) => ({
      finding_id: `f-${i}`,
      work_id: 'w-123',
      chapter,
      severity: 'medium',
      status: 'open',
      title: `Finding ${i}`,
      description: 'desc',
      target_executor: 'writer',
      kind: 'consistency',
      created_at: 0,
      updated_at: 0,
    }));
    return HttpResponse.json({ items, pagination: { limit: 200, has_more: false } });
  });
}

/** World KB graph with N entities for the density count assertion. */
function worldKbGraphHandler(worldId = 'world-1', entityCount = 5) {
  return http.get('/v1/local/worlds/:worldId/kb/graph', ({ params }) => {
    if (params.worldId !== worldId) {
      return HttpResponse.json({ entities: [], source_anchors: [], relationships: [] });
    }
    const entities = Array.from({ length: entityCount }, (_, i) => ({
      key_block_id: `kb-${i}`,
      world_id: worldId,
      block_type: 'entity',
      canonical_name: `Entity ${i}`,
      status: 'confirmed',
      version: 1,
    }));
    return HttpResponse.json({ entities, source_anchors: [], relationships: [] });
  });
}

/** Full reading-surface handler stack so no data hook logs an unhandled request. */
function readingHandlers(opts?: { chapter?: number; status?: string; findings?: number; kb?: number }) {
  const chapter = opts?.chapter ?? 1;
  const status = opts?.status ?? 'draft';
  const findings = opts?.findings ?? 0;
  const kb = opts?.kb ?? 0;
  return [
    chapterDetail(status),
    bodyHandler(),
    workDetailHandler(),
    chaptersListHandler(),
    openFindingsHandler(chapter, findings),
    worldKbGraphHandler('world-1', kb),
  ];
}

describe('ChapterPage (V1.75 residuals preserved)', () => {
  it('renders the canvas redirect CTA pointing at the outline canvas with the chapter preselect', async () => {
    useHandlers(...readingHandlers({ status: 'not_started' }));

    renderChapter();
    const cta = await screen.findByRole('link', {
      name: /Edit outline for Chapter 1 on the outline canvas/i,
    });
    expect(cta).toHaveAttribute('href', '/works/w-123/outline?chapter=1');
  });

  it('renders the chapter header (number + back link)', async () => {
    useHandlers(...readingHandlers());

    renderChapter();
    expect(await screen.findByText('Chapter 1')).toBeInTheDocument();
    expect(screen.getByRole('link', { name: /Back to Chapters/i })).toBeInTheDocument();
  });

  it('renders the body read-only and strips frontmatter', async () => {
    useHandlers(
      chapterDetail('draft'),
      bodyHandler('---\nstatus: draft\n---\n\nBody prose.', { status: 'draft' }),
      workDetailHandler(),
      chaptersListHandler(),
      openFindingsHandler(1, 0),
      worldKbGraphHandler('world-1', 0),
    );

    renderChapter();
    expect(await screen.findByText('Body prose.')).toBeInTheDocument();
    expect(screen.queryByText('---')).not.toBeInTheDocument();
    expect(screen.getByText(/Works\/WRK\/Stories\/ch01-ch01\.md/)).toBeInTheDocument();
  });

  it('copies the body path via the Copy Path button', async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.assign(navigator, { clipboard: { writeText } });

    useHandlers(...readingHandlers());

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
      workDetailHandler(),
      chaptersListHandler(),
      openFindingsHandler(1, 0),
      worldKbGraphHandler('world-1', 0),
    );

    renderChapter();
    expect(await screen.findByText(/Could not load the chapter body/i)).toBeInTheDocument();
  });

  it('opens the context menu on right-click and copies the path', async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.assign(navigator, { clipboard: { writeText } });

    useHandlers(...readingHandlers());

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

    useHandlers(...readingHandlers());

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
    useHandlers(...readingHandlers({ status: 'not_started' }));

    renderChapter();
    await screen.findByText('Body prose.');
    expect(screen.queryByLabelText('Outline editor')).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /Save Outline/i })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /^Reset$/i })).not.toBeInTheDocument();
    expect(screen.queryByRole('tab')).not.toBeInTheDocument();
  });
});

describe('ChapterPage (V1.79 reading surface)', () => {
  it('renders the session-only reading progress indicator', async () => {
    useHandlers(...readingHandlers());

    renderChapter();
    expect(await screen.findByRole('progressbar', { name: /Reading progress/i })).toBeInTheDocument();
  });

  it('renders prev/next chapter navigation derived from the chapter list', async () => {
    useHandlers(...readingHandlers({ chapter: 2 }));

    renderChapter('w-123', 2);
    // Chapter 2 has prev=1 and next=3 in the 3-chapter fixture.
    expect(await screen.findByRole('link', { name: /Previous chapter: Chapter 1/i })).toBeInTheDocument();
    expect(screen.getByRole('link', { name: /Next chapter: Chapter 3/i })).toBeInTheDocument();
  });

  it('renders the first-chapter placeholder when there is no previous chapter', async () => {
    useHandlers(...readingHandlers({ chapter: 1 }));

    renderChapter('w-123', 1);
    expect(await screen.findByText('First chapter')).toBeInTheDocument();
  });

  it('renders the maturation indicators (KB density + open findings counts) from existing data', async () => {
    useHandlers(...readingHandlers({ findings: 2, kb: 5 }));

    renderChapter();
    await screen.findByLabelText('Chapter maturation indicators');
    // KB density count renders the entity count; open-findings renders the
    // non-terminal finding count. Both resolve async (counts load after the
    // container) so use async queries. Interpretable without tooltips.
    expect(await screen.findByLabelText('5 key blocks')).toBeInTheDocument();
    expect(await screen.findByLabelText('2 open findings')).toBeInTheDocument();
  });

  it('renders a quiet zero-state for open findings when none are non-terminal', async () => {
    useHandlers(...readingHandlers({ findings: 0, kb: 0 }));

    renderChapter();
    await screen.findByLabelText('Chapter maturation indicators');
    expect(await screen.findByLabelText('0 open findings')).toBeInTheDocument();
  });

  it('does not offer any write affordance — only the canvas redirect (body-ownership invariant)', async () => {
    useHandlers(...readingHandlers());

    renderChapter();
    await screen.findByText('Body prose.');
    // The only edit affordance is the canvas redirect; no body-editor / save /
    // patch affordance exists on the reading surface.
    expect(screen.getByRole('link', { name: /Edit outline for Chapter 1/i })).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /Save body/i })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /Edit body/i })).not.toBeInTheDocument();
  });
});
