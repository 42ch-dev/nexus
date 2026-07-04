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
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
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

function readingProgressHandler(scrollProgress = 0) {
  return http.get('/v1/local/reading/progress', () =>
    HttpResponse.json({
      work_id: 'w-123',
      chapter: 1,
      scroll_progress: scrollProgress,
      updated_at: '2026-07-04T00:00:00Z',
    }),
  );
}

function saveReadingProgressHandler() {
  return http.put('/v1/local/reading/progress', async ({ request }) => {
    const body = (await request.json()) as { work_id: string; chapter: number; scroll_progress: number };
    return HttpResponse.json({
      work_id: body.work_id,
      chapter: body.chapter,
      scroll_progress: body.scroll_progress,
      updated_at: '2026-07-04T00:00:00Z',
    });
  });
}

function annotationsHandler(annotations: unknown[] = []) {
  return http.get('/v1/local/reading/annotations', () =>
    HttpResponse.json({ items: annotations }),
  );
}

function createAnnotationHandler() {
  return http.post('/v1/local/reading/annotations', async ({ request }) => {
    const body = (await request.json()) as Record<string, unknown>;
    return HttpResponse.json({
      annotation_id: 'new-annotation',
      ...body,
      created_at: '2026-07-04T00:00:00Z',
      updated_at: '2026-07-04T00:00:00Z',
    });
  });
}

function updateAnnotationHandler() {
  return http.patch('/v1/local/reading/annotations/:id', async ({ params, request }) => {
    const body = (await request.json()) as Record<string, unknown>;
    return HttpResponse.json({
      annotation_id: params.id,
      ...body,
      updated_at: '2026-07-04T00:00:00Z',
    });
  });
}

function deleteAnnotationHandler() {
  return http.delete('/v1/local/reading/annotations/:id', () => new HttpResponse(null, { status: 204 }));
}

let originalScrollY: PropertyDescriptor | undefined;
let originalScrollHeight: PropertyDescriptor | undefined;
let originalInnerHeight: PropertyDescriptor | undefined;
let originalScrollTo: typeof window.scrollTo;

/** Override scroll metrics so useReadingProgressSync can compute a ratio. */
function setScrollMetrics(metrics: { scrollY: number; scrollHeight: number; innerHeight: number }) {
  Object.defineProperty(window, 'scrollY', { value: metrics.scrollY, writable: true, configurable: true });
  Object.defineProperty(document.documentElement, 'scrollHeight', {
    value: metrics.scrollHeight,
    writable: true,
    configurable: true,
  });
  Object.defineProperty(window, 'innerHeight', { value: metrics.innerHeight, writable: true, configurable: true });
}

function selectTextInProse(startOffset: number, endOffset: number) {
  const prose = screen.getByRole('region', { name: 'Chapter body' });
  const range = document.createRange();
  const walker = document.createTreeWalker(prose, NodeFilter.SHOW_TEXT);
  let offset = 0;
  let startNode: Node | null = null;
  let endNode: Node | null = null;
  let node: Node | null;
  while ((node = walker.nextNode())) {
    const text = node.textContent ?? '';
    const nodeStart = offset;
    const nodeEnd = offset + text.length;
    if (startNode === null && startOffset < nodeEnd) {
      startNode = node;
      range.setStart(node, Math.max(0, startOffset - nodeStart));
    }
    if (endNode === null && endOffset <= nodeEnd) {
      endNode = node;
      range.setEnd(node, Math.max(0, endOffset - nodeStart));
      break;
    }
    offset += text.length;
  }
  // jsdom does not implement Range#getBoundingClientRect; stub it for the
  // toolbar positioning hook.
  range.getBoundingClientRect = () => ({ left: 0, top: 0, width: 0, height: 0, right: 0, bottom: 0, x: 0, y: 0, toJSON: () => {} });
  const selection = window.getSelection();
  selection?.removeAllRanges();
  selection?.addRange(range);
  document.dispatchEvent(new Event('selectionchange'));
}

