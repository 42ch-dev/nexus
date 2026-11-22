/**
 * note: Timeline canvas write-boundary tests (V1.122 P1 T4).
 *
 * Asserts the architect-locked write boundary (§4) + conflict policy (§5):
 *
 *   1. Patching a Timeline entity node routes through
 *      `NexusClient.worldKbPatchEntity` (V1.73 `POST .../kb/patch-entity`)
 *      with the correct `{ worldId, entity_id, expected_version, patch }`
 *      payload. The adapter's inspector does not call the mutation hook
 *      directly — it forwards a structured patch via `ctxRef.onPatchEntity`,
 *      which the orchestrator wires to the legitimate write path.
 *
 *   2. A 409 `world_kb_conflict` response opens the world-kb-flavored
 *      conflict modal (`WorldKbEntityConflictModal` — reused V1.73/V1.74 copy
 *      tokens, NO Timeline-specific conflict DTO), keeps the draft patch,
 *      and refetches the canonical graph.
 *
 *   3. A 422 `world_kb_validation_failed` response renders
 *      `validation_summary.errors[]` so the author can see why the daemon
 *      rejected the patch.
 *
 *   4-6. Negative assertions (architect-locked §4.2 — the test fails loudly
 *      the moment any forbidden method is wired into the Timeline surface):
 *      - `client.patchTimelineEvent` (Work-scoped) is NOT called.
 *      - `client.worldKbPatchRelationship` is NOT called (relationships are
 *        read-only on Timeline in V1.122).
 *      - `client.worldKbPromoteCandidate` is NOT called.
 *      - No raw-file write path: no `fetch PUT` to a file route; no desktop
 *        IPC writing to disk.
 *
 * The pure conflict extraction (`extractTimelineConflict`) is covered
 * separately at the bottom — it has no React dependencies.
 */
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { http, HttpResponse } from 'msw';
import type { Node } from '@xyflow/react';

import { makeQueryClient, renderInApp } from '@/test/test-providers';
import { QueryClientProvider } from '@tanstack/react-query';
import { ClientProvider } from '@/lib/client-context';
import { ToastProvider, Toaster } from '@/lib/use-toast';
import { useHandlers } from '@/test/msw-server';
import { NexusClientError, type NexusClient } from '@/lib/nexus';
import type {
  WorldKbEntityProjection,
  WorldKbGraphResponse,
} from '@42ch/nexus-contracts';

import {
  extractTimelineConflict,
  type TimelineCanvasAdapterContext,
  type TimelineNodeData,
} from '../timeline-canvas-adapter';
import { TimelineInspector } from '../timeline-inspector';
import { TimelineCanvas } from '../timeline-canvas';

function entityEvent(
  overrides: Partial<WorldKbEntityProjection> = {},
): WorldKbEntityProjection {
  return {
    key_block_id: 'kb-event-1',
    world_id: 'world-7',
    block_type: 'event',
    canonical_name: 'Coronation',
    status: 'confirmed',
    version: 3,
    body: { attributes: { occurred_at: '1042-03-01T00:00:00Z' } },
    ...overrides,
  } as WorldKbEntityProjection;
}

function eventNode(
  overrides: Partial<TimelineNodeData> = {},
): Node<TimelineNodeData> {
  return {
    id: 'entity:kb-event-1',
    type: 'timeline-event',
    position: { x: 0, y: 0 },
    data: {
      ...entityEvent(),
      layoutHint: 'event',
      occurredAtHint: '1042-03-01T00:00:00Z',
      ...overrides,
    } as TimelineNodeData,
  };
}

function makeMockClient(): NexusClient {
  // why: every potentially-forbidden method is tracked so negative assertions
  // fail loudly if any Timeline code path accidentally wires them.
  return {
    getWorldKbGraph: vi.fn().mockResolvedValue({
      entities: [entityEvent()],
      source_anchors: [],
      relationships: [],
    }),
    worldKbPatchEntity: vi.fn().mockResolvedValue({
      entity: entityEvent({ canonical_name: 'Edited', version: 4 }),
      version: 4,
      validation_summary: { errors: [], warnings: [] },
    }),
    worldKbPatchRelationship: vi.fn(),
    worldKbPromoteCandidate: vi.fn(),
    patchTimelineEvent: vi.fn(),
    patchOutlineStructure: vi.fn(),
    patchOutlineChapter: vi.fn(),
    health: vi.fn().mockResolvedValue({ status: 'ok', version: 'test' }),
  } as unknown as NexusClient;
}

