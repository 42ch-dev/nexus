/**
 * Canvas surface commands — palette registrations mounted inside each canvas
 * surface (V1.111 P0 T4).
 *
 * Unlike {@link CanvasNavCommands} (which lives in `RootLayout`), the
 * surface-switch (graph↔list toggle) and node-create commands are registered
 * INSIDE each canvas component because their handlers close over per-surface
 * local state (`showAlt` / `showList` / `createDialogOpen`). Mount/unmount of
 * the canvas auto-registers/auto-unregisters via `useRegisterCommand`.
 *
 * Coverage per task brief: command registration + handler invocation. Heavy
 * canvas internals (React Flow, daemon queries, inspectors, alt-views, conflict
 * modals) are mocked so the test focuses on the command registration and the
 * graph↔list toggle button, which is where the T4 wiring lives. The header
 * (which owns the toggle button) and the Strategy edge-create dialog (the
 * create-transition target) are kept real so handler effects are observable.
 */
import { act, cleanup, render } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import type { ReactElement } from 'react';

import { clearCommands, getCommands, type Command } from '@/lib/canvas/command-registry';

// ---------------------------------------------------------------------------
// Shared mocks — CanvasShell (React Flow) as a no-op so jsdom stays cheap.
// ---------------------------------------------------------------------------

vi.mock('@/components/canvas/canvas-shell', () => ({
  CanvasShell: () => null,
  useNodeChangeHandler: () => () => {},
}));

// ---------------------------------------------------------------------------
// OUTLINE mocks
// ---------------------------------------------------------------------------

const outlineMocks = vi.hoisted(() => ({
  outline: {
    data: {
      work_id: 'w1',
      outline_revision: 1,
      volumes: [],
      timeline_events: [],
      chapter_titles: {},
    },
    isLoading: false,
    isError: false,
    isFetching: false,
    refetch: vi.fn(),
    dataUpdatedAt: 0,
  },
  chapters: {
    data: { pages: [{ items: [] }] },
    isLoading: false,
    isError: false,
    hasNextPage: false,
    isFetchingNextPage: false,
    fetchNextPage: vi.fn(),
    refetch: vi.fn(),
    dataUpdatedAt: 0,
  },
  work: {
    data: { work_id: 'w1', title: 'Test Work', world_id: null, work_profile: 'novel' },
    isLoading: false,
    isError: false,
    refetch: vi.fn(),
    dataUpdatedAt: 0,
  },
  patch: { mutate: vi.fn(), isPending: false },
}));

vi.mock('@/api/queries', () => ({
  useWork: () => outlineMocks.work,
  useChapters: () => outlineMocks.chapters,
  flattenPages: () => [],
}));

vi.mock('@/lib/canvas/use-outline-data', async (orig) => {
  const actual = await orig<typeof import('@/lib/canvas/use-outline-data')>();
  return {
    ...actual,
    useWorkOutline: () => outlineMocks.outline,
    usePatchOutlineStructure: () => outlineMocks.patch,
    usePatchOutlineChapter: () => outlineMocks.patch,
    usePatchTimelineEvent: () => outlineMocks.patch,
  };
});

vi.mock('@/lib/nexus/query-keys', () => ({
  queryKeys: { chapters: { outlines: () => ['ch', 'out'] } },
}));

vi.mock('@/components/canvas/outline-canvas/use-outline-canvas-graph', () => ({
  useOutlineCanvasGraph: () => ({
    rfNodes: [],
    rfEdges: [],
    onNodesChange: () => {},
    selectedChapterId: null,
    setSelectedChapterId: () => {},
    selectedSceneId: null,
    selectedBeatId: null,
    projection: { nodes: [], edges: [] },
  }),
}));

// Outline heavy children → no-ops (keep canvas-layout real for the toggle).
vi.mock('@/components/canvas/outline-canvas/inspectors/chapter-inspector', () => ({
  ChapterInspector: () => null,
}));
vi.mock('@/components/canvas/outline-canvas/inspectors/scene-inspector', () => ({
  SceneInspector: () => null,
}));
vi.mock('@/components/canvas/outline-canvas/inspectors/beat-inspector', () => ({
  BeatInspector: () => null,
}));
vi.mock('@/components/canvas/outline-canvas/inspectors/event-inspector', () => ({
  TimelinePanel: () => null,
}));
vi.mock('@/components/canvas/outline-canvas/inspectors/structure-inspector', () => ({
  OutlineStructurePanel: () => null,
}));
vi.mock('@/components/canvas/outline-canvas/outline-alt-view', () => ({
  OutlineAltView: () => null,
}));
vi.mock('@/components/canvas/outline-canvas/conflict-modal', () => ({
  OutlineConflictDialog: () => null,
}));
vi.mock('@/components/canvas/outline-canvas/outline-nodes', () => ({
  outlineNodeTypes: {},
}));
vi.mock('@/components/canvas/outline-canvas/rf-projection', () => ({
  outlineGraphSummary: () => 'summary',
}));

