/**
 * Verifies the Work-Brief projection contract locked by
 * `canvas-strategy-surface.md` §3.3.3 V1.156 amendment + plan
 * `2026-08-10-v1.156-p2-work-timeline-brief-layer.md`:
 *
 *   - `WorkTimelineLayer` includes `'brief'` (UI-only union type — NOT a
 *     wire DTO).
 *   - `projectWorkTimelineGraph(graph, 'brief', fixture?, boundWorldGraph)`
 *     projects the bound World's `WorldKbGraphResponse.entities[block_type=era]`
 *     onto the Work Timeline Brief when-axis as `timeline-brief-era` nodes —
 *     the World Timeline Brief projection reused VERBATIM (Work-Brief feel
 *     ≡ World-Brief feel; same `TimelineNodeData` with `layoutHint: 'brief'`,
 *     same era markers, same directed-axis spine). Non-era entities are
 *     EXCLUDED from the Brief layer.
 *   - Honest empty-state: no bound World graph (undefined) or no era
 *     entities → zero nodes.
 *   - `createWorkTimelineCanvasAdapter(ctxRef, 'brief', boundWorldGraph)`
 *     makes `projectGraph` delegate to the Brief layer; the default layer
 *     stays `'narrative'` (architect UX-risk override §7.3, preserved in
 *     V1.156). The Brief era node type is registered alongside the
 *     Narrative/Moment node types.
 *   - Narrative + Moment projection behavior is unchanged (regression-safe).
 *
 * V1.160 P2 T1 (plan `2026-08-11-v1.160-p2-work-brief-inheritance-and-tracker-hygiene.md`):
 *
 *   - The adapter exposes the `renderBriefTimeBands` time-band slot
 *     (mirror of the World Timeline's V1.159 slot). It builds the era
 *     forest from the bound World's KB graph (`buildEraTree` — the SAME
 *     graph read the Brief projection consumes, NOT the Work outline
 *     graph) and renders `<BriefTimeBands />`, the vertical time-band
 *     panel that supersedes the V1.156 flat era sweep on the Work Brief
 *     layer. Work-Brief is read-only (PD-2) — no `onSelectEra` hand-off.
 *   - The Work canvas renders `surface.briefTimeBands` when the Brief
 *     layer is active + the bound World has era entities (canvas-level
 *     mount, mirroring the World Timeline's Brief panel).
 *
 * `wire_contracts_changed: false` — frontend-only. The Brief projection
 * reuses the V1.73 `GET /worlds/{id}/kb/graph` route + V1.123 Brief carrier
 * (`WorldKbGraphResponse.entities[block_type=era]`) verbatim; the
 * `WorkTimelineLayer` union extension is a UI-only type, not a wire DTO.
 */
import { describe, expect, it, vi } from 'vitest';
import { screen, waitFor, within } from '@testing-library/react';
import type { Node } from '@xyflow/react';

import type {
  WorldKbEntityProjection,
  WorldKbGraphResponse,
  WorkOutline,
} from '@42ch/nexus-contracts';

import type { NexusClient } from '@/lib/nexus';
import { renderInApp } from '@/test/test-providers';
import type { SceneBeatFixturePayload } from '../../outline-canvas/graph-projection';
import { WorkTimelineCanvas } from '../work-timeline-canvas';
import {
  createWorkTimelineCanvasAdapter,
  projectWorkTimelineGraph,
  type WorkTimelineCanvasAdapterContext,
  type WorkTimelineLayer,
  type WorkTimelineNodeData,
} from '../work-timeline-canvas-adapter';

function outline(overrides: Partial<WorkOutline> = {}): WorkOutline {
  return {
    work_id: 'work-1',
    outline_revision: 1,
    volumes: [],
    timeline_events: [],
    foreshadows: [],
    scenes: [],
    beats: [],
    chapter_titles: {},
    updated_at: '2026-07-18T00:00:00Z',
    ...overrides,
  } as WorkOutline;
}