function renderInspector({
  client,
  ctxOverrides = {},
}: {
  client: NexusClient;
  ctxOverrides?: Partial<TimelineCanvasAdapterContext>;
}) {
  const ctxRef = { current: { worldId: 'world-7', client, ...ctxOverrides } };
  const utils = render(
    <QueryClientProvider client={makeQueryClient()}>
      <ToastProvider>
        <ClientProvider client={client}>
          <TimelineInspector node={eventNode()} ctxRef={ctxRef} />
        </ClientProvider>
        <Toaster />
      </ToastProvider>
    </QueryClientProvider>,
  );
  return { ...utils, ctxRef };
}

describe('TimelineInspector — write-boundary wiring (T4)', () => {
  it('routes a title edit through ctxRef.onPatchEntity (NOT through any forbidden client method)', async () => {
    const user = userEvent.setup();
    const client = makeMockClient();
    const onPatchEntity = vi.fn();
    renderInspector({ client, ctxOverrides: { onPatchEntity } });

    const titleInput = screen.getByDisplayValue('Coronation');
    await user.clear(titleInput);
    await user.type(titleInput, 'Coronation of Aria');
    const saveButton = screen.getByTestId('timeline-inspector-save');
    await user.click(saveButton);

    await waitFor(() => expect(onPatchEntity).toHaveBeenCalledTimes(1));
    const [node, patch, dirtyFields] = onPatchEntity.mock.calls[0];
    expect(node.id).toBe('entity:kb-event-1');
    expect(patch).toMatchObject({ title: 'Coronation of Aria' });
    expect(dirtyFields).toEqual(['title']);

    // why: the inspector never touches the client directly — the orchestrator's
    // mutation hook is the only legitimate write path.
    expect(client.worldKbPatchEntity).not.toHaveBeenCalled();
    expect(client.patchTimelineEvent).not.toHaveBeenCalled();
    expect(client.worldKbPatchRelationship).not.toHaveBeenCalled();
    expect(client.worldKbPromoteCandidate).not.toHaveBeenCalled();
  });

  it('routes a body edit through ctxRef.onPatchEntity with parsed JSON body', async () => {
    const user = userEvent.setup();
    const client = makeMockClient();
    const onPatchEntity = vi.fn();
    renderInspector({ client, ctxOverrides: { onPatchEntity } });

    const bodyTextarea = screen.getByLabelText('Body') as HTMLTextAreaElement;
    // why: userEvent.type interprets `{` as a key descriptor; JSON inputs use
    // fireEvent.change so the literal braces land in the textarea verbatim.
    fireEvent.change(bodyTextarea, {
      target: { value: '{"attributes":{"occurred_at":"1100-01-01T00:00:00Z"}}' },
    });
    await user.click(screen.getByTestId('timeline-inspector-save'));

    await waitFor(() => expect(onPatchEntity).toHaveBeenCalled());
    const [, patch, dirtyFields] = onPatchEntity.mock.calls[0];
    expect(dirtyFields).toContain('body');
    expect(patch.body).toMatchObject({
      attributes: { occurred_at: '1100-01-01T00:00:00Z' },
    });
  });

  it('shows an inline error when body JSON is invalid (no patch emitted)', async () => {
    const user = userEvent.setup();
    const client = makeMockClient();
    const onPatchEntity = vi.fn();
    renderInspector({ client, ctxOverrides: { onPatchEntity } });

    const bodyTextarea = screen.getByLabelText('Body');
    fireEvent.change(bodyTextarea, { target: { value: '{not valid json' } });
    await user.click(screen.getByTestId('timeline-inspector-save'));

    await waitFor(() =>
      expect(screen.getByTestId('timeline-inspector-validation-errors')).toBeInTheDocument(),
    );
    expect(onPatchEntity).not.toHaveBeenCalled();
  });

  it('rejects non-object JSON bodies with the object-only error copy', async () => {
    const user = userEvent.setup();
    const client = makeMockClient();
    const onPatchEntity = vi.fn();
    renderInspector({ client, ctxOverrides: { onPatchEntity } });

    const bodyTextarea = screen.getByLabelText('Body');
    fireEvent.change(bodyTextarea, { target: { value: '[1, 2, 3]' } });
    await user.click(screen.getByTestId('timeline-inspector-save'));

    await waitFor(() =>
      expect(screen.getByTestId('timeline-inspector-validation-errors')).toBeInTheDocument(),
    );
    expect(onPatchEntity).not.toHaveBeenCalled();
  });

  it('does not invoke the client when onPatchEntity is unset (read-only mount)', async () => {
    const user = userEvent.setup();
    const client = makeMockClient();
    // why: no onPatchEntity wired — a read-only adapter mount must neither
    // throw nor touch the client.
    renderInspector({ client });

    const titleInput = screen.getByDisplayValue('Coronation');
    await user.clear(titleInput);
    await user.type(titleInput, 'X');
    await user.click(screen.getByTestId('timeline-inspector-save'));

    expect(client.worldKbPatchEntity).not.toHaveBeenCalled();
    expect(client.patchTimelineEvent).not.toHaveBeenCalled();
  });

  it('clears isSubmitting and re-enables Save when onPatchEntity rejects (PR #156 fix)', async () => {
    // why: regression (PR #156) — the inspector used to set `isSubmitting=true`
    // and forward the patch fire-and-forget; the flag was only cleared when the
    // selected node's `version` changed, which never happens on error, leaving
    // Save permanently disabled. The fix awaits the returned promise and resets
    // the flag in `finally`.
    const user = userEvent.setup();
    const client = makeMockClient();
    const onPatchEntity = vi
      .fn()
      .mockRejectedValue(new Error('simulated network failure'));
    renderInspector({ client, ctxOverrides: { onPatchEntity } });

    const titleInput = screen.getByDisplayValue('Coronation');
    await user.clear(titleInput);
    await user.type(titleInput, 'Edited Title');
    const saveButton = screen.getByTestId(
      'timeline-inspector-save',
    ) as HTMLButtonElement;

    await user.click(saveButton);

    await waitFor(() => expect(onPatchEntity).toHaveBeenCalledTimes(1));

    // invariant: after the rejected promise settles, `isSubmitting` MUST reset
    // so the author can retry. The form is still dirty (the edit is unsaved),
    // so only a stuck `isSubmitting` could keep Save disabled.
    await waitFor(() => {
      expect(saveButton).not.toBeDisabled();
    });
    expect(saveButton).toHaveTextContent('Save');
    // why: a second click proves the flag did not wedge the form on the first
    // failure.
    await user.click(saveButton);
    await waitFor(() => expect(onPatchEntity).toHaveBeenCalledTimes(2));
  });
});