// ---------------------------------------------------------------------------
// STRATEGY mocks
// ---------------------------------------------------------------------------

const strategyMocks = vi.hoisted(() => ({
  graphQuery: {
    isLoading: false,
    isError: false,
    refetch: vi.fn(),
    data: {
      revision: 1,
      parsed: { manifest: { states: [] }, problems: [] },
      graph: { danglingTargets: [] },
    },
  },
}));

vi.mock('@/components/canvas/strategy-canvas/hooks/use-strategy-canvas', () => {
  const strategyState = {
    graphQuery: strategyMocks.graphQuery,
    activeSession: null,
    creatorId: 'c1',
    nodes: [] as never[],
    edges: [] as never[],
    onNodesChange: () => {},
    onEdgesChange: () => {},
    onConnect: () => {},
    onReconnect: () => {},
    selected: null,
    selectedState: null,
    baseRevision: 1,
    promptTemplateRef: null,
    revisionStatus: 'clean' as const,
    summaryText: 'summary',
    activeScheduleId: undefined,
    form: { label: '', description: '', nextTarget: '', promptBody: '' },
    setForm: () => {},
    saveStatuses: {},
    setSaveStatuses: () => {},
    setActiveSection: () => {},
    conflict: null,
    setConflict: () => {},
    saveTriggers: {},
    workingRevisionRef: { current: 1 },
    handleConflict: () => {},
    handleReapply: () => {},
    selectedDraftEdge: null,
    draftSourceState: null,
    commitDraft: () => {},
    isCommittingDraft: false,
    cancelDraft: () => {},
    commitKeyboardCreate: () => {},
    isCommittingKeyboardCreate: false,
  };
  return { useStrategyCanvas: () => strategyState };
});

// Strategy heavy children → no-ops (keep canvas-layout + edge-create-dialog real).
vi.mock('@/components/canvas/strategy-canvas/inspectors/state-inspector', () => ({
  StateInspector: () => null,
}));
vi.mock('@/components/canvas/strategy-canvas/inspectors/edge-inspector', () => ({
  EdgeInspector: () => null,
  DraftEdgeInspector: () => null,
}));
vi.mock('@/components/canvas/strategy-canvas/inspectors/prompt-inspector', () => ({
  PromptInspector: () => null,
}));
vi.mock('@/components/canvas/strategy-canvas/inspector-panel', () => ({
  InspectorPanel: () => null,
  StrategyConflictModal: () => null,
}));
vi.mock('@/components/canvas/strategy-canvas/state-machine', () => ({
  ValidationPanel: () => null,
  RevisionBadge: () => null,
  ArtifactsList: () => null,
  originalFormOf: () => ({ label: '', description: '', nextTarget: '', promptBody: '' }),
}));
vi.mock('@/components/canvas/strategy-alt-view', () => ({
  StrategyAltView: () => null,
}));
vi.mock('@/components/canvas/strategy-nodes', () => ({
  strategyNodeTypes: {},
}));
vi.mock('@/components/canvas/idea-input', () => ({
  IdeaInput: () => null,
}));

// ---------------------------------------------------------------------------
// WORLD KB mocks
// ---------------------------------------------------------------------------

const worldKbMocks = vi.hoisted(() => ({
  graph: {
    data: { entities: [], source_anchors: [], relationships: [] },
    isLoading: false,
    isError: false,
    isFetching: false,
    dataUpdatedAt: 0,
    refetch: vi.fn(),
  },
  candidates: { data: { items: [] }, isLoading: false },
  patchRelationship: { mutate: vi.fn(), mutateAsync: vi.fn(), isPending: false },
}));

vi.mock('@/lib/canvas/use-world-kb-data', () => ({
  useWorldKbGraph: () => worldKbMocks.graph,
  useWorldKbCandidates: () => worldKbMocks.candidates,
  usePatchWorldKbRelationship: () => worldKbMocks.patchRelationship,
  usePatchWorldKbEntity: () => ({ mutate: vi.fn(), isPending: false }),
  usePromoteWorldKbCandidate: () => ({ mutate: vi.fn(), mutateAsync: vi.fn(), isPending: false }),
  isWorldKbConflictError: () => false,
  isWorldKbValidationError: () => false,
}));