function entity(
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

function eraEntity(
  overrides: Partial<WorldKbEntityProjection> &
    Pick<WorldKbEntityProjection, 'key_block_id' | 'canonical_name'>,
): WorldKbEntityProjection {
  const { key_block_id, canonical_name, body, ...rest } = overrides;
  return entity({
    key_block_id,
    block_type: 'era',
    canonical_name,
    body: body ?? {
      attributes: {
        era_id: 'era-1',
        start_hint: '1000-01-01T00:00:00Z',
        end_hint: '1100-01-01T00:00:00Z',
        world_summary: 'The First Age',
      },
    },
    ...rest,
  });
}

function worldGraph(
  entities: WorldKbEntityProjection[],
  relationships: WorldKbGraphResponse['relationships'] = [],
): WorldKbGraphResponse {
  return { entities, source_anchors: [], relationships };
}

function parentEraRel(
  sourceEntityId: string,
  targetEntityId: string,
  index: number,
): WorldKbGraphResponse['relationships'][number] {
  return {
    relationship_id: `rel-${index}`,
    world_id: 'world-7',
    source_entity_id: sourceEntityId,
    target_entity_id: targetEntityId,
    relation_type: 'custom',
    custom_label: 'parent_era',
    symmetric: false,
    source_anchor_ids: [],
    needs_review: false,
    source: 'manual',
    version: 1,
    updated_at: '2026-08-11T00:00:00Z',
    projection_direction: 'stored',
  };
}

/** Locate one time band by era id (mirrors the World brief-time-bands tests). */
function bandOf(container: HTMLElement, eraId: string): HTMLElement {
  const el = container.querySelector(
    `[data-era-id="${eraId}"][data-testid="brief-time-band"]`,
  );
  if (!(el instanceof HTMLElement)) {
    throw new Error(`band for era ${eraId} not found`);
  }
  return el;
}

function makeMockClient(): NexusClient {
  return {
    getWorkOutline: vi.fn(),
    patchOutlineStructure: vi.fn(),
    patchOutlineChapter: vi.fn(),
    patchTimelineEvent: vi.fn(),
    health: vi.fn().mockResolvedValue({ status: 'ok', version: 'test' }),
  } as unknown as NexusClient;
}

function makeContext(
  overrides: Partial<WorkTimelineCanvasAdapterContext> = {},
): WorkTimelineCanvasAdapterContext {
  return {
    workId: 'work-1',
    client: makeMockClient(),
    ...overrides,
  };
}

describe('WorkTimelineLayer — V1.156 P2 T1 union extension', () => {
  it("includes 'brief' alongside 'narrative' | 'moment' (UI-only union — not a wire DTO)", () => {
    const layers: WorkTimelineLayer[] = ['brief', 'narrative', 'moment'];
    expect(layers).toContain('brief');
    expect(projectWorkTimelineGraph(outline(), 'brief')).toEqual({
      nodes: [],
      edges: [],
    });
  });
});

describe('WorkTimelineCanvasAdapter.projectGraphForLayer — Brief projection (bound World era entities)', () => {
  it("filters the bound World's entities to block_type='era' for the Brief layer", () => {
    const era = eraEntity({
      key_block_id: 'kb-era-1',
      canonical_name: 'The First Age',
    });
    const event = entity({
      key_block_id: 'kb-event-1',
      block_type: 'event',
      canonical_name: 'Coronation',
    });
    const character = entity({
      key_block_id: 'kb-char-1',
      block_type: 'character',
      canonical_name: 'Aria',
    });

    const { nodes } = projectWorkTimelineGraph(
      outline(),
      'brief',
      undefined,
      worldGraph([era, event, character]),
    );

    expect(nodes).toHaveLength(2);
    const node = nodes[0] as Node<WorkTimelineNodeData>;
    expect(node.id).toBe('entity:kb-era-1');
    expect(node.type).toBe('timeline-brief-era');
    expect(node.data.layoutHint).toBe('brief');
    expect(node.data.canonical_name).toBe('The First Age');
  });

  it('excludes event and non-era entities from the Brief layer', () => {
    const event = entity({
      key_block_id: 'kb-event-1',
      block_type: 'event',
      canonical_name: 'Coronation',
    });
    const character = entity({
      key_block_id: 'kb-char-1',
      block_type: 'character',
      canonical_name: 'Aria',
    });

    const { nodes } = projectWorkTimelineGraph(
      outline(),
      'brief',
      undefined,
      worldGraph([event, character]),
    );

    expect(nodes).toEqual([]);
  });

  it('emits honest empty-state when no bound World graph is supplied (zero nodes)', () => {
    expect(projectWorkTimelineGraph(outline(), 'brief')).toEqual({
      nodes: [],
      edges: [],
    });
    expect(projectWorkTimelineGraph(outline(), 'brief', undefined, undefined)).toEqual({
      nodes: [],
      edges: [],
    });
  });

  it('positions era nodes horizontally (LR) sorted by body.attributes.start_hint', () => {
    const earlier = eraEntity({
      key_block_id: 'kb-era-early',
      canonical_name: 'The First Age',
      body: {
        attributes: {
          era_id: 'era-first',
          start_hint: '1000-01-01T00:00:00Z',
          end_hint: '1100-01-01T00:00:00Z',
        },
      },
    });
    const later = eraEntity({
      key_block_id: 'kb-era-late',
      canonical_name: 'The Second Age',
      body: {
        attributes: {
          era_id: 'era-second',
          start_hint: '1100-01-01T00:00:00Z',
          end_hint: '1200-01-01T00:00:00Z',
        },
      },
    });

    const { nodes } = projectWorkTimelineGraph(
      outline(),
      'brief',
      undefined,
      worldGraph([later, earlier]),
    );

    const early = nodes.find((n) => n.id === 'entity:kb-era-early')!;
    const late = nodes.find((n) => n.id === 'entity:kb-era-late')!;
    expect(early.position.x).toBeLessThan(late.position.x);
    expect(early.position.y).toBe(0);
    expect(late.position.y).toBe(0);
  });

  it('clusters eras without start_hint in a temporal-unknown group off the when-axis', () => {
    const undated = eraEntity({
      key_block_id: 'kb-era-undated',
      canonical_name: 'The Forgotten Age',
      body: { attributes: { era_id: 'era-forgotten' } },
    });
    const dated = eraEntity({
      key_block_id: 'kb-era-dated',
      canonical_name: 'The First Age',
      body: {
        attributes: {
          era_id: 'era-first',
          start_hint: '1000-01-01T00:00:00Z',
        },
      },
    });

    const { nodes } = projectWorkTimelineGraph(
      outline(),
      'brief',
      undefined,
      worldGraph([undated, dated]),
    );

    const undatedNode = nodes.find((n) => n.id === 'entity:kb-era-undated')!;
    const datedNode = nodes.find((n) => n.id === 'entity:kb-era-dated')!;
    expect(undatedNode.data.startHint).toBeUndefined();
    expect(datedNode.data.startHint).toBe('1000-01-01T00:00:00Z');
    expect(undatedNode.position.y).toBeGreaterThan(0);
    expect(datedNode.position.y).toBe(0);
  });

  it('carries era markers (eraId, startHint, endHint, worldSummary) extracted from body.attributes', () => {
    const era = eraEntity({
      key_block_id: 'kb-era-1',
      canonical_name: 'The First Age',
      body: {
        attributes: {
          era_id: 'era-first',
          start_hint: '1000-01-01T00:00:00Z',
          end_hint: '1100-01-01T00:00:00Z',
          world_summary: 'A time of myth and legend.',
        },
      },
    });

    const { nodes } = projectWorkTimelineGraph(
      outline(),
      'brief',
      undefined,
      worldGraph([era]),
    );
    const node = nodes.find((n) => n.id === 'entity:kb-era-1') as Node<WorkTimelineNodeData>;

    expect(node.data.layoutHint).toBe('brief');
    expect(node.data.eraId).toBe('era-first');
    expect(node.data.startHint).toBe('1000-01-01T00:00:00Z');
    expect(node.data.endHint).toBe('1100-01-01T00:00:00Z');
    expect(node.data.worldSummary).toBe('A time of myth and legend.');
  });

  it('omits directedAxisSpine when all eras lack start_hint; emits it when dated eras exist', () => {
    const undatedA = eraEntity({
      key_block_id: 'kb-era-undated-a',
      canonical_name: 'The Forgotten Age',
      body: { attributes: { era_id: 'era-forgotten-a' } },
    });
    const undatedB = eraEntity({
      key_block_id: 'kb-era-undated-b',
      canonical_name: 'The Lost Age',
      body: { attributes: { era_id: 'era-forgotten-b' } },
    });

    const undatedResult = projectWorkTimelineGraph(
      outline(),
      'brief',
      undefined,
      worldGraph([undatedA, undatedB]),
    );
    expect(undatedResult.nodes).toHaveLength(2);
    expect(
      undatedResult.nodes.find((n) => n.id === 'directed-axis-spine'),
    ).toBeUndefined();

    const dated = eraEntity({
      key_block_id: 'kb-era-dated',
      canonical_name: 'The First Age',
      body: {
        attributes: { era_id: 'era-first', start_hint: '1000-01-01T00:00:00Z' },
      },
    });
    const datedResult = projectWorkTimelineGraph(
      outline(),
      'brief',
      undefined,
      worldGraph([dated, undatedA]),
    );
    expect(
      datedResult.nodes.find((n) => n.id === 'directed-axis-spine'),
    ).toBeDefined();
  });

  it('emits no edges on the Brief layer (minimal density — era sweep only)', () => {
    const era = eraEntity({
      key_block_id: 'kb-era-1',
      canonical_name: 'The First Age',
    });

    const { edges } = projectWorkTimelineGraph(
      outline(),
      'brief',
      undefined,
      worldGraph([era]),
    );

    expect(edges).toEqual([]);
  });
});

describe('WorkTimelineCanvasAdapter — Brief layer factory wiring', () => {
  it("createWorkTimelineCanvasAdapter(ctxRef, 'brief', worldGraph) delegates projectGraph to the Brief layer", () => {
    const era = eraEntity({
      key_block_id: 'kb-era-1',
      canonical_name: 'The First Age',
    });
    const g = outline({ timeline_events: [] });

    const adapter = createWorkTimelineCanvasAdapter(
      { current: makeContext() },
      'brief',
      worldGraph([era]),
    );
    const { nodes } = adapter.projectGraph(g);

    expect(nodes).toHaveLength(2); // era node + directed-axis spine
    expect(nodes[0].type).toBe('timeline-brief-era');
    expect(nodes[0].id).toBe('entity:kb-era-1');
  });

  it('reads the bound World graph from the ctxRef slot when the factory param is absent (fallback)', () => {
    const era = eraEntity({
      key_block_id: 'kb-era-1',
      canonical_name: 'The First Age',
    });

    const adapter = createWorkTimelineCanvasAdapter(
      { current: makeContext({ boundWorldGraph: worldGraph([era]) }) },
      'brief',
    );
    const { nodes } = adapter.projectGraph(outline());

    expect(nodes.some((n) => n.id === 'entity:kb-era-1')).toBe(true);
  });

  it("projectGraphForLayer(graph, 'brief') dispatches the Brief projection", () => {
    const era = eraEntity({
      key_block_id: 'kb-era-1',
      canonical_name: 'The First Age',
    });

    const adapter = createWorkTimelineCanvasAdapter(
      { current: makeContext() },
      'narrative', // active layer is narrative — projectGraphForLayer must still honor 'brief'
      worldGraph([era]),
    );
    const { nodes } = adapter.projectGraphForLayer(outline(), 'brief');

    expect(nodes.some((n) => n.type === 'timeline-brief-era')).toBe(true);
  });

  it("registers the 'timeline-brief-era' node type alongside Narrative/Moment types", () => {
    const adapter = createWorkTimelineCanvasAdapter({ current: makeContext() });

    expect(adapter.nodeTypes['timeline-brief-era']).toBeDefined();
    expect(adapter.nodeTypes['work-timeline-narrative-event']).toBeDefined();
    expect(adapter.nodeTypes['work-timeline-moment-scene']).toBeDefined();
    expect(adapter.nodeTypes['work-timeline-moment-beat']).toBeDefined();
    expect(adapter.nodeTypes['directedAxisSpine']).toBeDefined();
  });

  it("keeps defaultLayer 'narrative' and gives Brief the World-Brief layout feel (LR + wide rankSep)", () => {
    const briefAdapter = createWorkTimelineCanvasAdapter(
      { current: makeContext() },
      'brief',
    );
    const narrativeAdapter = createWorkTimelineCanvasAdapter(
      { current: makeContext() },
      'narrative',
    );

    expect(briefAdapter.defaultLayer).toBe('narrative');
    expect(narrativeAdapter.defaultLayer).toBe('narrative');
    expect(briefAdapter.layoutOptions?.direction).toBe('LR');
    expect(briefAdapter.layoutOptions?.rankSep).toBe(240);
    expect(briefAdapter.layoutOptions?.nodeSep).toBe(40);
    expect(briefAdapter.layoutOptions?.hasSuppliedPositions).toBe(true);
  });
});

describe('WorkTimelineCanvasAdapter.renderBriefTimeBands — V1.160 P2 T1 time-band slot', () => {
  it('exposes the renderBriefTimeBands slot on the adapter (mirror of the World Timeline)', () => {
    const adapter = createWorkTimelineCanvasAdapter(
      { current: makeContext() },
      'brief',
    );

    expect(typeof adapter.renderBriefTimeBands).toBe('function');
  });

  it('renders the BriefTimeBands panel from the bound World graph — NOT from the Work outline graph', () => {
    const era = eraEntity({
      key_block_id: 'kb-era-1',
      canonical_name: 'The First Age',
    });
    const g = outline({
      timeline_events: [{ event_id: 'evt-1', title: 'Inciting Incident' }],
    });

    const adapter = createWorkTimelineCanvasAdapter(
      { current: makeContext() },
      'brief',
      worldGraph([era]),
    );

    const { container } = renderInApp(
      adapter.renderBriefTimeBands!(g) as React.ReactElement,
    );

    const bands = screen.getByTestId('brief-time-bands');
    expect(bands).toBeInTheDocument();
    expect(within(bands).getAllByTestId('brief-time-band')).toHaveLength(1);
    expect(bandOf(container, 'kb-era-1')).toHaveTextContent('The First Age');
  });

  it('falls back to the ctxRef boundWorldGraph slot when the factory param is absent', () => {
    const era = eraEntity({
      key_block_id: 'kb-era-1',
      canonical_name: 'The First Age',
    });

    const adapter = createWorkTimelineCanvasAdapter(
      { current: makeContext({ boundWorldGraph: worldGraph([era]) }) },
      'brief',
    );

    const { container } = renderInApp(
      adapter.renderBriefTimeBands!(outline()) as React.ReactElement,
    );

    const bands = screen.getByTestId('brief-time-bands');
    expect(within(bands).getAllByTestId('brief-time-band')).toHaveLength(1);
    expect(bandOf(container, 'kb-era-1')).toHaveTextContent('The First Age');
  });

  it('renders no band panel when the bound World graph has no era entities', () => {
    const adapter = createWorkTimelineCanvasAdapter(
      { current: makeContext() },
      'brief',
      worldGraph([]),
    );

    const { container } = renderInApp(
      adapter.renderBriefTimeBands!(outline()) as React.ReactElement,
    );

    expect(screen.queryByTestId('brief-time-bands')).toBeNull();
    expect(container).toBeEmptyDOMElement();
  });

  it('renders nested eras as indented bands from parent_era relationships (shared V1.159 buildEraTree)', () => {
    const kingdom = eraEntity({
      key_block_id: 'kb-kingdom',
      canonical_name: 'The Mythic Kingdom',
    });
    const age = eraEntity({
      key_block_id: 'kb-age',
      canonical_name: 'The First Age',
    });

    const adapter = createWorkTimelineCanvasAdapter(
      { current: makeContext() },
      'brief',
      worldGraph([kingdom, age], [parentEraRel('kb-kingdom', 'kb-age', 1)]),
    );

    const { container } = renderInApp(
      adapter.renderBriefTimeBands!(outline()) as React.ReactElement,
    );

    const bands = screen.getByTestId('brief-time-bands');
    expect(within(bands).getAllByTestId('brief-time-band')).toHaveLength(2);
    expect(bandOf(container, 'kb-kingdom').parentElement).toHaveAttribute(
      'data-depth',
      '0',
    );
    expect(bandOf(container, 'kb-age').parentElement).toHaveAttribute(
      'data-depth',
      '1',
    );
  });
});

describe('WorkTimelineCanvasAdapter — Narrative/Moment regression with the extended signature', () => {
  it('Narrative projection is unchanged when a bound World graph is supplied', () => {
    const g = outline({
      timeline_events: [
        { event_id: 'evt-1', title: 'Inciting Incident', realizes_chapter_id: 3 },
        { event_id: 'evt-2', title: 'Midpoint Reversal', realizes_chapter_id: 7 },
      ],
    });
    const era = eraEntity({
      key_block_id: 'kb-era-1',
      canonical_name: 'The First Age',
    });

    const { nodes } = projectWorkTimelineGraph(
      g,
      'narrative',
      undefined,
      worldGraph([era]),
    );

    const entityNodes = nodes.filter((n) => n.type !== 'directedAxisSpine');
    expect(entityNodes).toHaveLength(2);
    expect(entityNodes.every((n) => n.type === 'work-timeline-narrative-event')).toBe(true);
    expect(entityNodes.map((n) => n.id).sort()).toEqual(['wt-event:evt-1', 'wt-event:evt-2']);
  });

  it('Moment projection is unchanged (fixture-driven; brief graph does not leak in)', () => {
    const g = outline({
      timeline_events: [{ event_id: 'evt-1', title: 'Inciting Incident' }],
    });
    const fixture: SceneBeatFixturePayload = {
      scenes: [{ sceneId: 'sc-1', chapterId: 1, title: 'Opening', status: 'drafted' }],
      beats: [{ beatId: 'bt-1', sceneId: 'sc-1', title: 'Beat one', status: 'drafted' }],
    };
    const era = eraEntity({
      key_block_id: 'kb-era-1',
      canonical_name: 'The First Age',
    });

    const { nodes } = projectWorkTimelineGraph(
      g,
      'moment',
      fixture,
      worldGraph([era]),
    );

    expect(nodes.some((n) => n.type === 'work-timeline-moment-scene')).toBe(true);
    expect(nodes.some((n) => n.type === 'work-timeline-moment-beat')).toBe(true);
    expect(nodes.some((n) => n.type === 'timeline-brief-era')).toBe(false);
  });

  it("projectGraph with the default active layer still delegates to 'narrative'", () => {
    const g = outline({
      timeline_events: [{ event_id: 'evt-1', title: 'Inciting Incident' }],
    });

    const adapter = createWorkTimelineCanvasAdapter({
      current: makeContext(),
    });
    const { nodes } = adapter.projectGraph(g);

    expect(nodes.some((n) => n.type === 'work-timeline-narrative-event')).toBe(true);
  });
});

describe('WorkTimelineCanvas — Brief layer time-band rendering (V1.160 P2 T1)', () => {
  it('renders the vertical time-band panel when Brief is active and the bound World has era entities', async () => {
    const era = eraEntity({
      key_block_id: 'kb-era-1',
      canonical_name: 'The First Age',
    });
    const g = outline({
      timeline_events: [{ event_id: 'evt-1', title: 'Inciting Incident' }],
    });
    const client = {
      getWorkOutline: vi.fn().mockResolvedValue(g),
      getWork: vi.fn().mockResolvedValue({
        work_id: 'work-1',
        world_id: 'world-7',
      }),
      getWorldKbGraph: vi.fn().mockResolvedValue(worldGraph([era])),
    } as unknown as NexusClient;

    const { container } = renderInApp(<WorkTimelineCanvas workId="work-1" />, {
      client,
      initialRouterEntries: ['/?layer=brief'],
    });

    await waitFor(() => {
      expect(screen.getByTestId('brief-time-bands')).toBeInTheDocument();
    });

    const bands = screen.getByTestId('brief-time-bands');
    expect(within(bands).getAllByTestId('brief-time-band')).toHaveLength(1);
    expect(bandOf(container, 'kb-era-1')).toHaveTextContent('The First Age');
    expect(screen.queryByTestId('canvas-shell-timeline-badge')).toBeNull();
  });

  it('keeps the spatial canvas when the bound World has no era entities (Brief empty-state path)', async () => {
    const g = outline({
      timeline_events: [{ event_id: 'evt-1', title: 'Inciting Incident' }],
    });
    const client = {
      getWorkOutline: vi.fn().mockResolvedValue(g),
      getWork: vi.fn().mockResolvedValue({
        work_id: 'work-1',
        world_id: 'world-7',
      }),
      getWorldKbGraph: vi.fn().mockResolvedValue(worldGraph([])),
    } as unknown as NexusClient;

    renderInApp(<WorkTimelineCanvas workId="work-1" />, {
      client,
      initialRouterEntries: ['/?layer=brief'],
    });

    await waitFor(() => {
      expect(screen.getByTestId('work-timeline-canvas')).toBeInTheDocument();
    });
    expect(screen.queryByTestId('brief-time-bands')).toBeNull();
  });
});