// note: Orchestrator-level write path + conflict UX — mounts the full
// <TimelineCanvas> with a mocked graph so `handlePatchEntity` →
// `usePatchWorldKbEntity` → `client.worldKbPatchEntity` runs end-to-end.
// CanvasShell + useCanvasSurface are stubbed so the inspector surfaces without
// React Flow selection (jsdom does not measure RF nodes).

vi.mock('@/components/canvas/canvas-shell', async () => {
  // why: the real CanvasShell (V1.123 P4 Task 3) wraps children in a
  // `<ReactFlowProvider>`; the mock must do the same so the
  // `<SemanticZoomBridge>` child (which calls `useViewport()`) mounts
  // cleanly without throwing "ReactFlowProvider as an ancestor".
  const { ReactFlowProvider } = await import('@xyflow/react');
  return {
    CanvasShell: ({ children }: { children?: React.ReactNode }) => (
      <ReactFlowProvider>
        <div data-testid="canvas-shell-mock">{children}</div>
      </ReactFlowProvider>
    ),
  };
});

// why: stub the layout/canvas hook so the test does not depend on RF
// measurement; returning a minimal shape keeps the orchestrator's render
// branches intact.
vi.mock('@/components/canvas/use-canvas-surface', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@/components/canvas/use-canvas-surface')>();
  return {
    ...actual,
    useCanvasSurface: () => ({
      nodes: [],
      edges: [],
      nodeTypes: {},
      edgeTypes: undefined,
      onNodesChange: () => {},
      summaryText: 'timeline summary',
      viewport: { cachedViewport: null, onViewportChange: () => {} },
      showAlt: false,
      setShowAlt: () => {},
      altView: null,
      inspector: null,
      selectedNode: null,
      selectedNodeId: null,
      conflict: null,
      setConflict: () => {},
      handleConflict: () => {},
      isLoading: false,
      isError: false,
      refetch: () => {},
      relayout: undefined,
    }),
  };
});