beforeEach(() => {
  originalScrollTo = window.scrollTo;
  window.scrollTo = vi.fn();
  originalScrollY = Object.getOwnPropertyDescriptor(window, 'scrollY');
  originalScrollHeight = Object.getOwnPropertyDescriptor(document.documentElement, 'scrollHeight');
  originalInnerHeight = Object.getOwnPropertyDescriptor(window, 'innerHeight');
});

afterEach(() => {
  window.scrollTo = originalScrollTo;
  if (originalScrollY) {
    Object.defineProperty(window, 'scrollY', originalScrollY);
  }
  if (originalScrollHeight) {
    Object.defineProperty(document.documentElement, 'scrollHeight', originalScrollHeight);
  }
  if (originalInnerHeight) {
    Object.defineProperty(window, 'innerHeight', originalInnerHeight);
  }
  vi.useRealTimers();
});
function readingHandlers(opts?: { chapter?: number; status?: string; findings?: number; kb?: number; scrollProgress?: number; annotations?: unknown[] }) {
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
    readingProgressHandler(opts?.scrollProgress ?? 0),
    saveReadingProgressHandler(),
    annotationsHandler(opts?.annotations ?? []),
    createAnnotationHandler(),
    updateAnnotationHandler(),
    deleteAnnotationHandler(),
  ];
}

/**
 * Cursor-paginated chapter list fixture for nav-truncation regression tests.
 * The daemon clamps `limit` to `[1, 100]`, so a Work with >100 chapters is
 * served in pages. The opaque cursor is simulated as a page index so the
 * client's cursor-walk (`fetchNextPage`) resolves successive pages.
 */
function paginatedChaptersHandler(totalChapters: number, pageSize: number) {
  return http.get('/v1/local/works/:workId/chapters', ({ request }) => {
    const url = new URL(request.url);
    const cursor = url.searchParams.get('cursor');
    const page = cursor ? Number.parseInt(cursor, 10) || 0 : 0;
    const start = page * pageSize;
    const remaining = totalChapters - start;
    if (remaining <= 0) {
      return HttpResponse.json({ items: [], pagination: { limit: pageSize, has_more: false } });
    }
    const count = Math.min(pageSize, remaining);
    const items = Array.from({ length: count }, (_, i) => {
      const ch = start + i + 1;
      return {
        work_id: 'w-123',
        chapter: ch,
        volume: 1,
        slug: `ch${ch}`,
        planned_word_count: 4000,
        status: 'draft',
        created_at: '2026-06-25T00:00:00Z',
        updated_at: '2026-06-25T00:00:00Z',
      };
    });
    const hasMore = start + pageSize < totalChapters;
    return HttpResponse.json({
      items,
      pagination: {
        limit: pageSize,
        has_more: hasMore,
        ...(hasMore ? { next_cursor: String(page + 1) } : {}),
      },
    });
  });
}

/** Open-findings page that signals truncation (`has_more: true`). */
function truncatedOpenFindingsHandler(chapter: number, count: number) {
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
    return HttpResponse.json({ items, pagination: { limit: 200, has_more: true } });
  });
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
      readingProgressHandler(),
      saveReadingProgressHandler(),
      annotationsHandler(),
      createAnnotationHandler(),
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
      readingProgressHandler(),
      saveReadingProgressHandler(),
      annotationsHandler(),
      createAnnotationHandler(),
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