vi.mock('@/components/canvas/world-kb/use-world-kb-canvas-state', () => ({
  useWorldKbCanvasState: () => ({
    selection: null,
    setSelection: () => {},
    selectedNodeId: null,
    selectedRelationshipId: null,
    entityConflict: null,
    promoteConflict: null,
    relationshipConflict: null,
    reseedSignal: 0,
    bumpReseed: () => {},
    setEntityConflict: () => {},
    setPromoteConflict: () => {},
    setRelationshipConflict: () => {},
    onSelectNode: () => {},
    onSelectRelationship: () => {},
    onCreateRelationship: () => {},
    onEdgeClick: () => {},
  }),
  buildEntityConflict: () => null,
  handleRelationshipConflict: () => {},
  handlePromoteConflict: () => {},
}));

vi.mock('@/components/canvas/world-kb/use-view-preference', () => ({
  useReducedMotionPreference: () => false,
}));

vi.mock('@/components/canvas/world-kb/world-kb-canvas-conflicts', () => ({
  WorldKbCanvasConflicts: () => null,
}));
vi.mock('@/components/canvas/world-kb/world-kb-alt-view', () => ({
  WorldKbAltView: () => null,
}));
vi.mock('@/components/canvas/world-kb/world-kb-inspector-panel', () => ({
  InspectorPanel: () => null,
}));
vi.mock('@/components/canvas/world-kb/entity-node', () => ({
  worldKbNodeTypes: {},
}));
vi.mock('@/components/canvas/world-kb/graph-projection', () => ({
  anchorNodes: () => [],
  deriveEdges: () => [],
  entryCountOf: () => 0,
  graphSummary: () => 'summary',
  layoutNodes: () => [],
}));
vi.mock('@/components/canvas/world-kb/relationship-projection', () => ({
  deriveRelationshipEdges: () => [],
  filterRelationshipEdgesByConfidence: () => [],
}));
vi.mock('@/components/canvas/world-kb/relationship-inspector-logic', () => ({
  buildRelationshipRemoveRequest: () => ({}),
}));
vi.mock('@/components/canvas/world-kb/world-kb-canvas-utils', () => ({
  formatRelative: () => 'now',
  nodesToData: () => [],
}));
vi.mock('@/components/canvas/world-kb/types', () => ({}));

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function findById(id: string): Command | undefined {
  return getCommands().find((c) => c.id === id);
}

const queryClient = new QueryClient({
  defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
});

/** Wrap an element in the QueryClientProvider the canvas hooks require. */
function withProviders(element: ReactElement): ReactElement {
  return <QueryClientProvider client={queryClient}>{element}</QueryClientProvider>;
}

beforeEach(() => {
  clearCommands();
});

afterEach(() => {
  cleanup();
  clearCommands();
});

// ---------------------------------------------------------------------------
// Strategy — toggle view + create transition (the only surface with a clean
// canvas-level creation entrypoint: the "Create Transition…" button + dialog).
// ---------------------------------------------------------------------------

describe('StrategyCanvas — palette commands', () => {
  it('registers a toggle-view and a create-transition command on mount', async () => {
    const { StrategyCanvas } = await import('@/components/canvas/strategy-canvas');
    render(withProviders(<StrategyCanvas presetId="p1" />));
    expect(getCommands().map((c) => c.id).sort()).toEqual([
      'strategy.create-transition',
      'strategy.toggle-view',
    ]);
  });

  it('unregisters both on unmount', async () => {
    const { StrategyCanvas } = await import('@/components/canvas/strategy-canvas');
    const { unmount } = render(withProviders(<StrategyCanvas presetId="p1" />));
    unmount();
    expect(getCommands()).toEqual([]);
  });

  it('toggle-view command carries the Strategy group', async () => {
    const { StrategyCanvas } = await import('@/components/canvas/strategy-canvas');
    render(withProviders(<StrategyCanvas presetId="p1" />));
    expect(findById('strategy.toggle-view')?.groupKey).toBe('group.strategy');
  });

  it('toggle handler flips aria-pressed across repeated invocations', async () => {
    // useRegisterCommand captures the command once on mount. The toggle handler
    // uses setShowAlt(v => !v) so it never closes over a stale boolean — two
    // invocations must flip true then false, not true then true.
    const { StrategyCanvas } = await import('@/components/canvas/strategy-canvas');
    const { container } = render(withProviders(<StrategyCanvas presetId="p1" />));

    const toggle = container.querySelector('button[aria-pressed]') as HTMLButtonElement;
    expect(toggle).not.toBeNull();
    expect(toggle.getAttribute('aria-pressed')).toBe('false');

    act(() => {
      findById('strategy.toggle-view')?.handler();
    });
    expect(toggle.getAttribute('aria-pressed')).toBe('true');

    // Second invocation via the SAME captured handler must flip back.
    act(() => {
      findById('strategy.toggle-view')?.handler();
    });
    expect(toggle.getAttribute('aria-pressed')).toBe('false');
  });

  it('create-transition handler opens the edge-create dialog', async () => {
    const { StrategyCanvas } = await import('@/components/canvas/strategy-canvas');
    const renderResult = render(withProviders(<StrategyCanvas presetId="p1" />));

    // The Radix Dialog is not mounted while closed. (The header's "Create
    // Transition…" button is always visible, so assert on role="dialog"
    // rather than text — the dialog content renders into a portal only when
    // open=true.)
    expect(renderResult.queryByRole('dialog')).toBeNull();

    act(() => {
      findById('strategy.create-transition')?.handler();
    });

    expect(renderResult.getByRole('dialog')).toBeInTheDocument();
  });
});

