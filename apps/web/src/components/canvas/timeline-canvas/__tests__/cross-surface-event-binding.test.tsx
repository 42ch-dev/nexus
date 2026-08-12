/**
 * Cross-surface EVENT binding — V1.163 P1 Task 2 (World → Work deep-link).
 *
 * Upgrades the V1.123 P3 surface-level "View in Work Timeline" CTA to an
 * event-level deep link: when the selected World Narrative event
 * (`block_type=event` KB entity, `key_block_id` = referent) is referenced by
 * a realizing Work outline event (`WorkOutline.timeline_events[].world_event_id`),
 * the CTA navigates to `/works/:workId/timeline?layer=narrative&event=<id>`.
 *
 * Locks pinned here:
 *   - Plan `2026-08-12-v1.163-p1-cross-surface-event-binding.md` Task 2.
 *   - AC-V1163-2 (event-level reverse CTA + URL), AC-V1163-3 (inbound focus —
 *     `?event=<worldEventId>` selects `entity:<id>`; unknown id → no phantom
 *     selection, no hard failure), AC-V1163-4/5 (World side: surface bind
 *     fallback preserves V1.123 `?layer=narrative` CTA; no bind → hidden).
 *   - PD-5 three-state matrix, PD-7 single deterministic reverse target
 *     (most-recently-updated realizing Work, then stable `event_id`).
 *   - Task 4: PD-5 matrix re-pinned TABLE-DRIVEN for both directions, and
 *     AC-V1163-7 (no write path — the inspector exposes no control to
 *     set/clear `world_event_id`; CTA clicks navigate and never emit a patch).
 *
 * Two layers of coverage:
 *   1. Inspector-level (mirrors `cross-surface-nav.test.tsx`) — the CTA slot
 *      wiring: presence / fallback / hide.
 *   2. Orchestrator-level (mocked daemon) — the composed reverse resolve
 *      (works list → detail fan-out → outline scan) and the real navigation
 *      URL, plus the inbound `?event=` focus on the World Timeline.
 */
import { afterEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { http, HttpResponse } from 'msw';
import type { MutableRefObject } from 'react';
import { MemoryRouter, Route, Routes, useLocation } from 'react-router';
import type { Node } from '@xyflow/react';

import { renderInApp } from '@/test/test-providers';
import { useHandlers } from '@/test/msw-server';
import { BrowserClient } from '@/lib/nexus';
import type { WorkOutline } from '@42ch/nexus-contracts';
import type { WorldKbEntityProjection } from '@42ch/nexus-contracts';

import { TimelineInspector } from '../timeline-inspector';
import { TimelineCanvas } from '../timeline-canvas';
import type {
  TimelineCanvasAdapterContext,
  TimelineNodeData,
} from '../timeline-canvas-adapter';

// ─── Fixtures ───────────────────────────────────────────────────────────────

const KB_EVENT_ID = 'kb-evt-1';

function kbEntity(
  overrides: Partial<WorldKbEntityProjection> &
    Pick<WorldKbEntityProjection, 'key_block_id' | 'block_type' | 'canonical_name'>,
): WorldKbEntityProjection {
  return {
    world_id: 'world-7',
    status: 'confirmed',
    version: 1,
    ...overrides,
  } as WorldKbEntityProjection;
}

const KB_EVENT = kbEntity({
  key_block_id: KB_EVENT_ID,
  block_type: 'event',
  canonical_name: 'Coronation',
  body: { attributes: { occurred_at: '1042-spring' } },
});

const KB_CONTEXT = kbEntity({
  key_block_id: 'kb-char-1',
  block_type: 'character',
  canonical_name: 'Aria',
});

function workSummary(workId: string, updatedAt: string) {
  return {
    work_id: workId,
    title: `Work ${workId}`,
    status: 'active',
    intake_status: 'complete',
    primary_preset_id: 'preset-a',
    updated_at: updatedAt,
  };
}

function workDetail(workId: string, worldId: string | null) {
  return {
    work_id: workId,
    status: 'active',
    title: `Work ${workId}`,
    long_term_goal: '',
    initial_idea: '',
    intake_status: 'complete',
    world_id: worldId ?? undefined,
    inspiration_log: [],
    primary_preset_id: 'preset-a',
    schedule_ids: [],
    created_at: '2026-01-01T00:00:00Z',
    updated_at: '2026-01-01T00:00:00Z',
    current_stage: 'draft',
    stage_status: 'active',
    auto_chain_enabled: false,
    auto_chain_interrupted: false,
    auto_review_master_on_timeout: false,
    total_planned_chapters: 0,
    current_chapter: 0,
  };
}

function outline(workId: string, timelineEvents: WorkOutline['timeline_events']): WorkOutline {
  return {
    work_id: workId,
    outline_revision: 1,
    volumes: [],
    timeline_events: timelineEvents,
    foreshadows: [],
    chapter_titles: {},
    updated_at: '2026-08-01T00:00:00Z',
  };
}

// ─── Inspector-level fixtures (mirrors cross-surface-nav.test.tsx) ──────────

function timelineEventNode(
  overrides: Partial<TimelineNodeData> = {},
): Node<TimelineNodeData> {
  return {
    id: `entity:${KB_EVENT_ID}`,
    type: 'timeline-event',
    position: { x: 0, y: 0 },
    data: {
      key_block_id: KB_EVENT_ID,
      world_id: 'world-7',
      block_type: 'event',
      canonical_name: 'Coronation',
      status: 'confirmed',
      version: 1,
      sequence_no: 1,
      body: { attributes: { occurred_at: '1042-spring' } },
      source_anchor_count: 0,
      layoutHint: 'event',
      occurredAtHint: '1042-spring',
      ...overrides,
    } as TimelineNodeData,
  };
}

function ctxRefWith(
  overrides: Partial<TimelineCanvasAdapterContext> = {},
): MutableRefObject<TimelineCanvasAdapterContext> {
  return {
    current: {
      worldId: 'world-7',
      ...overrides,
    },
  };
}

// ─── Orchestrator-level journey (mocked daemon) ─────────────────────────────

function crossSurfaceJourney(
  over: {
    kbEntities?: WorldKbEntityProjection[];
    works?: ReturnType<typeof workSummary>[];
    details?: Record<string, { world_id: string | null }>;
    outlines?: Record<string, WorkOutline>;
  } = {},
) {
  // AC-V1163-7 write-path recorder — any patch reaching the World Timeline's
  // legitimate entity write (`worldKbPatchEntity`) or the Work timeline patch
  // surface (`patchTimelineEvent`) is recorded. The Task 4 no-write tests
  // assert this array stays empty after CTA navigation, pinning that the
  // cross-surface affordance is navigation-only.
  const writes: string[] = [];
  const handlers = [
    http.get('/v1/daemon/worlds/:worldId/kb/graph', () =>
      HttpResponse.json({
        entities: over.kbEntities ?? [KB_EVENT, KB_CONTEXT],
        source_anchors: [],
        relationships: [],
      }),
    ),
    http.get('/v1/daemon/worlds/:worldId/timeline/events', () =>
      HttpResponse.json({ items: [], has_more: false, next_cursor: undefined }),
    ),
    http.get('/v1/daemon/works', () =>
      HttpResponse.json({
        items: over.works ?? [],
        pagination: { limit: 20, has_more: false },
      }),
    ),
    http.get('/v1/daemon/works/:workId', ({ params }) => {
      const detail = (over.details ?? {})[String(params.workId)];
      return HttpResponse.json(
        detail ? workDetail(String(params.workId), detail.world_id) : workDetail(String(params.workId), null),
      );
    }),
    http.get('/v1/daemon/works/:workId/outline', ({ params }) => {
      const o = (over.outlines ?? {})[String(params.workId)];
      return HttpResponse.json(o ?? outline(String(params.workId), []));
    }),
    http.get('/v1/daemon/compute/modules', () =>
      HttpResponse.json({ items: [], has_more: false }),
    ),
    http.get('/v1/daemon/narrative/worlds', () =>
      HttpResponse.json({
        worlds: [{ world_id: 'world-7', title: 'Test World' }],
      }),
    ),
    // Write surfaces are expected to stay untouched in every test here —
    // recording them lets the no-write assertions observe any regression.
    http.post('/v1/daemon/worlds/:worldId/kb/patch-entity', () => {
      writes.push('worldKbPatchEntity');
      return HttpResponse.json({ error: 'unexpected write' }, { status: 409 });
    }),
    http.post('/v1/daemon/works/:workId/timeline/patch', () => {
      writes.push('patchTimelineEvent');
      return HttpResponse.json({ error: 'unexpected write' }, { status: 409 });
    }),
  ];
  return { handlers, writes };
}

function CrossSurfaceAppRoutes() {
  const location = useLocation();
  return (
    <>
      <Routes location={location}>
        <Route
          path="worlds/:worldId/timeline"
          element={<TimelineCanvas worldId="world-7" />}
        />
        <Route path="*" element={<div data-testid="fallback-route" />} />
      </Routes>
      <div data-testid="location-probe">{`${location.pathname}${location.search}`}</div>
    </>
  );
}

function renderCrossSurfaceApp(
  over: Parameters<typeof crossSurfaceJourney>[0] = {},
  initial = ['/worlds/world-7/timeline'],
): { writes: string[] } {
  const { handlers, writes } = crossSurfaceJourney(over);
  useHandlers(...handlers);
  renderInApp(<CrossSurfaceAppRoutes />, {
    client: new BrowserClient(),
    initialRouterEntries: initial,
  });
  return { writes };
}

/** Select the KB event row via the sortable alt-view (established pattern). */
async function selectEventRow(user: ReturnType<typeof userEvent.setup>) {
  await user.click(await screen.findByRole('button', { name: 'Show list view' }));
  const row = await screen.findByText('Coronation');
  fireEvent.click(row.closest('tr')!);
  await screen.findByTestId('timeline-inspector-title');
}

afterEach(() => {
  vi.restoreAllMocks();
});

// ─── Inspector-level: CTA slot wiring ───────────────────────────────────────

describe('V1.163 Task 2 — World inspector CTA slots (event-level)', () => {
  it('renders "View in Work Timeline" with the event id when boundWorkEventId is supplied', () => {
    render(
      <MemoryRouter>
        <TimelineInspector
          node={timelineEventNode()}
          ctxRef={ctxRefWith({
            boundWorkId: 'work-1',
            boundWorkEventId: 'evt-work-1',
            onViewInWorkTimeline: () => undefined,
          })}
        />
      </MemoryRouter>,
    );

    const cta = screen.queryByTestId('timeline-view-in-work-timeline');
    expect(cta).not.toBeNull();
    expect(cta).toHaveTextContent('View in Work Timeline');
    // The CTA target carries the event id when an event-level match exists
    // (PD-5 state 1: deep-link `?layer=narrative&event=…`).
    expect(cta).toHaveAttribute('data-event-id', 'evt-work-1');
  });

  it('renders the CTA WITHOUT an event id when only the surface bind exists (V1.123 fallback)', () => {
    // PD-5 state 2: surface bind only → the CTA stays but carries no event id
    // (the orchestrator falls back to `?layer=narrative`).
    render(
      <MemoryRouter>
        <TimelineInspector
          node={timelineEventNode()}
          ctxRef={ctxRefWith({
            boundWorkId: 'work-1',
            onViewInWorkTimeline: () => undefined,
          })}
        />
      </MemoryRouter>,
    );

    const cta = screen.queryByTestId('timeline-view-in-work-timeline');
    expect(cta).not.toBeNull();
    expect(cta).not.toHaveAttribute('data-event-id');
  });

  it('hides the CTA when no realizing Work is bound (even with an orphan event id)', () => {
    // An event id without a bound Work is not a valid bind — never surface a
    // dead CTA (honest scope cut).
    render(
      <MemoryRouter>
        <TimelineInspector
          node={timelineEventNode()}
          ctxRef={ctxRefWith({
            boundWorkEventId: 'evt-work-1',
          })}
        />
      </MemoryRouter>,
    );

    expect(screen.queryByTestId('timeline-view-in-work-timeline')).toBeNull();
  });

  it('does NOT render the affordance on context (non-event) nodes even with an event bind', () => {
    const contextNode = timelineEventNode({
      key_block_id: 'kb-char-1',
      block_type: 'character',
      canonical_name: 'Aria',
      layoutHint: 'context',
    });
    delete (contextNode.data as Partial<TimelineNodeData>).occurredAtHint;

    render(
      <MemoryRouter>
        <TimelineInspector
          node={contextNode}
          ctxRef={ctxRefWith({
            boundWorkId: 'work-1',
            boundWorkEventId: 'evt-work-1',
            onViewInWorkTimeline: () => undefined,
          })}
        />
      </MemoryRouter>,
    );

    expect(screen.queryByTestId('timeline-view-in-work-timeline')).toBeNull();
  });

  it('does NOT render the affordance on compute (non-event family) nodes', () => {
    const computeNode = timelineEventNode({
      key_block_id: 'log:evt-compute-1',
      canonical_name: 'Compute result',
      layoutHint: 'compute',
    });
    delete (computeNode.data as Partial<TimelineNodeData>).occurredAtHint;

    render(
      <MemoryRouter>
        <TimelineInspector
          node={computeNode}
          ctxRef={ctxRefWith({
            boundWorkId: 'work-1',
            boundWorkEventId: 'evt-work-1',
            onViewInWorkTimeline: () => undefined,
          })}
        />
      </MemoryRouter>,
    );

    expect(screen.queryByTestId('timeline-view-in-work-timeline')).toBeNull();
  });
});

// ─── Orchestrator-level: reverse resolve + navigation ───────────────────────

describe('V1.163 Task 2 — World → Work event deep-link (AC-V1163-2/4/5, PD-5/7)', () => {
  it('AC-V1163-2: matching Work outline world_event_id → CTA click URL includes event=<workEventId>', async () => {
    const user = userEvent.setup();
    renderCrossSurfaceApp({
      works: [workSummary('work-1', '2026-08-02T00:00:00Z')],
      details: { 'work-1': { world_id: 'world-7' } },
      outlines: {
        'work-1': outline('work-1', [
          { event_id: 'evt-work-1', title: 'Coronation beat', world_event_id: KB_EVENT_ID },
        ]),
      },
    });

    await selectEventRow(user);
    const cta = await screen.findByTestId('timeline-view-in-work-timeline');
    expect(cta).toHaveAttribute('data-event-id', 'evt-work-1');

    fireEvent.click(cta);

    await waitFor(() =>
      expect(screen.getByTestId('location-probe').textContent).toBe(
        '/works/work-1/timeline?layer=narrative&event=evt-work-1',
      ),
    );
  });

  it('PD-5 (AC-V1163-4): surface bind only → CTA stays, URL has layer=narrative and NO event', async () => {
    const user = userEvent.setup();
    renderCrossSurfaceApp({
      works: [workSummary('work-1', '2026-08-02T00:00:00Z')],
      details: { 'work-1': { world_id: 'world-7' } },
      // Realizing Work's outline event does NOT reference the selected World
      // event → event-level match absent, V1.123 surface fallback applies.
      outlines: {
        'work-1': outline('work-1', [
          { event_id: 'evt-work-1', title: 'Coronation beat' },
        ]),
      },
    });

    await selectEventRow(user);
    const cta = await screen.findByTestId('timeline-view-in-work-timeline');
    expect(cta).not.toHaveAttribute('data-event-id');

    fireEvent.click(cta);

    await waitFor(() =>
      expect(screen.getByTestId('location-probe').textContent).toBe(
        '/works/work-1/timeline?layer=narrative',
      ),
    );
  });

  it('AC-V1163-5: no realizing Work → CTA hidden (honest scope cut)', async () => {
    const user = userEvent.setup();
    // Zero Works at all.
    renderCrossSurfaceApp({ works: [] });

    await selectEventRow(user);
    await waitFor(() =>
      expect(screen.queryByTestId('timeline-view-in-work-timeline')).toBeNull(),
    );
  });

  it('AC-V1163-5: realizing Works bind a different World → CTA hidden', async () => {
    const user = userEvent.setup();
    renderCrossSurfaceApp({
      works: [workSummary('work-9', '2026-08-02T00:00:00Z')],
      details: { 'work-9': { world_id: 'world-other' } },
      outlines: {
        'work-9': outline('work-9', [
          { event_id: 'evt-work-9', title: 'Beat', world_event_id: KB_EVENT_ID },
        ]),
      },
    });

    await selectEventRow(user);
    await waitFor(() =>
      expect(screen.queryByTestId('timeline-view-in-work-timeline')).toBeNull(),
    );
  });

  it('PD-7: multi-match across Works → most-recently-updated realizing Work wins', async () => {
    const user = userEvent.setup();
    renderCrossSurfaceApp({
      works: [
        workSummary('work-old', '2026-01-01T00:00:00Z'),
        workSummary('work-new', '2026-08-01T00:00:00Z'),
      ],
      details: {
        'work-old': { world_id: 'world-7' },
        'work-new': { world_id: 'world-7' },
      },
      outlines: {
        'work-old': outline('work-old', [
          { event_id: 'evt-old', title: 'Old beat', world_event_id: KB_EVENT_ID },
        ]),
        'work-new': outline('work-new', [
          { event_id: 'evt-new', title: 'New beat', world_event_id: KB_EVENT_ID },
        ]),
      },
    });

    await selectEventRow(user);
    const cta = await screen.findByTestId('timeline-view-in-work-timeline');
    expect(cta).toHaveAttribute('data-event-id', 'evt-new');

    fireEvent.click(cta);

    await waitFor(() =>
      expect(screen.getByTestId('location-probe').textContent).toBe(
        '/works/work-new/timeline?layer=narrative&event=evt-new',
      ),
    );
  });

  it('PD-7: same updated_at → stable event_id tiebreak (lexicographic first)', async () => {
    const user = userEvent.setup();
    const same = '2026-08-01T00:00:00Z';
    renderCrossSurfaceApp({
      works: [workSummary('work-a', same), workSummary('work-b', same)],
      details: {
        'work-a': { world_id: 'world-7' },
        'work-b': { world_id: 'world-7' },
      },
      outlines: {
        'work-a': outline('work-a', [
          { event_id: 'evt-zz', title: 'A beat', world_event_id: KB_EVENT_ID },
        ]),
        'work-b': outline('work-b', [
          { event_id: 'evt-aa', title: 'B beat', world_event_id: KB_EVENT_ID },
        ]),
      },
    });

    await selectEventRow(user);
    const cta = await screen.findByTestId('timeline-view-in-work-timeline');
    expect(cta).toHaveAttribute('data-event-id', 'evt-aa');
  });
});

// ─── Orchestrator-level: inbound event focus (AC-V1163-3, World side) ───────

describe('V1.163 Task 2 — inbound `?event=` focus on the World Timeline (AC-V1163-3)', () => {
  it('selects the React Flow node entity:<id> when the event param matches a projected event', async () => {
    renderCrossSurfaceApp(
      {},
      ['/worlds/world-7/timeline?layer=narrative&event=' + KB_EVENT_ID],
    );

    // Selection drives the inspector (existing shell behavior — no new focus
    // primitive). The deep-linked event's inspector opening is the observable
    // proof that `entity:kb-evt-1` got selected.
    await screen.findByTestId('timeline-inspector-title');
    expect(screen.getByTestId('timeline-inspector-title')).toBeInTheDocument();
  });

  it('unknown event id → no phantom selection, no hard failure (Narrative layer still renders)', async () => {
    renderCrossSurfaceApp(
      {},
      ['/worlds/world-7/timeline?layer=narrative&event=kb-unknown'],
    );

    // Canvas renders normally (no error state).
    await screen.findByTestId('timeline-canvas');
    await waitFor(() =>
      expect(screen.getByTestId('timeline-canvas')).toHaveAttribute(
        'data-active-layer',
        'narrative',
      ),
    );
    // The unknown id must NOT fabricate a selection (no inspector opens).
    expect(screen.queryByTestId('timeline-inspector-title')).toBeNull();
  });
});

// ─── Task 4: PD-5 three-state matrix, table-driven (World → Work) ───────────

/**
 * PD-5 honest scope cut — same CTA chrome, three states. World → Work
 * direction: surface bind = realizing Work (`boundWorkId`), event bind =
 * matching outline `world_event_id` (`boundWorkEventId`). Expected CTA +
 * URL contract per the plan's Task 4 table.
 */
type WorldPd5MatrixState = {
  state: string;
  surfaceBind: boolean;
  eventBind: boolean;
  cta: 'hidden' | 'shown';
  eventId?: string;
};

const WORLD_PD5_MATRIX: WorldPd5MatrixState[] = [
  { state: 'no surface bind, no event bind', surfaceBind: false, eventBind: false, cta: 'hidden' },
  { state: 'surface bind only (V1.123 fallback)', surfaceBind: true, eventBind: false, cta: 'shown' },
  { state: 'surface + event bind (deep-link)', surfaceBind: true, eventBind: true, cta: 'shown', eventId: 'evt-work-1' },
];

/** Orchestrator-level journey inputs for the same three states. */
type WorldPd5MatrixJourney = {
  state: string;
  works: ReturnType<typeof workSummary>[];
  details: Record<string, { world_id: string | null }>;
  outlines: Record<string, WorkOutline>;
  cta: 'hidden' | 'shown';
  expectedUrl?: string;
};

const WORLD_PD5_JOURNEYS: WorldPd5MatrixJourney[] = [
  {
    state: 'no surface bind → CTA hidden, no navigation',
    works: [],
    details: {},
    outlines: {},
    cta: 'hidden',
  },
  {
    state: 'surface bind only → ?layer=narrative (no event)',
    works: [workSummary('work-1', '2026-08-02T00:00:00Z')],
    details: { 'work-1': { world_id: 'world-7' } },
    outlines: {
      'work-1': outline('work-1', [
        { event_id: 'evt-work-1', title: 'Coronation beat' },
      ]),
    },
    cta: 'shown',
    expectedUrl: '/works/work-1/timeline?layer=narrative',
  },
  {
    state: 'surface + event bind → ?layer=narrative&event=<id>',
    works: [workSummary('work-1', '2026-08-02T00:00:00Z')],
    details: { 'work-1': { world_id: 'world-7' } },
    outlines: {
      'work-1': outline('work-1', [
        { event_id: 'evt-work-1', title: 'Coronation beat', world_event_id: KB_EVENT_ID },
      ]),
    },
    cta: 'shown',
    expectedUrl: '/works/work-1/timeline?layer=narrative&event=evt-work-1',
  },
];

describe('V1.163 Task 4 — PD-5 three-state matrix, World → Work (table-driven)', () => {
  it.each(WORLD_PD5_MATRIX)(
    'inspector: $state → CTA $cta',
    ({ surfaceBind, eventBind, cta, eventId }) => {
      render(
        <MemoryRouter>
          <TimelineInspector
            node={timelineEventNode()}
            ctxRef={ctxRefWith({
              ...(surfaceBind ? { boundWorkId: 'work-1' } : {}),
              ...(eventBind ? { boundWorkEventId: 'evt-work-1' } : {}),
              ...(surfaceBind ? { onViewInWorkTimeline: () => undefined } : {}),
            })}
          />
        </MemoryRouter>,
      );

      const ctaEl = screen.queryByTestId('timeline-view-in-work-timeline');
      if (cta === 'hidden') {
        expect(ctaEl).toBeNull();
        return;
      }
      expect(ctaEl).not.toBeNull();
      if (eventId) {
        expect(ctaEl).toHaveAttribute('data-event-id', eventId);
      } else {
        expect(ctaEl).not.toHaveAttribute('data-event-id');
      }
    },
  );

  it.each(WORLD_PD5_JOURNEYS)(
    'orchestrator: $state',
    async ({ state: _state, works, details, outlines, cta, expectedUrl }) => {
      const user = userEvent.setup();
      const { writes } = renderCrossSurfaceApp({ works, details, outlines });

      await selectEventRow(user);

      if (cta === 'hidden') {
        await waitFor(() =>
          expect(screen.queryByTestId('timeline-view-in-work-timeline')).toBeNull(),
        );
        // No navigation happened; no write op fired.
        expect(screen.getByTestId('location-probe').textContent).toBe('/worlds/world-7/timeline');
        expect(writes).toEqual([]);
        return;
      }

      const ctaEl = await screen.findByTestId('timeline-view-in-work-timeline');
      fireEvent.click(ctaEl);

      await waitFor(() =>
        expect(screen.getByTestId('location-probe').textContent).toBe(expectedUrl),
      );
      // AC-V1163-7: the CTA navigates — it never emits a patch/bind write.
      expect(writes).toEqual([]);
    },
  );
});

// ─── Task 4: AC-V1163-7 — no write path for world_event_id (World side) ─────

describe('V1.163 Task 4 — AC-V1163-7: no write path for world_event_id (World side)', () => {
  it('inspector exposes no control to edit world_event_id on a fully bound event node', () => {
    const { container } = render(
      <MemoryRouter>
        <TimelineInspector
          node={timelineEventNode()}
          ctxRef={ctxRefWith({
            boundWorkId: 'work-1',
            boundWorkEventId: 'evt-work-1',
            onViewInWorkTimeline: () => undefined,
            onPatchEntity: () => undefined,
          })}
        />
      </MemoryRouter>,
    );

    // The ONLY editable fields are title + body (the pre-existing entity
    // patch surface). Block type renders read-only. No field — editable or
    // not — references the bind carrier.
    const editable = Array.from(
      container.querySelectorAll<HTMLInputElement | HTMLTextAreaElement>(
        'input:not([readonly]):not([type="hidden"]), textarea',
      ),
    );
    expect(editable.map((el) => el.id)).toEqual(['tl-title', 'tl-body']);

    for (const el of container.querySelectorAll('input, textarea, select')) {
      expect(el.getAttribute('id') ?? '').not.toMatch(/world[-_]?event/i);
      expect(el.getAttribute('name') ?? '').not.toMatch(/world[-_]?event/i);
    }
    expect(screen.queryByLabelText(/world event/i)).toBeNull();

    // The cross-surface affordance is a navigation button, not a form submit.
    expect(screen.getByTestId('timeline-view-in-work-timeline')).toHaveAttribute('type', 'button');
  });

  it('the entity patch write path cannot carry world_event_id (emitted patch has no bind key)', async () => {
    const user = userEvent.setup();
    const onPatchEntity = vi.fn();
    render(
      <MemoryRouter>
        <TimelineInspector node={timelineEventNode()} ctxRef={ctxRefWith({ onPatchEntity })} />
      </MemoryRouter>,
    );

    const titleInput = screen.getByDisplayValue('Coronation');
    await user.clear(titleInput);
    await user.type(titleInput, 'Coronation of Aria');
    fireEvent.click(screen.getByTestId('timeline-inspector-save'));

    await waitFor(() => expect(onPatchEntity).toHaveBeenCalledTimes(1));
    const [, patch, dirtyFields] = onPatchEntity.mock.calls[0];
    expect(dirtyFields).toEqual(['title']);
    expect(patch).not.toHaveProperty('world_event_id');
  });

  it('clicking the cross-surface CTA navigates — it never emits a patch (no write path)', () => {
    const onPatchEntity = vi.fn();
    const onViewInWorkTimeline = vi.fn();
    render(
      <MemoryRouter>
        <TimelineInspector
          node={timelineEventNode()}
          ctxRef={ctxRefWith({
            boundWorkId: 'work-1',
            boundWorkEventId: 'evt-work-1',
            onViewInWorkTimeline,
            onPatchEntity,
          })}
        />
      </MemoryRouter>,
    );

    fireEvent.click(screen.getByTestId('timeline-view-in-work-timeline'));
    expect(onViewInWorkTimeline).toHaveBeenCalledTimes(1);
    expect(onPatchEntity).not.toHaveBeenCalled();
  });
});