describe('ChapterPage (V1.79 P0 QC fix-wave — pagination correctness)', () => {
  it('renders an honest "N+" open-findings label when the page is truncated (qc3 W-QC3-002)', async () => {
    useHandlers(
      chapterDetail('draft'),
      bodyHandler(),
      workDetailHandler(),
      chaptersListHandler(),
      // Page reports has_more: true — the count (2) is a lower bound, not exact.
      truncatedOpenFindingsHandler(1, 2),
      worldKbGraphHandler('world-1', 0),
      readingProgressHandler(),
      saveReadingProgressHandler(),
      annotationsHandler(),
      createAnnotationHandler(),
    );

    renderChapter();
    await screen.findByLabelText('Chapter maturation indicators');
    // Truncated count renders "2+" — honest lower bound, not a clipped exact integer.
    expect(await screen.findByLabelText('2+ open findings')).toBeInTheDocument();
  });

  it('resolves prev/next across the server page boundary by cursor-walking (qc3 W-QC3-001)', async () => {
    // 150 chapters served in pages of 100 (daemon cap). Chapter 101 lives on
    // page 2; without the cursor-walk its prev/next would be silently lost and
    // the nav would degrade to "First/Last chapter" placeholders.
    useHandlers(
      chapterDetail('draft'),
      bodyHandler(),
      workDetailHandler(),
      paginatedChaptersHandler(150, 100),
      openFindingsHandler(101, 0),
      worldKbGraphHandler('world-1', 0),
      readingProgressHandler(),
      saveReadingProgressHandler(),
      annotationsHandler(),
      createAnnotationHandler(),
    );

    renderChapter('w-123', 101);
    // After the walk completes, chapter 101 has prev=100 and next=102 — proving
    // the nav no longer loses chapters past the first server page.
    expect(await screen.findByRole('link', { name: /Previous chapter: Chapter 100/i })).toBeInTheDocument();
    expect(screen.getByRole('link', { name: /Next chapter: Chapter 102/i })).toBeInTheDocument();
  });
});