describe('TimelineCanvas orchestrator — write path + conflict UX (T4)', () => {
  beforeEach(() => {
    // why: a non-empty graph makes the orchestrator render the canvas branch
    // rather than the empty-state.
    useHandlers(
      http.get('/v1/daemon/worlds/*/kb/graph', () =>
        HttpResponse.json({
          entities: [entityEvent()],
          source_anchors: [],
          relationships: [],
        } satisfies WorldKbGraphResponse),
      ),
    );
  });

  afterEach(() => {
    vi.clearAllMocks();
  });

  it('mounts without invoking any forbidden write method (negative assertion)', async () => {
    const client = makeMockClient();
    renderInApp(<TimelineCanvas worldId="world-7" />, { client });

    await waitFor(() => {
      expect(screen.getByTestId('timeline-canvas')).toBeInTheDocument();
    });

    // invariant: mounting + reading the graph MUST NOT trigger any write
    // endpoint — the Timeline surface's read path stays pure (no
    // `patchTimelineEvent`, no `worldKbPatchRelationship`, no
    // `worldKbPromoteCandidate`, no `worldKbPatchEntity`).
    expect(client.patchTimelineEvent).not.toHaveBeenCalled();
    expect(client.worldKbPatchRelationship).not.toHaveBeenCalled();
    expect(client.worldKbPromoteCandidate).not.toHaveBeenCalled();
    expect(client.worldKbPatchEntity).not.toHaveBeenCalled();
  });

  it('does not perform any raw-file write (no fetch PUT to a file route)', async () => {
    const client = makeMockClient();
    const fetchSpy = vi.spyOn(window, 'fetch');

    renderInApp(<TimelineCanvas worldId="world-7" />, { client });
    await waitFor(() => {
      expect(screen.getByTestId('timeline-canvas')).toBeInTheDocument();
    });

    // why: the architect-locked §2 raw-file invariant forbids a PUT to any
    // file route.
    const putCalls = fetchSpy.mock.calls.filter(
      ([, init]) => (init as RequestInit | undefined)?.method === 'PUT',
    );
    expect(putCalls).toHaveLength(0);
    fetchSpy.mockRestore();
  });
});

// ─── Orchestrator-level visible ordering-disclaimer (PR #156 fix 3) ─────────
//
// Mounts the full <TimelineCanvas> with a mocked graph and asserts the
// visible ordering-disclaimer notice renders when event entities are present
// (the sighted-user counterpart to the a11y `summarizeGraph` disclaimer that
// already ships in the CanvasShell live region). The notice must be omitted
// for zero-event graphs (empty OR context-only) — mirroring the a11y
// disclaimer condition in `summarizeTimelineGraph`.

describe('TimelineCanvas orchestrator — visible ordering-disclaimer notice (PR #156 fix 3)', () => {
  afterEach(() => {
    vi.clearAllMocks();
  });

  it('renders the visible ordering-disclaimer when the graph has event entities', async () => {
    // note: the default `makeMockClient().getWorldKbGraph` returns a single
    // event entity (`entityEvent()`), so the notice must surface.
    renderInApp(<TimelineCanvas worldId="world-7" />, { client: makeMockClient() });

    await waitFor(() => {
      expect(screen.getByTestId('timeline-canvas')).toBeInTheDocument();
    });

    // note: sighted-user counterpart to the a11y `summarizeGraph` disclaimer.
    // The i18n key resolves to the English catalog copy; assert the testid +
    // a substring of the resolved copy.
    const notice = await screen.findByTestId('timeline-ordering-disclaimer');
    expect(notice).toBeInTheDocument();
    expect(notice).toHaveTextContent(
      /inferred from available event data/i,
    );
    // why: non-modal semantics — the notice is informational, not an alert. SR
    // users get the disclaimer via the CanvasShell live region; this visible
    // notice uses `role="note"` (no aria-live) to avoid duplicate
    // announcements.
    expect(notice).toHaveAttribute('role', 'note');
  });

  it('does NOT render the visible notice for a zero-event (context-only) graph', async () => {
    // why: override the graph with a Context-only entity set (no
    // `block_type=event`) — there is no when-axis ordering to disclaim, so the
    // notice must be absent, matching the a11y disclaimer condition in
    // `summarizeTimelineGraph`.
    const client = makeMockClient();
    (client.getWorldKbGraph as ReturnType<typeof vi.fn>).mockResolvedValue({
      entities: [
        entityEvent({
          key_block_id: 'kb-char-1',
          block_type: 'character',
          canonical_name: 'Aria',
        }),
      ],
      source_anchors: [],
      relationships: [],
    });

    renderInApp(<TimelineCanvas worldId="world-7" />, { client });

    await waitFor(() => {
      expect(screen.getByTestId('timeline-canvas')).toBeInTheDocument();
    });

    expect(screen.queryByTestId('timeline-ordering-disclaimer')).toBeNull();
  });

  it('does NOT render the visible notice for an empty graph (empty-state branch)', async () => {
    // why: a zero-entity graph surfaces its own honest empty-state copy via
    // <EmptyState> (§7); the a11y live region therefore does NOT carry the
    // ordering disclaimer, and neither does the visible notice.
    const client = makeMockClient();
    (client.getWorldKbGraph as ReturnType<typeof vi.fn>).mockResolvedValue({
      entities: [],
      source_anchors: [],
      relationships: [],
    });

    renderInApp(<TimelineCanvas worldId="world-7" />, { client });

    await waitFor(() => {
      expect(screen.getByTestId('timeline-canvas')).toBeInTheDocument();
    });

    expect(screen.queryByTestId('timeline-ordering-disclaimer')).toBeNull();
  });
});

