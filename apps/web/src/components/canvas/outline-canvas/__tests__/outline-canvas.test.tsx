/**
 * Outline canvas orchestrator — conflict modal trigger on stale revision
 * (FB-C1-003) and panel selection path regression (V1.108 P0 T2).
 *
 * CanvasShell (React Flow) is mocked out so jsdom never needs ResizeObserver;
 * the test focuses on the orchestrator's 409 conflict wiring and the panel
 * → inspector selection path that must remain functional alongside the new
 * graph-click selection sync.
 */
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { act } from 'react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';

import { OutlineCanvas } from '@/components/canvas/outline-canvas';
import { NexusClientError } from '@/lib/nexus/errors';

// ---------------------------------------------------------------------------
// Mocks
// ---------------------------------------------------------------------------

// V1.109 P2 T3 (FB-GS-002) — CanvasShell is now backed by the REAL React Flow
// integration harness instead of a div stub. The harness mounts a genuine
// `<ReactFlowProvider>` + `<ReactFlow>` consuming the same nodes/edges/
// onNodesChange props the orchestrator passes to CanvasShell, so graph-click →
// inspector selection flows through real RF state. The ResizeObserver polyfill
// in `src/test/setup.ts` covers jsdom mounting (same path `outline-page.test`
// relies on). `testUseNodeChangeHandler` mirrors the real CanvasShell helper so
// the hook's `onNodesChange` actually applies RF selection changes.
//
// I-QC1-001 — the harness renders children so the in-shell EmptyState overlay
// test still asserts its presence inside the shell.
vi.mock('@/components/canvas/canvas-shell', async () => {
  const harness = await import('@/components/canvas/__tests__/rf-integration-harness');
  return {
    CanvasShell: harness.RFIntegrationHarness,
    useNodeChangeHandler: harness.testUseNodeChangeHandler,
  };
});

const mocks = vi.hoisted(() => {
  const WORK = {
    work_id: 'wk_test',
    title: 'Test Work',
    work_profile: 'novel',
    created_at: '',
    updated_at: '',
  };
  const CHAPTER_1 = {
    work_id: 'wk_test',
    chapter: 1,
    volume: 1,
    title: 'Chapter One',
    slug: 'ch-1',
    status: 'draft',
    planned_word_count: 1000,
    actual_word_count: 500,
    outline_path: undefined,
    body_path: undefined,
    created_at: '',
    updated_at: '',
  };
  const OUTLINE = {
    work_id: 'wk_test',
    outline_revision: 2,
    volumes: [{ volume_id: 1, label: 'Volume 1', chapter_ids: [1] }],
    timeline_events: [],
    foreshadows: [],
    chapter_titles: {},
    updated_at: '',
  };
  return {
    WORK,
    CHAPTER_1,
    OUTLINE,
    outlineResult: {
      data: OUTLINE,
      isLoading: false,
      isError: false,
      isFetching: false,
      refetch: vi.fn().mockResolvedValue({ data: OUTLINE }),
      dataUpdatedAt: 0,
    },
    chaptersResult: {
      data: { pages: [{ items: [CHAPTER_1], pagination: { has_more: false, next_cursor: null } }] },
      isLoading: false,
      isError: false,
      isFetching: false,
      hasNextPage: false,
      isFetchingNextPage: false,
      fetchNextPage: vi.fn(),
      refetch: vi.fn(),
      dataUpdatedAt: 0,
    },
    workResult: {
      data: WORK,
      isLoading: false,
      isError: false,
      isFetching: false,
      refetch: vi.fn(),
      dataUpdatedAt: 0,
    },
    patchStructureResult: { mutate: vi.fn(), isPending: false },
    patchChapterResult: { mutate: vi.fn(), isPending: false },
    patchTimelineResult: { mutate: vi.fn(), isPending: false },
  };
});

vi.mock('@/api/queries', () => ({
  useWork: () => mocks.workResult,
  useChapters: () => mocks.chaptersResult,
  useChapterOutline: () => ({
    data: undefined,
    isLoading: false,
    isError: false,
    isFetching: false,
    refetch: vi.fn(),
    dataUpdatedAt: 0,
  }),
  flattenPages: (data: { pages: { items: unknown[] }[] } | undefined): unknown[] => {
    if (!data) return [];
    return data.pages.flatMap((p) => p.items);
  },
}));