describe('ChapterPage (V1.89 Deeper Manuscript Reading)', () => {
  it('restores persisted scroll progress on load', async () => {
    useHandlers(...readingHandlers({ scrollProgress: 5_000 }));
    setScrollMetrics({ scrollY: 0, scrollHeight: 2_000, innerHeight: 1_000 });

    renderChapter();
    await screen.findByText('Body prose.');

    await waitFor(() => {
      expect(window.scrollTo).toHaveBeenCalledWith(expect.objectContaining({ top: 500 }));
    });
  });

  it('saves scroll progress debounced while reading', async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    const saveRequests: { scroll_progress: number }[] = [];
    useHandlers(
      chapterDetail('draft'),
      bodyHandler(),
      workDetailHandler(),
      chaptersListHandler(),
      openFindingsHandler(1, 0),
      worldKbGraphHandler('world-1', 0),
      readingProgressHandler(0),
      http.put('/v1/local/reading/progress', async ({ request }) => {
        const body = (await request.json()) as { scroll_progress: number };
        saveRequests.push(body);
        return HttpResponse.json({ work_id: 'w-123', chapter: 1, scroll_progress: body.scroll_progress, updated_at: '2026-07-04T00:00:00Z' });
      }),
      annotationsHandler(),
      createAnnotationHandler(),
    );
    setScrollMetrics({ scrollY: 0, scrollHeight: 2_000, innerHeight: 1_000 });

    renderChapter();
    await screen.findByText('Body prose.');

    // Simulate scroll to 50%.
    setScrollMetrics({ scrollY: 500, scrollHeight: 2_000, innerHeight: 1_000 });
    window.dispatchEvent(new Event('scroll'));
    setScrollMetrics({ scrollY: 600, scrollHeight: 2_000, innerHeight: 1_000 });
    window.dispatchEvent(new Event('scroll'));

    vi.advanceTimersByTime(600);

    await waitFor(() => {
      expect(saveRequests.length).toBeGreaterThanOrEqual(1);
    });
    const last = saveRequests.at(-1);
    expect(last?.scroll_progress).toBeGreaterThanOrEqual(5_000);
  });

  it('creates a highlight from text selection', async () => {
    const requests: Record<string, unknown>[] = [];
    useHandlers(
      chapterDetail('draft'),
      bodyHandler('Body prose text for selection.'),
      workDetailHandler(),
      chaptersListHandler(),
      openFindingsHandler(1, 0),
      worldKbGraphHandler('world-1', 0),
      readingProgressHandler(),
      saveReadingProgressHandler(),
      annotationsHandler(),
      http.post('/v1/local/reading/annotations', async ({ request }) => {
        const body = (await request.json()) as Record<string, unknown>;
        requests.push(body);
        return HttpResponse.json({ annotation_id: 'a-new', ...body, created_at: '2026-07-04T00:00:00Z', updated_at: '2026-07-04T00:00:00Z' });
      }),
      updateAnnotationHandler(),
      deleteAnnotationHandler(),
    );

    renderChapter();
    await screen.findByText('Body prose text for selection.');

    selectTextInProse(5, 9);
    const toolbar = await screen.findByRole('toolbar', { name: /annotation actions/i });
    const highlightButton = screen.getByRole('button', { name: /highlight selection/i });
    await userEvent.click(highlightButton);

    await waitFor(() => {
      expect(requests).toHaveLength(1);
    });
    expect(requests[0]).toMatchObject({
      work_id: 'w-123',
      chapter: 1,
      start_offset: 5,
      end_offset: 9,
      selected_text: 'pros',
      color: 'yellow',
    });
    expect(toolbar).not.toBeVisible();
  });

  it('renders a drift notice when persisted annotations are out of bounds', async () => {
    useHandlers(
      chapterDetail('draft'),
      bodyHandler('Short.'),
      workDetailHandler(),
      chaptersListHandler(),
      openFindingsHandler(1, 0),
      worldKbGraphHandler('world-1', 0),
      readingProgressHandler(),
      saveReadingProgressHandler(),
      annotationsHandler([
        {
          annotation_id: 'a-1',
          work_id: 'w-123',
          chapter: 1,
          start_offset: 100,
          end_offset: 200,
          selected_text: 'gone',
          color: 'yellow',
          created_at: '2026-07-04T00:00:00Z',
          updated_at: '2026-07-04T00:00:00Z',
        },
      ]),
      createAnnotationHandler(),
      updateAnnotationHandler(),
      deleteAnnotationHandler(),
    );

    renderChapter();
    await screen.findByText('Short.');
    expect(await screen.findByRole('note')).toHaveTextContent(/may have shifted/i);
  });

  it('lists annotations in the inspector and supports delete', async () => {
    const user = userEvent.setup();
    let deletedId: string | null = null;
    useHandlers(
      chapterDetail('draft'),
      bodyHandler('Body prose.'),
      workDetailHandler(),
      chaptersListHandler(),
      openFindingsHandler(1, 0),
      worldKbGraphHandler('world-1', 0),
      readingProgressHandler(),
      saveReadingProgressHandler(),
      annotationsHandler([
        {
          annotation_id: 'a-1',
          work_id: 'w-123',
          chapter: 1,
          start_offset: 0,
          end_offset: 4,
          selected_text: 'Body',
          note: 'important',
          color: 'yellow',
          created_at: '2026-07-04T00:00:00Z',
          updated_at: '2026-07-04T00:00:00Z',
        },
      ]),
      createAnnotationHandler(),
      updateAnnotationHandler(),
      http.delete('/v1/local/reading/annotations/:id', ({ params }) => {
        deletedId = params.id as string;
        return new HttpResponse(null, { status: 204 });
      }),
    );

    renderChapter();
    await screen.findByText('Body prose.');
    expect(await screen.findByText('important')).toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: /delete highlight/i }));
    await waitFor(() => {
      expect(deletedId).toBe('a-1');
    });
  });

  it('preserves the body-ownership invariant — no body/outline write affordances on the reading surface', async () => {
    useHandlers(...readingHandlers());

    renderChapter();
    await screen.findByText('Body prose.');
    expect(screen.getByRole('link', { name: /Edit outline for Chapter 1/i })).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /Save body/i })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /Edit body/i })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /Save outline/i })).not.toBeInTheDocument();
  });
});