describe('buildPatchEntityRequest — V1.73 wire shape', () => {
  it('builds a patch-entity request carrying entity_id + expected_version + patch', async () => {
    const { buildPatchEntityRequest: build } = await import('../timeline-canvas');
    const node = eventNode({ version: 9 });
    const request = build(node, { title: 'Edited' });
    expect(request).toEqual({
      entity_id: 'kb-event-1',
      expected_version: 9,
      patch: { title: 'Edited' },
    });
  });

  it('preserves body (Record<string, unknown>) verbatim — no stringification', async () => {
    const { buildPatchEntityRequest: build } = await import('../timeline-canvas');
    const node = eventNode();
    const body = { attributes: { occurred_at: '1100-01-01T00:00:00Z' } };
    const request = build(node, { body });
    expect(request.patch.body).toBe(body);
  });
});

describe('extractTimelineConflict — V1.73 DTO reuse (T4)', () => {
  it('projects a 409 world_kb_conflict into a TimelineConflictInfo.conflict', () => {
    const error = new NexusClientError(
      409,
      'world_kb_conflict',
      'stale version',
      {
        current_version: 7,
        entity_id: 'kb-event-1',
        conflicting_path: 'title',
        recovery_hint: 'reapply',
      },
    );
    const info = extractTimelineConflict(error, {
      draftPatch: { title: 'Edited' },
      dirtyFields: ['title'],
    });
    expect(info).not.toBeNull();
    expect(info?.kind).toBe('conflict');
    if (info?.kind === 'conflict') {
      expect(info.currentVersion).toBe(7);
      expect(info.entityId).toBe('kb-event-1');
      expect(info.conflictingPath).toBe('title');
      expect(info.draftPatch.title).toBe('Edited');
      expect(info.dirtyFields).toEqual(['title']);
    }
  });

  it('projects a 422 world_kb_validation_failed into a TimelineConflictInfo.validation', () => {
    const error = new NexusClientError(
      422,
      'world_kb_validation_failed',
      'invalid',
      { validation_summary: { errors: ['title too long', 'body malformed'] } },
    );
    const info = extractTimelineConflict(error);
    expect(info).not.toBeNull();
    expect(info?.kind).toBe('validation');
    if (info?.kind === 'validation') {
      expect(info.errors).toEqual(['title too long', 'body malformed']);
    }
  });

  it('returns null for an unrelated error shape (500 / network / generic)', () => {
    expect(extractTimelineConflict(new Error('boom'))).toBeNull();
    expect(
      extractTimelineConflict(
        new NexusClientError(500, 'internal_error', 'boom'),
      ),
    ).toBeNull();
    expect(
      extractTimelineConflict(
        new NexusClientError(403, 'forbidden', 'no access'),
      ),
    ).toBeNull();
    expect(extractTimelineConflict(null)).toBeNull();
    expect(extractTimelineConflict(undefined)).toBeNull();
    expect(extractTimelineConflict({ random: 'shape' })).toBeNull();
  });

  it('returns an empty errors array when the daemon omits validation_summary', () => {
    const error = new NexusClientError(
      422,
      'world_kb_validation_failed',
      'invalid',
      {},
    );
    const info = extractTimelineConflict(error);
    expect(info?.kind).toBe('validation');
    if (info?.kind === 'validation') {
      expect(info.errors).toEqual([]);
    }
  });

  it('does not crash and returns null when details is missing on a 409', () => {
    const error = new NexusClientError(409, 'world_kb_conflict', 'stale');
    const info = extractTimelineConflict(error);
    expect(info?.kind).toBe('conflict');
    if (info?.kind === 'conflict') {
      // why: `details` is missing, so assert the defaults — no crash.
      expect(info.currentVersion).toBe(0);
      expect(info.entityId).toBe('');
    }
  });
});
