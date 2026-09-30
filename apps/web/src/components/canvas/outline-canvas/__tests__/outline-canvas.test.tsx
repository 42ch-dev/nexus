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
import { i18n } from '@/lib/i18n/config';
import { NexusClientError } from '@/lib/nexus/errors';
import type * as WorldKbData from '@/lib/canvas/use-world-kb-data';
import type { WorkOutline } from '@42ch/nexus-contracts';

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
  const OUTLINE: WorkOutline = {
    work_id: 'wk_test',
    outline_revision: 2,
    volumes: [{ volume_id: 1, label: 'Volume 1', chapter_ids: [1] }],
    timeline_events: [],
    foreshadows: [],
    scenes: [],
    beats: [],
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

// The event inspector composes its bound-World KB graph read on every render
// (the query itself is gated on the bound World id). This file renders the
// orchestrator under a bare QueryClientProvider — no ClientProvider — and the
// Work it mocks is unbound, so the read is stubbed to its disabled shape.
vi.mock('@/lib/canvas/use-world-kb-data', async (importOriginal) => {
  const actual = await importOriginal<typeof WorldKbData>();
  return {
    ...actual,
    useWorldKbGraph: () => ({ data: undefined, isLoading: false, isError: false }),
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

/**
 * Build a real NexusClientError 409 mirroring the daemon's OutlineConflict
 * envelope (core_error.rs: details carry `current_revision`). The rendered
 * revision in the modal is falsifiable because the cached fixture outline
 * carries `outline_revision: 2` while tests inject 5: a modal showing 5
 * proves the envelope won; 2 would mean the cache fallback was taken.
 */
function outlineConflictErr(currentRevision: number): NexusClientError {
  return new NexusClientError(409, 'outline_conflict', 'stale revision', {
    current_revision: currentRevision,
    conflicting_path: 'volumes/1',
  });
}

/** Invoke the latest captured chapter mutate call's onError callback. */
async function rejectLastChapterAsConflict(currentRevision: number) {
  const chapterMutate = mocks.patchChapterResult.mutate;
  const lastCall = chapterMutate.mock.calls.at(-1);
  if (!lastCall) throw new Error('no patchChapter.mutate call captured');
  const opts = lastCall[1] as { onError?: (e: unknown) => void };
  await act(async () => {
    opts.onError?.(outlineConflictErr(currentRevision));
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
    await user.click(screen.getByRole('button', { name: /^Save$/i }));
    await rejectLastChapterAsConflict(5);

    // 4. The outline-flavored conflict modal must be visible with the SERVER
    //    revision (FB-C1-003 acceptance: stale revision → conflict modal
    //    appears with retry/merge path). The envelope carries
    //    `current_revision: 5` while the cached outline revision is 2, so
    //    this passes only when the envelope field feeds the modal.
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
    await user.click(screen.getByRole('button', { name: /^Save$/i }));
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

// ---------------------------------------------------------------------------
// V1.200 DR-26 Task 3 — Scene/Beat authoring controls
//
// The Outline canvas is the single scene/beat authoring surface (both Timeline
// Moment layers are read-only projections). Every control routes through the
// orchestrator's `handleStructure` — the SAME `outline.patch_structure`
// mutation + conflict capture the structure/timeline patches use — so a stale
// revision opens the existing conflict modal instead of a new error path.
// ---------------------------------------------------------------------------

const AUTHORING_OUTLINE: WorkOutline = {
  ...mocks.OUTLINE,
  scenes: [{ scene_id: 'scn_1', chapter_id: 1, title: 'Opening', status: 'drafted' }],
  beats: [{ beat_id: 'bet_1', scene_id: 'scn_1', title: 'Hook', status: 'drafted' }],
};

/** Invoke the latest captured structure mutate call's onError callback. */
async function rejectLastStructure(error: unknown) {
  const lastCall = mocks.patchStructureResult.mutate.mock.calls.at(-1);
  if (!lastCall) throw new Error('no patchStructure.mutate call captured');
  const opts = lastCall[1] as { onError?: (e: unknown) => void };
  await act(async () => {
    opts.onError?.(error);
  });
}

async function rejectLastStructureAsConflict(currentRevision: number) {
  await rejectLastStructure(outlineConflictErr(currentRevision));
}

describe('OutlineCanvas — Scene/Beat authoring (V1.200 DR-26 Task 3)', () => {
  beforeEach(() => {
    mocks.patchStructureResult.mutate.mockClear();
    mocks.outlineResult.data = mocks.OUTLINE;
  });

  it('creating a scene sends add_scene with its target chapter_id', async () => {
    const user = userEvent.setup();
    renderOutline();

    await user.selectOptions(screen.getByTestId('outline-scene-chapter'), '1');
    await user.type(screen.getByTestId('outline-scene-title'), 'Opening Scene');
    await user.click(screen.getByTestId('outline-add-scene'));

    expect(mocks.patchStructureResult.mutate).toHaveBeenCalledWith(
      expect.objectContaining({
        work_id: 'wk_test',
        base_revision: 2,
        operation: 'add_scene',
        chapter_id: 1,
        title: 'Opening Scene',
      }),
      expect.anything(),
    );
  });

  it('a stale-revision 409 on add_scene opens the existing conflict modal', async () => {
    const user = userEvent.setup();
    renderOutline();

    await user.type(screen.getByTestId('outline-scene-title'), 'Opening Scene');
    await user.click(screen.getByTestId('outline-add-scene'));
    await rejectLastStructureAsConflict(5);

    // Envelope `current_revision: 5` ≠ cached `outline_revision: 2`: the
    // modal must show the server's canonical 5, not the stale cached 2.
    expect(screen.getByRole('heading', { name: 'Outline Conflict' })).toBeInTheDocument();
    expect(screen.getByText('5', { selector: 'span.font-mono' })).toBeInTheDocument();
  });

  it('renders the authoring copy from the canvas catalog in zh-CN (Greptile wave A)', async () => {
    await i18n.changeLanguage('zh-CN');
    mocks.outlineResult.data = AUTHORING_OUTLINE;
    renderOutline();

    // Copy comes from the catalog keys (zh-CN), not an inline English default.
    expect(screen.getByText('场景与节拍')).toBeInTheDocument();
    expect(screen.getByText('添加场景')).toBeInTheDocument();
    expect(screen.getByText('场景标题')).toBeInTheDocument();
    expect(screen.getByText('第 1 章')).toBeInTheDocument();
    expect(screen.getByTestId('outline-remove-scene-scn_1')).toHaveTextContent('移除');
    expect(screen.getByTestId('outline-remove-beat-bet_1')).toHaveTextContent('移除');
    expect(screen.getByTestId('outline-add-beat-scn_1')).toHaveTextContent('添加节拍');
  });

  it('adds a beat to its parent scene and removes scenes/beats by canonical id', async () => {
    const user = userEvent.setup();
    mocks.outlineResult.data = AUTHORING_OUTLINE;
    // Removing a scene that owns beats is gated behind a cascade confirmation.
    const confirmSpy = vi.spyOn(window, 'confirm').mockReturnValue(true);
    renderOutline();

    await user.type(screen.getByTestId('outline-beat-title-scn_1'), 'Turn');
    await user.click(screen.getByTestId('outline-add-beat-scn_1'));
    expect(mocks.patchStructureResult.mutate).toHaveBeenLastCalledWith(
      expect.objectContaining({
        operation: 'add_beat',
        scene_id: 'scn_1',
        title: 'Turn',
      }),
      expect.anything(),
    );

    await user.click(screen.getByTestId('outline-remove-beat-bet_1'));
    expect(mocks.patchStructureResult.mutate).toHaveBeenLastCalledWith(
      expect.objectContaining({ operation: 'remove_beat', beat_id: 'bet_1' }),
      expect.anything(),
    );

    await user.click(screen.getByTestId('outline-remove-scene-scn_1'));
    expect(mocks.patchStructureResult.mutate).toHaveBeenLastCalledWith(
      expect.objectContaining({ operation: 'remove_scene', scene_id: 'scn_1' }),
      expect.anything(),
    );

    confirmSpy.mockRestore();
  });
});

// ---------------------------------------------------------------------------
// V1.200 Greptile wave A — canonical scene/beat visibility + authoring guards
//
//   1. Authored scenes/beats (the Work's canonical arrays) must be visible on
//      the whole Outline surface — graph, alt view, Scene/Beat inspectors —
//      without an explicit Studio fixture.
//   2. `remove_scene` on a scene that owns beats confirms first and names the
//      cascaded beats.
//   4. Draft titles are cleared only after the write SUCCEEDS (a 409 refusal
//      keeps the draft for retry).
// ---------------------------------------------------------------------------

describe('OutlineCanvas — canonical scene/beat visibility (Greptile wave A)', () => {
  beforeEach(() => {
    mocks.patchStructureResult.mutate.mockClear();
    mocks.outlineResult.data = AUTHORING_OUTLINE;
    mocks.chaptersResult.data = {
      pages: [{ items: [mocks.CHAPTER_1], pagination: { has_more: false, next_cursor: null } }],
    };
  });

  it('projects the Work canonical scenes/beats into the graph, inspector and alt view without a fixture', async () => {
    const user = userEvent.setup();
    renderOutline(); // No sceneBeatFixture prop — canonical arrays are the source.

    const harness = screen.getByTestId('rf-integration-harness');
    // Real RF nodes for the canonical scene + beat (previously dropped by the
    // empty fixture, which hid authored structure from the graph).
    expect(harness.querySelector('.react-flow__node[data-id="scene:scn_1"]')).not.toBeNull();
    expect(harness.querySelector('.react-flow__node[data-id="beat:bet_1"]')).not.toBeNull();

    // Clicking the canonical scene node resolves the Scene inspector from those
    // same arrays (parent-chapter helper included).
    await user.click(within(harness).getByText('Opening'));
    expect(screen.getByText('Part of Chapter One.')).toBeInTheDocument();

    // Alt view nests the canonical scene/beat rows under their chapter.
    await user.click(screen.getByRole('button', { name: 'Show list view' }));
    const altSection = screen.getByLabelText('Outline chapters and timeline in list order');
    expect(within(altSection).getByText('Opening')).toBeInTheDocument();
    expect(within(altSection).getByText('Hook')).toBeInTheDocument();
  });

  it('keeps honest empty chrome when the canonical arrays are genuinely empty', async () => {
    const user = userEvent.setup();
    mocks.outlineResult.data = mocks.OUTLINE; // scenes: [], beats: []
    renderOutline();

    await user.click(screen.getByRole('button', { name: 'Show list view' }));
    expect(screen.queryByText('No scenes in this chapter yet.')).not.toBeInTheDocument();
  });
});

describe('OutlineCanvas — scene removal cascade guard (Greptile wave A)', () => {
  beforeEach(() => {
    mocks.patchStructureResult.mutate.mockClear();
    mocks.outlineResult.data = AUTHORING_OUTLINE; // scn_1 owns bet_1 ("Hook")
    mocks.chaptersResult.data = {
      pages: [{ items: [mocks.CHAPTER_1], pagination: { has_more: false, next_cursor: null } }],
    };
  });

  it('confirms first and names the affected beats before removing a scene with beats', async () => {
    const user = userEvent.setup();
    const confirmSpy = vi.spyOn(window, 'confirm').mockReturnValue(true);
    renderOutline();

    await user.click(screen.getByTestId('outline-remove-scene-scn_1'));

    expect(confirmSpy).toHaveBeenCalledTimes(1);
    expect(confirmSpy.mock.calls[0]?.[0]).toContain('Hook');
    expect(mocks.patchStructureResult.mutate).toHaveBeenLastCalledWith(
      expect.objectContaining({ operation: 'remove_scene', scene_id: 'scn_1' }),
      expect.anything(),
    );

    confirmSpy.mockRestore();
  });

  it('does not dispatch remove_scene when the author cancels the cascade confirmation', async () => {
    const user = userEvent.setup();
    const confirmSpy = vi.spyOn(window, 'confirm').mockReturnValue(false);
    renderOutline();

    await user.click(screen.getByTestId('outline-remove-scene-scn_1'));

    expect(confirmSpy).toHaveBeenCalledTimes(1);
    expect(mocks.patchStructureResult.mutate).not.toHaveBeenCalled();

    confirmSpy.mockRestore();
  });

  it('removes a beat-less scene without asking for confirmation', async () => {
    const user = userEvent.setup();
    mocks.outlineResult.data = { ...AUTHORING_OUTLINE, beats: [] };
    const confirmSpy = vi.spyOn(window, 'confirm').mockReturnValue(true);
    renderOutline();

    await user.click(screen.getByTestId('outline-remove-scene-scn_1'));

    expect(confirmSpy).not.toHaveBeenCalled();
    expect(mocks.patchStructureResult.mutate).toHaveBeenLastCalledWith(
      expect.objectContaining({ operation: 'remove_scene', scene_id: 'scn_1' }),
      expect.anything(),
    );

    confirmSpy.mockRestore();
  });
});

describe('OutlineCanvas — drafts survive failed writes (Greptile wave A)', () => {
  beforeEach(() => {
    mocks.patchStructureResult.mutate.mockClear();
    mocks.outlineResult.data = mocks.OUTLINE;
  });

  it('clears the scene title draft only after the write succeeds', async () => {
    const user = userEvent.setup();
    renderOutline();

    await user.type(screen.getByTestId('outline-scene-title'), 'Opening Scene');
    await user.click(screen.getByTestId('outline-add-scene'));

    // The mutation mock never resolves on its own — the draft is still there.
    expect(screen.getByTestId('outline-scene-title')).toHaveValue('Opening Scene');

    const options = mocks.patchStructureResult.mutate.mock.calls.at(-1)?.[1] as {
      onSuccess?: () => void;
    };
    await act(async () => {
      options.onSuccess?.();
    });

    expect(screen.getByTestId('outline-scene-title')).toHaveValue('');
  });

  it('keeps the scene title draft when add_scene is refused with a 409', async () => {
    const user = userEvent.setup();
    renderOutline();

    await user.type(screen.getByTestId('outline-scene-title'), 'Opening Scene');
    await user.click(screen.getByTestId('outline-add-scene'));
    await rejectLastStructureAsConflict(5);

    // Conflict modal opens AND the draft survives so the author can retry.
    expect(screen.getByRole('heading', { name: 'Outline Conflict' })).toBeInTheDocument();
    expect(screen.getByTestId('outline-scene-title')).toHaveValue('Opening Scene');
  });

  it('keeps the scene title draft when add_scene is refused with a non-conflict 422', async () => {
    const user = userEvent.setup();
    renderOutline();

    await user.type(screen.getByTestId('outline-scene-title'), 'Opening Scene');
    await user.click(screen.getByTestId('outline-add-scene'));
    await rejectLastStructure(
      new NexusClientError(422, 'outline_validation_failed', 'title must not be blank'),
    );

    // A validation refusal is not a conflict: no modal, and the typed title
    // survives so the author can correct it and retry (qc3 F-001 named the
    // non-conflict refusal class; the 409 cases above cover the stale-revision
    // class, whose draft also survives for the modal's replay).
    expect(screen.queryByRole('heading', { name: 'Outline Conflict' })).toBeNull();
    expect(screen.getByTestId('outline-scene-title')).toHaveValue('Opening Scene');
  });

  it('keeps a beat title draft when add_beat is refused with a 409', async () => {
    const user = userEvent.setup();
    mocks.outlineResult.data = AUTHORING_OUTLINE;
    renderOutline();

    await user.type(screen.getByTestId('outline-beat-title-scn_1'), 'Turn');
    await user.click(screen.getByTestId('outline-add-beat-scn_1'));
    await rejectLastStructureAsConflict(5);

    expect(screen.getByTestId('outline-beat-title-scn_1')).toHaveValue('Turn');
  });

  it('clears the beat title draft once add_beat succeeds', async () => {
    const user = userEvent.setup();
    mocks.outlineResult.data = AUTHORING_OUTLINE;
    renderOutline();

    await user.type(screen.getByTestId('outline-beat-title-scn_1'), 'Turn');
    await user.click(screen.getByTestId('outline-add-beat-scn_1'));
    expect(screen.getByTestId('outline-beat-title-scn_1')).toHaveValue('Turn');

    const options = mocks.patchStructureResult.mutate.mock.calls.at(-1)?.[1] as {
      onSuccess?: () => void;
    };
    await act(async () => {
      options.onSuccess?.();
    });

    expect(screen.getByTestId('outline-beat-title-scn_1')).toHaveValue('');
  });
});
