/**
 * Outline canvas — Scene/Beat inspector selection wiring (V1.109 C2 T4;
 * FB-C2-002).
 *
 * The RF graph (CanvasShell) is mocked out in the orchestrator test file, so
 * graph-click selection cannot drive `selectedSceneId`/`selectedBeatId`
 * through real RF state. This file mocks `useOutlineCanvasGraph` directly to
 * verify the orchestrator's WIRING: that when the hook returns a selected
 * scene/beat, the orchestrator resolves the entity from the fixture payload
 * and mounts the correct inspector with the right parent-title helper.
 *
 * The hook's own selection logic is tested separately in
 * `use-outline-canvas-graph.test.ts` (16 tests including scene/beat selection).
 */
import { describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';

import { OutlineCanvas } from '@/components/canvas/outline-canvas';

// ---------------------------------------------------------------------------
// Mocks — minimal stubs. The hook is mocked so we control selection state.
// ---------------------------------------------------------------------------

vi.mock('@/components/canvas/canvas-shell', () => ({
  CanvasShell: ({ children }: { children?: React.ReactNode }) => (
    <div data-testid="canvas-shell-mock">{children}</div>
  ),
  useNodeChangeHandler: () => () => {},
}));

const mocks = vi.hoisted(() => ({
  WORK: { work_id: 'wk_test', title: 'Test Work', work_profile: 'novel', created_at: '', updated_at: '' },
  CHAPTER_1: {
    work_id: 'wk_test', chapter: 1, volume: 1, title: 'Chapter One', slug: 'ch-1',
    status: 'draft' as const, planned_word_count: 1000, actual_word_count: 500,
    outline_path: undefined, body_path: undefined, created_at: '', updated_at: '',
  },
  OUTLINE: {
    work_id: 'wk_test', outline_revision: 2,
    volumes: [{ volume_id: 1, label: 'Volume 1', chapter_ids: [1] }],
    timeline_events: [], foreshadows: [], chapter_titles: {}, updated_at: '',
  },
}));

vi.mock('@/api/queries', () => ({
  useWork: () => ({ data: mocks.WORK, isLoading: false, isError: false, isFetching: false, refetch: vi.fn(), dataUpdatedAt: 0 }),
  useChapters: () => ({
    data: { pages: [{ items: [mocks.CHAPTER_1], pagination: { has_more: false, next_cursor: null } }] },
    isLoading: false, isError: false, isFetching: false, hasNextPage: false,
    isFetchingNextPage: false, fetchNextPage: vi.fn(), refetch: vi.fn(), dataUpdatedAt: 0,
  }),
  useChapterOutline: () => ({ data: undefined, isLoading: false, isError: false, isFetching: false, refetch: vi.fn(), dataUpdatedAt: 0 }),
  flattenPages: (data: { pages: { items: unknown[] }[] } | undefined): unknown[] => {
    if (!data) return [];
    return data.pages.flatMap((p) => p.items);
  },
}));

vi.mock('@/lib/canvas/use-outline-data', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@/lib/canvas/use-outline-data')>();
  return {
    ...actual,
    useWorkOutline: () => ({
      data: mocks.OUTLINE, isLoading: false, isError: false, isFetching: false,
      refetch: vi.fn().mockResolvedValue({ data: mocks.OUTLINE }), dataUpdatedAt: 0,
    }),
    usePatchOutlineStructure: () => ({ mutate: vi.fn(), isPending: false }),
    usePatchOutlineChapter: () => ({ mutate: vi.fn(), isPending: false }),
    usePatchTimelineEvent: () => ({ mutate: vi.fn(), isPending: false }),
  };
});

vi.mock('@/lib/nexus/query-keys', () => ({
  queryKeys: {
    chapters: { outlines: () => ['chapters', 'outlines'], detail: () => ['chapters', 'detail'], lists: () => ['chapters', 'lists'], list: () => ['chapters', 'list'] },
    outline: { detail: () => ['outline', 'detail'] },
  },
}));

// Control what the hook returns — this is the wiring-under-test.
const hookReturn = vi.hoisted(() => ({
  rfNodes: [] as never[],
  rfEdges: [] as never[],
  onNodesChange: () => {},
  selectedChapterId: null as number | null,
  setSelectedChapterId: () => {},
  selectedSceneId: null as string | null,
  selectedBeatId: null as string | null,
  projection: null as never,
}));

vi.mock('@/components/canvas/outline-canvas/use-outline-canvas-graph', () => ({
  useOutlineCanvasGraph: () => hookReturn,
}));

// ---------------------------------------------------------------------------
// Fixture + helpers
// ---------------------------------------------------------------------------

const FIXTURE = {
  scenes: [
    { sceneId: 'scene-1', chapterId: 1, title: 'Opening Scene', status: 'drafted' as const },
    { sceneId: 'scene-2', chapterId: 1, title: 'Closing Scene', status: 'completed' as const },
  ],
  beats: [
    { beatId: 'beat-1', sceneId: 'scene-1', title: 'Inciting Moment', status: null },
  ],
};

const queryClient = new QueryClient({
  defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
});

function renderWithFixture() {
  return render(
    <QueryClientProvider client={queryClient}>
      <OutlineCanvas workId="wk_test" sceneBeatFixture={FIXTURE} />
    </QueryClientProvider>,
  );
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe('OutlineCanvas — Scene/Beat inspector selection wiring (FB-C2-002)', () => {
  it('renders the Scene inspector with resolved data + parent chapter helper when a scene is selected', () => {
    hookReturn.selectedSceneId = 'scene-1';
    hookReturn.selectedBeatId = null;
    hookReturn.selectedChapterId = null;

    renderWithFixture();

    // Scene inspector heading + title.
    expect(screen.getByText('Scene').closest('[class*="card"]')).toBeInTheDocument();
    expect(screen.getByText('Opening Scene')).toBeInTheDocument();
    // Parent chapter helper (Voice & Content lock).
    expect(screen.getByText('Part of Chapter One.')).toBeInTheDocument();
    // Read-only banner.
    expect(screen.getByText('Scene details are view-only for now.')).toBeInTheDocument();
    // Status field.
    expect(screen.getByText('Drafted')).toBeInTheDocument();
  });

  it('renders the Beat inspector with resolved data + parent scene helper when a beat is selected', () => {
    hookReturn.selectedSceneId = null;
    hookReturn.selectedBeatId = 'beat-1';
    hookReturn.selectedChapterId = null;

    renderWithFixture();

    expect(screen.getByText('Beat').closest('[class*="card"]')).toBeInTheDocument();
    expect(screen.getByText('Inciting Moment')).toBeInTheDocument();
    // Parent scene helper — beat-1 belongs to scene-1 ("Opening Scene").
    expect(screen.getByText('Part of Opening Scene.')).toBeInTheDocument();
    expect(screen.getByText('Beat details are view-only for now.')).toBeInTheDocument();
  });

  it('falls back to the Chapter inspector when neither Scene nor Beat is selected', () => {
    hookReturn.selectedSceneId = null;
    hookReturn.selectedBeatId = null;
    hookReturn.selectedChapterId = null;

    renderWithFixture();

    // Chapter inspector empty-state prompt (no chapter selected).
    expect(screen.getByText('Select a chapter to inspect its outline metadata.')).toBeInTheDocument();
    // No Scene/Beat inspector headings.
    expect(screen.queryByText('Scene')).not.toBeInTheDocument();
    expect(screen.queryByText('Beat')).not.toBeInTheDocument();
  });
});