vi.mock('@/lib/canvas/use-outline-data', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@/lib/canvas/use-outline-data')>();
  return {
    ...actual,
    useWorkOutline: () => mocks.outlineResult,
    usePatchOutlineStructure: () => mocks.patchStructureResult,
    usePatchOutlineChapter: () => mocks.patchChapterResult,
    usePatchTimelineEvent: () => mocks.patchTimelineResult,
  };
});

vi.mock('@/lib/nexus/query-keys', () => ({
  queryKeys: {
    chapters: {
      outlines: () => ['chapters', 'outlines'],
      detail: () => ['chapters', 'detail'],
      lists: () => ['chapters', 'lists'],
      list: () => ['chapters', 'list'],
    },
    outline: {
      detail: () => ['outline', 'detail'],
    },
  },
}));

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

const queryClient = new QueryClient({
  defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
});

function renderOutline() {
  return render(
    <QueryClientProvider client={queryClient}>
      <OutlineCanvas workId="wk_test" />
    </QueryClientProvider>,
  );
}

/**
 * Scope to the Outline structure panel Card. V1.109 P2 T3 — now that real RF
 * mounts, the chapter title appears in BOTH the graph node and the structure
 * panel row, so unscoped `getByText('Chapter One')` is ambiguous. The panel
 * Card is anchored by its unique "Volumes & Chapters" CardTitle.
 */
function structurePanel(): HTMLElement {
  return (
    screen.getByText('Volumes & Chapters').closest('[class*="card"]') ?? document.body
  );
}

/** Build a real NexusClientError 409 carrying `current_version`. */
function outlineConflictErr(currentVersion: number): NexusClientError {
  return new NexusClientError(409, 'outline_conflict', 'stale revision', {
    current_version: currentVersion,
    conflicting_path: 'volumes/1',
  });
}