// ---------------------------------------------------------------------------
// Outline — toggle view only. No node-create: the outline canvas exposes no
// chapter-creation entrypoint (structure panel is select/move only).
// ---------------------------------------------------------------------------

describe('OutlineCanvas — palette commands', () => {
  it('registers ONLY a toggle-view command (no node-create)', async () => {
    const { OutlineCanvas } = await import('@/components/canvas/outline-canvas');
    render(withProviders(<OutlineCanvas workId="w1" />));
    expect(getCommands().map((c) => c.id)).toEqual(['outline.toggle-view']);
  });

  it('toggle-view command carries the Outline group', async () => {
    const { OutlineCanvas } = await import('@/components/canvas/outline-canvas');
    render(withProviders(<OutlineCanvas workId="w1" />));
    expect(findById('outline.toggle-view')?.groupKey).toBe('group.outline');
  });

  it('toggle handler flips aria-pressed across repeated invocations', async () => {
    const { OutlineCanvas } = await import('@/components/canvas/outline-canvas');
    const { container } = render(withProviders(<OutlineCanvas workId="w1" />));

    const toggle = container.querySelector('button[aria-pressed]') as HTMLButtonElement;
    expect(toggle).not.toBeNull();
    expect(toggle.getAttribute('aria-pressed')).toBe('false');

    act(() => {
      findById('outline.toggle-view')?.handler();
    });
    expect(toggle.getAttribute('aria-pressed')).toBe('true');

    act(() => {
      findById('outline.toggle-view')?.handler();
    });
    expect(toggle.getAttribute('aria-pressed')).toBe('false');
  });

  it('unregisters on unmount', async () => {
    const { OutlineCanvas } = await import('@/components/canvas/outline-canvas');
    const { unmount } = render(withProviders(<OutlineCanvas workId="w1" />));
    unmount();
    expect(getCommands()).toEqual([]);
  });
});

// ---------------------------------------------------------------------------
// World KB — toggle view only. No node-create: the World KB canvas exposes no
// clean canvas-level creation button (relationships are created via graph
// edge-drag or alt-view row action, both of which require entity context).
// ---------------------------------------------------------------------------

describe('WorldKbCanvas — palette commands', () => {
  it('registers ONLY a toggle-view command (no node-create)', async () => {
    const { WorldKbCanvas } = await import('@/components/canvas/world-kb/world-kb-canvas');
    render(withProviders(<WorldKbCanvas worldId="world1" />));
    expect(getCommands().map((c) => c.id)).toEqual(['world-kb.toggle-view']);
  });

  it('toggle-view command carries the World KB group', async () => {
    const { WorldKbCanvas } = await import('@/components/canvas/world-kb/world-kb-canvas');
    render(withProviders(<WorldKbCanvas worldId="world1" />));
    expect(findById('world-kb.toggle-view')?.groupKey).toBe('group.world-kb');
  });

  it('toggle handler flips aria-pressed and returns to the initial value', async () => {
    const { WorldKbCanvas } = await import('@/components/canvas/world-kb/world-kb-canvas');
    const { container } = render(withProviders(<WorldKbCanvas worldId="world1" />));

    const toggle = container.querySelector('button[aria-pressed]') as HTMLButtonElement;
    expect(toggle).not.toBeNull();
    // useReducedMotionPreference is mocked false → showList starts false.
    const initial = toggle.getAttribute('aria-pressed');
    expect(initial).toBe('false');

    act(() => {
      findById('world-kb.toggle-view')?.handler();
    });
    expect(toggle.getAttribute('aria-pressed')).toBe('true');

    act(() => {
      findById('world-kb.toggle-view')?.handler();
    });
    expect(toggle.getAttribute('aria-pressed')).toBe(initial);
  });

  it('unregisters on unmount', async () => {
    const { WorldKbCanvas } = await import('@/components/canvas/world-kb/world-kb-canvas');
    const { unmount } = render(withProviders(<WorldKbCanvas worldId="world1" />));
    unmount();
    expect(getCommands()).toEqual([]);
  });
});