/** Invoke the latest captured chapter mutate call's onError callback. */
async function rejectLastChapterAsConflict(currentVersion: number) {
  const chapterMutate = mocks.patchChapterResult.mutate;
  const lastCall = chapterMutate.mock.calls.at(-1);
  if (!lastCall) throw new Error('no patchChapter.mutate call captured');
  const opts = lastCall[1] as { onError?: (e: unknown) => void };
  await act(async () => {
    opts.onError?.(outlineConflictErr(currentVersion));
  });
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe('OutlineCanvas — conflict modal trigger (FB-C1-003)', () => {
  it('renders the outline graph shell and structure panel', () => {
    renderOutline();
    expect(screen.getByTestId('rf-integration-harness')).toBeInTheDocument();
    expect(screen.getByText('Volumes & Chapters')).toBeInTheDocument();
  });

  it('shows the outline conflict modal when a chapter patch returns 409', async () => {
    const user = userEvent.setup();
    renderOutline();

    // 1. Select the chapter via the structure panel (panel selection path).
    //    Scoped to the panel because real RF also renders the chapter title in
    //    the graph node (FB-GS-002).
    await user.click(within(structurePanel()).getByText('Chapter One'));

    // 2. Edit the title field to make the save button actionable.
    const titleInput = screen.getByDisplayValue('Chapter One');
    await user.clear(titleInput);
    await user.type(titleInput, 'Revised Chapter One');

    // 3. Save chapter → mutate fires → simulate 409 outline_conflict.
    await user.click(screen.getByRole('button', { name: /Save chapter/i }));
    await rejectLastChapterAsConflict(5);

    // 4. The outline-flavored conflict modal must be visible with the new
    //    server revision (FB-C1-003 acceptance: stale revision → conflict
    //    modal appears with retry/merge path).
    expect(
      screen.getByRole('heading', { name: 'Outline Conflict' }),
    ).toBeInTheDocument();
    expect(
      screen.getByText('5', { selector: 'span.font-mono' }),
    ).toBeInTheDocument();
  });

  it('lists the chapter title in the local changed fields on 409', async () => {
    const user = userEvent.setup();
    renderOutline();

    await user.click(within(structurePanel()).getByText('Chapter One'));
    const titleInput = screen.getByDisplayValue('Chapter One');
    await user.clear(titleInput);
    await user.type(titleInput, 'New Title');
    await user.click(screen.getByRole('button', { name: /Save chapter/i }));
    await rejectLastChapterAsConflict(5);

    const draftSection = screen.getByText('What you were about to do').closest('div')!;
    expect(draftSection.textContent).toContain('Chapter title');
  });
});

describe('OutlineCanvas — panel selection path regression', () => {
  it('selecting a chapter in the panel updates the chapter inspector', async () => {
    const user = userEvent.setup();
    renderOutline();

    // Before selection, the inspector shows the empty-state message.
    expect(screen.getByText('Select a chapter to inspect its outline metadata.')).toBeInTheDocument();

    // Click the chapter in the structure panel. Scoped to the panel because
    // real RF also renders the chapter title in the graph node (FB-GS-002).
    await user.click(within(structurePanel()).getByText('Chapter One'));

    // The inspector should now show the Chapter Inspector with the chapter number.
    const inspector = screen.getByText('Chapter Inspector').closest('[class*="card"]') ?? document.body;
    expect(within(inspector as HTMLElement).getByText(/#1/)).toBeInTheDocument();
  });
});

describe('OutlineCanvas — graph↔list alt toggle (FB-C1-004)', () => {
  it('defaults to graph view (CanvasShell mounted, alt toggle not pressed)', () => {
    renderOutline();
    expect(screen.getByTestId('rf-integration-harness')).toBeInTheDocument();
    const toggle = screen.getByRole('button', { name: 'Show list view' });
    expect(toggle).toHaveAttribute('aria-pressed', 'false');
  });

  it('switches to alt list view on toggle click and back to graph', async () => {
    const user = userEvent.setup();
    renderOutline();

    // Click "Show list view" → alt view appears, graph mock disappears.
    await user.click(screen.getByRole('button', { name: 'Show list view' }));
    expect(screen.queryByTestId('rf-integration-harness')).not.toBeInTheDocument();
    expect(screen.getByText('Chapters')).toBeInTheDocument();
    expect(screen.getByText('Timeline Events')).toBeInTheDocument();

    // The toggle label flips and aria-pressed is true.
    const graphToggle = screen.getByRole('button', { name: 'Show graph' });
    expect(graphToggle).toHaveAttribute('aria-pressed', 'true');

    // Click "Show graph" → back to graph view.
    await user.click(graphToggle);
    expect(screen.getByTestId('rf-integration-harness')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Show list view' })).toHaveAttribute(
      'aria-pressed',
      'false',
    );
  });

  it('renders chapter list with status in alt view', async () => {
    const user = userEvent.setup();
    renderOutline();

    await user.click(screen.getByRole('button', { name: 'Show list view' }));

    // The alt view section renders the chapter title and status badge.
    // Scope to the alt section to avoid collision with the structure panel below.
    const altSection = screen.getByLabelText('Outline chapters and timeline in list order');
    expect(within(altSection).getByText('Chapter One')).toBeInTheDocument();
    expect(within(altSection).getByText('Draft')).toBeInTheDocument();
  });
});

// I-QC1-001 — CanvasShell must always mount for the graph view, even when the
// projection produces zero nodes. The EmptyState renders as an in-shell overlay.
describe('OutlineCanvas — empty graph shell parity (I-QC1-001)', () => {
  it('mounts CanvasShell with in-shell EmptyState when projection has zero nodes', () => {
    // Override the outline to have no volumes/events → projection.nodes.length === 0.
    mocks.outlineResult.data = {
      ...mocks.OUTLINE,
      volumes: [],
      timeline_events: [],
    };
    // Also clear chapters so no unassigned-chapter nodes are produced.
    mocks.chaptersResult.data = {
      pages: [{ items: [], pagination: { has_more: false, next_cursor: null } }],
    };
    renderOutline();

    // CanvasShell must be mounted (shared-shell parity FB-C1-000).
    expect(screen.getByTestId('rf-integration-harness')).toBeInTheDocument();
    // The in-shell EmptyState overlay must be visible inside the shell.
    expect(screen.getByText('No graph nodes')).toBeInTheDocument();
  });
});

// ---------------------------------------------------------------------------
// V1.109 C2 T4 — Scene/Beat integration (FB-C2-000/002/003/004)
// ---------------------------------------------------------------------------

/** Scene/Beat fixture payload with a full Volume/Chapter/Scene/Beat hierarchy. */
const SCENE_BEAT_FIXTURE = {
  scenes: [
    { sceneId: 'scene-1', chapterId: 1, title: 'Opening Scene', status: 'drafted' as const },
    { sceneId: 'scene-2', chapterId: 1, title: null, status: 'completed' as const },
  ],
  beats: [
    { beatId: 'beat-1', sceneId: 'scene-1', title: 'Inciting Moment', status: null },
  ],
};

function renderOutlineWithFixture(fixture: typeof SCENE_BEAT_FIXTURE) {
  return render(
    <QueryClientProvider client={queryClient}>
      <OutlineCanvas workId="wk_test" sceneBeatFixture={fixture} />
    </QueryClientProvider>,
  );
}

describe('OutlineCanvas — Scene/Beat alt view integration (FB-C2-000/003)', () => {
  beforeEach(() => {
    // Restore default mock data — the empty-graph test above mutates the
    // shared mocks. Each Scene/Beat test needs the default Volume/Chapter
    // structure to render the hierarchy.
    mocks.outlineResult.data = mocks.OUTLINE;
    mocks.chaptersResult.data = {
      pages: [{ items: [mocks.CHAPTER_1], pagination: { has_more: false, next_cursor: null } }],
    };
  });

  it('renders Scene/Beat rows nested under chapters in alt view when fixture provided', async () => {
    const user = userEvent.setup();
    renderOutlineWithFixture(SCENE_BEAT_FIXTURE);

    // Switch to list view.
    await user.click(screen.getByRole('button', { name: 'Show list view' }));

    const altSection = screen.getByLabelText('Outline chapters and timeline in list order');

    // Scene rows appear with type badges and titles.
    expect(within(altSection).getByText('Opening Scene')).toBeInTheDocument();
    expect(within(altSection).getAllByText('Scene')).toHaveLength(2);

    // Beat row nests under its scene.
    expect(within(altSection).getByText('Inciting Moment')).toBeInTheDocument();
    expect(within(altSection).getByText('Beat')).toBeInTheDocument();

    // Null-title scene falls back to Untitled Scene (Voice & Content lock).
    expect(within(altSection).getByText('Untitled Scene')).toBeInTheDocument();
  });

  it('shows the empty-under-chapter helper for chapters with zero scenes when fixture is active', async () => {
    const user = userEvent.setup();
    // Add a second chapter to the outline + chapters data so it has zero scenes.
    mocks.outlineResult.data = {
      ...mocks.OUTLINE,
      volumes: [
        { volume_id: 1, label: 'Volume 1', chapter_ids: [1, 2] },
      ],
    };
    mocks.chaptersResult.data = {
      pages: [
        {
          items: [
            mocks.CHAPTER_1,
            { ...mocks.CHAPTER_1, chapter: 2, title: 'Chapter Two' },
          ],
          pagination: { has_more: false, next_cursor: null },
        },
      ],
    };

    renderOutlineWithFixture(SCENE_BEAT_FIXTURE);
    await user.click(screen.getByRole('button', { name: 'Show list view' }));

    // Chapter 2 has no scenes in the fixture → the empty helper shows.
    expect(screen.getByText('No scenes in this chapter yet.')).toBeInTheDocument();
  });

  it('does NOT render Scene/Beat rows or empty-under-chapter helper when no fixture (honest empty chrome)', async () => {
    const user = userEvent.setup();
    renderOutline(); // No fixture prop — real Work behavior.

    await user.click(screen.getByRole('button', { name: 'Show list view' }));

    // No Scene/Beat chrome at all.
    expect(screen.queryByText('Scene')).not.toBeInTheDocument();
    expect(screen.queryByText('Beat')).not.toBeInTheDocument();
    expect(screen.queryByText('No scenes in this chapter yet.')).not.toBeInTheDocument();
  });
});

describe('OutlineCanvas — Scene/Beat inspector mounting (FB-C2-002)', () => {
  beforeEach(() => {
    mocks.outlineResult.data = mocks.OUTLINE;
    mocks.chaptersResult.data = {
      pages: [{ items: [mocks.CHAPTER_1], pagination: { has_more: false, next_cursor: null } }],
    };
  });

  it('shows the Chapter inspector by default (no Scene/Beat selection)', () => {
    renderOutlineWithFixture(SCENE_BEAT_FIXTURE);

    // Chapter inspector is the default even with a fixture — its empty-state
    // prompt shows when no chapter is selected. Scene/Beat inspectors only
    // appear when those nodes are selected via graph click.
    expect(screen.getByText('Select a chapter to inspect its outline metadata.')).toBeInTheDocument();
    // Scene/Beat inspector headings do NOT appear (those inspectors are not
    // mounted when selectedScene/selectedBeat are null).
    expect(screen.queryByText('Scene')).not.toBeInTheDocument();
    expect(screen.queryByText('Beat')).not.toBeInTheDocument();
  });
});

// ---------------------------------------------------------------------------
// V1.109 P2 T3 — Real React Flow integration (FB-GS-002)
// ---------------------------------------------------------------------------
//
// Before this change the file stubbed CanvasShell with a div so React Flow
// never mounted in jsdom. That left the graph-click → inspector selection path
// (the very wiring `useOutlineCanvasGraph` exists to provide) uncovered by a
// real RF tree: a regression that broke RF node selection silently would pass
// every mock-based test. The file-level mock factory now backs CanvasShell with
// the real-RF integration harness (`rf-integration-harness.tsx`), so every test
// in this file — including these — mounts a genuine `<ReactFlowProvider>` +
// `<ReactFlow>` tree, renders real node components, and flows a real graph
// click through RF's `onNodesChange` → the hook's selection-sync effect → the
// inspector. These two tests pin the integration contract explicitly.
describe('OutlineCanvas — real RF graph-click selection (FB-GS-002)', () => {
  beforeEach(() => {
    mocks.outlineResult.data = mocks.OUTLINE;
    mocks.chaptersResult.data = {
      pages: [{ items: [mocks.CHAPTER_1], pagination: { has_more: false, next_cursor: null } }],
    };
  });

  it('renders real React Flow nodes from the projection (no mock stub)', () => {
    renderOutline();

    // The real-RF harness region mounts (replaces the div stub).
    const harness = screen.getByTestId('rf-integration-harness');
    // A real RF chapter node is rendered inside the harness — the projection
    // (rfNodes) is consumed by a genuine <ReactFlow>. The chapter title appears
    // inside the graph node, scoped to the harness so it does not collide with
    // the structure-panel row.
    expect(within(harness).getByText('Chapter One')).toBeInTheDocument();
    // RF wraps each node in a `.react-flow__node` element carrying the node id —
    // proof a real RF tree (not a stub) rendered the projection.
    expect(harness.querySelector('.react-flow__node[data-id="chapter:1"]')).not.toBeNull();
  });

  it('clicking a chapter node in the RF graph drives the chapter inspector', async () => {
    const user = userEvent.setup();
    renderOutline();

    // Before selection, the inspector shows the empty-state prompt.
    expect(
      screen.getByText('Select a chapter to inspect its outline metadata.'),
    ).toBeInTheDocument();

    const harness = screen.getByTestId('rf-integration-harness');
    // Click the chapter title rendered INSIDE the real RF graph node (scoped to
    // the harness so this targets the graph node, not the structure-panel row).
    await user.click(within(harness).getByText('Chapter One'));

    // Real RF selection flows: node `selected` → onNodesChange → hook
    // selection-sync → setSelectedChapterId → Chapter inspector mounts with #1.
    const inspector =
      screen.getByText('Chapter Inspector').closest('[class*="card"]') ?? document.body;
    expect(within(inspector as HTMLElement).getByText(/#1/)).toBeInTheDocument();
  });
});
