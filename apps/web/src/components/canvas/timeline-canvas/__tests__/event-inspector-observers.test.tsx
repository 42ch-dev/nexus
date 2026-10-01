/**
 * Timeline Narrative event inspector — modules.observation.observers
 * read-only display (V1.164 P3 Task 4).
 *
 * Locks AC-V1164-13/15 + PD-9/PD-18 for the App event inspector: a
 * Narrative event with a populated `modules.observation.observers` bag
 * renders the "Observers:" metadata line. Observer entry_ids resolve to
 * canonical names ONLY when the name is already in the loaded graph
 * (PD-18 — no new fan-out fetch solely for this panel); otherwise the raw
 * id renders. `observers: []` renders the explicit "No observers" claim
 * (PD-9 — empty = explicitly nobody, distinct from absent). Absent
 * `modules` / `modules.observation` skips the line entirely (PD-9 —
 * absent = unrecorded). Malformed (non-array) observers also skips —
 * lenient like the P2 checker. Copy resolves through the existing web i18n
 * (canvas namespace, `timeline.inspector.observers` / `.noObservers`).
 *
 * V1.165 P2 T3 — AC-V165-8 completion E2E (R-V1164P3QC-001): the
 * graph-derived tests below feed a `WorldKbGraphResponse` shaped exactly
 * like the daemon's post-patch wire (T1's daemon test
 * `patch_entity_observation_on_event_entity_round_trip_on_graph_read`
 * returns `entities[].modules.observation` from a T1 KB patch) through
 * `projectGraph` → `projectNarrativeLayer` (block_type filter) →
 * `entityToTimelineNodeData` (spread) → `TimelineInspector` reads
 * `data.modules.observation.observers` gated `layoutHint === 'event'`.
 */
import { fireEvent, render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import type { Node } from '@xyflow/react';

import { makeQueryClient } from '@/test/test-providers';
import { QueryClientProvider } from '@tanstack/react-query';
import { ClientProvider } from '@/lib/client-context';
import { ToastProvider, Toaster } from '@/lib/use-toast';
import type { NexusClient } from '@/lib/nexus';
import type { WorldKbEntityProjection, WorldKbGraphResponse } from '@42ch/nexus-contracts';

import {
  createTimelineCanvasAdapter,
  type TimelineCanvasAdapterContext,
  type TimelineNodeData,
} from '../timeline-canvas-adapter';
import { TimelineInspector } from '../timeline-inspector';

// ─── Fixture builders (mirror timeline-write-boundary.test.tsx) ─────────────

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

/**
 * Projected graph nodes carrying canonical names (the "already in the
 * loaded graph" PD-18 resolution source — `ctxRef.current.nodes` mirrors
 * what the orchestrator supplies from `surface.nodes`).
 */
function graphNodesWith(names: Record<string, string>): Node<TimelineNodeData>[] {
  return Object.entries(names).map(([id, name]) => ({
    id: `entity:${id}`,
    type: 'timeline-key-block',
    position: { x: 0, y: 0 },
    data: {
      ...entityEvent({ key_block_id: id, block_type: 'character', canonical_name: name }),
      layoutHint: 'context',
    } as TimelineNodeData,
  }));
}

/** Event with recorded observation (mirrors the Task 2 fixture). */
const EVENT_WITH_OBSERVERS: Partial<TimelineNodeData> = {
  modules: {
    observation: {
      observers: ['kb_char_1', 'kb_char_2'],
      access: { line_of_sight: true },
    },
  },
};

/**
 * AC-V165-8 — the daemon wire shape after a T1 KB patch on a `block_type =
 * event` entity: `GET /worlds/:id/kb/graph` returns `entities[].modules`
 * verbatim (mirrors T1's `patch_entity_observation_on_event_entity_
 * round_trip_on_graph_read` assertion shape). The observers are the KB
 * character entities so the PD-18 name resolution has an in-graph source.
 */
function observedEventGraph(): WorldKbGraphResponse {
  return {
    entities: [
      entityEvent({
        key_block_id: 'kb_event',
        canonical_name: 'Hidden Transfer',
        modules: {
          observation: {
            observers: ['kb_char_1', 'kb_char_2'],
            access: { read: ['kb_char_1', 'kb_char_2'] },
          },
        },
      }),
      entityEvent({
        key_block_id: 'kb_char_1',
        block_type: 'character',
        canonical_name: 'Char One',
      }),
      entityEvent({
        key_block_id: 'kb_char_2',
        block_type: 'character',
        canonical_name: 'Char Two',
      }),
    ],
    source_anchors: [],
    relationships: [],
  };
}

function makeClient(): NexusClient {
  return {
    getWorldKbGraph: vi.fn(),
    health: vi.fn().mockResolvedValue({ status: 'ok', version: 'test' }),
  } as unknown as NexusClient;
}

function renderInspector({
  node,
  ctxOverrides = {},
}: {
  node: Node<TimelineNodeData>;
  ctxOverrides?: Partial<TimelineCanvasAdapterContext>;
}) {
  const client = makeClient();
  const ctxRef = { current: { worldId: 'world-7', client, ...ctxOverrides } };
  return render(
    <QueryClientProvider client={makeQueryClient()}>
      <ToastProvider>
        <ClientProvider client={client}>
          <TimelineInspector node={node} ctxRef={ctxRef} />
        </ClientProvider>
        <Toaster />
      </ToastProvider>
    </QueryClientProvider>,
  );
}

describe('TimelineInspector — modules.observation.observers line (V1.164 P3 Task 4)', () => {
  it('renders "Observers: <names>" when modules.observation.observers is populated (AC proof)', () => {
    renderInspector({
      node: eventNode({ ...EVENT_WITH_OBSERVERS }),
      ctxOverrides: {
        nodes: graphNodesWith({ kb_char_1: 'Char One', kb_char_2: 'Char Two' }),
      },
    });

    const line = screen.getByTestId('event-observers-line');
    expect(line).toBeInTheDocument();
    expect(within(line).getByText('Observers:')).toBeInTheDocument();
    // PD-18 — names resolve to "name (id)" when the graph already has them.
    expect(
      within(line).getByText('Char One (kb_char_1), Char Two (kb_char_2)'),
    ).toBeInTheDocument();
  });

  it('falls back to raw ids when observer names are NOT in the loaded graph (PD-18 — no new fetch)', () => {
    // No `nodes` in the ctx (minimal mount) — the line still renders with
    // the raw entry_ids.
    renderInspector({ node: eventNode({ ...EVENT_WITH_OBSERVERS }) });

    const line = screen.getByTestId('event-observers-line');
    expect(line).toBeInTheDocument();
    expect(within(line).getByText('kb_char_1, kb_char_2')).toBeInTheDocument();
  });

  it('omits the observers line when modules is absent (PD-9 — unrecorded)', () => {
    renderInspector({ node: eventNode() });

    expect(screen.queryByTestId('event-observers-line')).not.toBeInTheDocument();
    expect(screen.queryByText('Observers:')).not.toBeInTheDocument();
  });

  it('omits the observers line when modules has no observation (PD-9 — unrecorded)', () => {
    renderInspector({
      node: eventNode({ modules: { mental: { beliefs: { ref: 'kb_b1' } } } }),
    });

    expect(screen.queryByTestId('event-observers-line')).not.toBeInTheDocument();
  });

  it('renders an explicit "No observers" line for observers: [] (PD-9 — empty = explicitly nobody)', () => {
    renderInspector({
      node: eventNode({ modules: { observation: { observers: [] } } }),
    });

    const line = screen.getByTestId('event-observers-line');
    expect(line).toBeInTheDocument();
    expect(within(line).getByText('No observers')).toBeInTheDocument();
  });

  it('omits the observers line when modules is null (defensive null degradation)', () => {
    renderInspector({
      node: eventNode({ modules: null } as unknown as Partial<TimelineNodeData>),
    });

    expect(screen.queryByTestId('event-observers-line')).not.toBeInTheDocument();
  });

  it('omits the observers line when observers is malformed (non-array — lenient skip)', () => {
    renderInspector({
      node: eventNode({
        modules: { observation: { observers: 'kb_char_1' } },
      }),
    });

    expect(screen.queryByTestId('event-observers-line')).not.toBeInTheDocument();
  });

  it('omits the observers line when modules.observation is a non-object (defensive degradation)', () => {
    renderInspector({
      node: eventNode({
        modules: { observation: 'not-an-object' },
      }),
    });

    expect(screen.queryByTestId('event-observers-line')).not.toBeInTheDocument();
    expect(screen.queryByText('Observers:')).not.toBeInTheDocument();
  });

  it('omits the observers line when observers is null (defensive null degradation)', () => {
    renderInspector({
      node: eventNode({
        modules: { observation: { observers: null } },
      }),
    });

    expect(screen.queryByTestId('event-observers-line')).not.toBeInTheDocument();
  });

  it('omits the observers line on context nodes even when modules.observation is populated (S-4 — event-only axis)', () => {
    renderInspector({
      node: eventNode({
        ...EVENT_WITH_OBSERVERS,
        layoutHint: 'context',
      }),
    });

    expect(screen.queryByTestId('event-observers-line')).not.toBeInTheDocument();
    expect(screen.queryByText('Observers:')).not.toBeInTheDocument();
  });
});

// ─── AC-V165-8 E2E — observation→canvas via the KB patch carrier (T3) ───────

describe('TimelineInspector — AC-V165-8: observation→canvas via the KB patch wire (T3)', () => {
  function project(graph: WorldKbGraphResponse) {
    const adapter = createTimelineCanvasAdapter({
      current: { worldId: 'world-7', client: makeClient() },
    });
    return adapter.projectGraph(graph);
  }

  it('renders the observers line from the post-patch graph wire (graph → projection → spread → inspector)', () => {
    const { nodes } = project(observedEventGraph());

    // Chain link 1+2 — projectNarrativeLayer keeps block_type=event and
    // entityToTimelineNodeData spreads `...entity`, so `modules` rides the
    // node data verbatim (the AR-7 referent: WorldKbEntityProjection.modules).
    const eventNode = nodes.find((n) => n.data.key_block_id === 'kb_event');
    expect(eventNode).toBeDefined();
    expect(eventNode!.data.layoutHint).toBe('event');
    expect(eventNode!.data.modules).toEqual({
      observation: {
        observers: ['kb_char_1', 'kb_char_2'],
        access: { read: ['kb_char_1', 'kb_char_2'] },
      },
    });

    // Chain link 3 — the inspector reads data.modules.observation.observers
    // (PD-18: ctx.nodes mirrors surface.nodes — the full projected graph).
    renderInspector({ node: eventNode!, ctxOverrides: { nodes } });

    const line = screen.getByTestId('event-observers-line');
    expect(line).toBeInTheDocument();
    expect(within(line).getByText('Observers:')).toBeInTheDocument();
    expect(
      within(line).getByText('Char One (kb_char_1), Char Two (kb_char_2)'),
    ).toBeInTheDocument();
  });

  it('omits the line for non-event entities carrying the same modules (block_type filter → context gate)', () => {
    const graph: WorldKbGraphResponse = {
      entities: [
        entityEvent({
          key_block_id: 'kb_char_1',
          block_type: 'character',
          canonical_name: 'Char One',
          modules: { observation: { observers: ['kb_char_2'] } },
        }),
        entityEvent({
          key_block_id: 'kb_char_2',
          block_type: 'character',
          canonical_name: 'Char Two',
        }),
      ],
      source_anchors: [],
      relationships: [],
    };
    const { nodes } = project(graph);

    // The projection spreads modules onto every entity; only the event gate
    // (layoutHint === 'event') surfaces the Observers line.
    const charNode = nodes.find((n) => n.data.key_block_id === 'kb_char_1');
    expect(charNode).toBeDefined();
    expect(charNode!.data.layoutHint).toBe('context');
    expect(charNode!.data.modules).toEqual({
      observation: { observers: ['kb_char_2'] },
    });

    renderInspector({ node: charNode!, ctxOverrides: { nodes } });
    expect(screen.queryByTestId('event-observers-line')).not.toBeInTheDocument();
    expect(screen.queryByText('Observers:')).not.toBeInTheDocument();
  });
});

// ─── v1.203 P2 T3 — O3: observation editing on the event inspector ──────────

describe('TimelineInspector — O3 observation editing (v1.203 P2 T3)', () => {
  const CTX_NODES = graphNodesWith({ kb_char_1: 'Char One', kb_char_2: 'Char Two' });

  it('sets observers + access on an event without a stored observation module → save upserts the whole modules.observation value', async () => {
    const user = userEvent.setup();
    const onPatchEntity = vi.fn().mockResolvedValue(undefined);
    renderInspector({
      node: eventNode(),
      ctxOverrides: { nodes: CTX_NODES, onPatchEntity },
    });

    // The edit affordance renders on event nodes even when nothing is stored.
    expect(screen.getByTestId('observation-section')).toBeInTheDocument();
    // Save starts disabled — absent stays absent until the author edits.
    expect(screen.getByTestId('timeline-inspector-save')).toBeDisabled();

    await user.click(screen.getByTestId('observation-observer-kb_char_1'));
    fireEvent.change(screen.getByTestId('observation-access-input'), {
      target: { value: '{"read":true}' },
    });

    expect(screen.getByTestId('timeline-inspector-save')).toBeEnabled();
    await user.click(screen.getByTestId('timeline-inspector-save'));

    expect(onPatchEntity).toHaveBeenCalledTimes(1);
    const [nodeArg, patchArg, dirtyArg] = onPatchEntity.mock.calls[0];
    expect(nodeArg.data.key_block_id).toBe('kb-event-1');
    expect(patchArg).toEqual({
      modules: { observation: { observers: ['kb_char_1'], access: { read: true } } },
    });
    expect(dirtyArg).toContain('modules');
    // The editing status line resolves the new `modules` field label.
    expect(screen.getByText(/Modules/)).toBeInTheDocument();
  });

  it('modifies observers (uncheck one) while access round-trips verbatim', async () => {
    const user = userEvent.setup();
    const onPatchEntity = vi.fn().mockResolvedValue(undefined);
    renderInspector({
      node: eventNode({ ...EVENT_WITH_OBSERVERS }),
      ctxOverrides: { nodes: CTX_NODES, onPatchEntity },
    });

    await user.click(screen.getByTestId('observation-observer-kb_char_1'));
    await user.click(screen.getByTestId('timeline-inspector-save'));

    expect(onPatchEntity).toHaveBeenCalledTimes(1);
    const [, patchArg] = onPatchEntity.mock.calls[0];
    // Whole-first-level-value upsert: the untouched access object is carried
    // along with the edited observers list.
    expect(patchArg).toEqual({
      modules: {
        observation: { observers: ['kb_char_2'], access: { line_of_sight: true } },
      },
    });
  });

  it('clearing every observer writes the explicit empty list (PD-9 claim preserved)', async () => {
    const user = userEvent.setup();
    const onPatchEntity = vi.fn().mockResolvedValue(undefined);
    renderInspector({
      node: eventNode({ ...EVENT_WITH_OBSERVERS }),
      ctxOverrides: { nodes: CTX_NODES, onPatchEntity },
    });

    await user.click(screen.getByTestId('observation-clear'));
    await user.click(screen.getByTestId('timeline-inspector-save'));

    expect(onPatchEntity).toHaveBeenCalledTimes(1);
    const [, patchArg] = onPatchEntity.mock.calls[0];
    // `observers: []` is the meaningful "no observers" claim — NOT an omitted
    // key (absent would read back as unrecorded).
    expect(patchArg.modules.observation.observers).toEqual([]);
    expect(patchArg.modules.observation.access).toEqual({ line_of_sight: true });
  });

  it('clearing access (blank text) on a stored observation writes the empty object', async () => {
    const user = userEvent.setup();
    const onPatchEntity = vi.fn().mockResolvedValue(undefined);
    renderInspector({
      node: eventNode({ ...EVENT_WITH_OBSERVERS }),
      ctxOverrides: { nodes: CTX_NODES, onPatchEntity },
    });

    await user.clear(screen.getByTestId('observation-access-input'));
    await user.click(screen.getByTestId('timeline-inspector-save'));

    const [, patchArg] = onPatchEntity.mock.calls[0];
    expect(patchArg.modules.observation).toEqual({
      observers: ['kb_char_1', 'kb_char_2'],
      access: {},
    });
  });

  it('blocks the write and surfaces a field error when access is not a JSON object', async () => {
    const user = userEvent.setup();
    const onPatchEntity = vi.fn().mockResolvedValue(undefined);
    renderInspector({
      node: eventNode(),
      ctxOverrides: { nodes: CTX_NODES, onPatchEntity },
    });

    fireEvent.change(screen.getByTestId('observation-access-input'), {
      target: { value: 'not-json' },
    });
    await user.click(screen.getByTestId('timeline-inspector-save'));

    expect(onPatchEntity).not.toHaveBeenCalled();
    expect(screen.getByTestId('observation-section')).toHaveTextContent(
      'access must be a valid JSON object.',
    );
  });

  it('blocks the write when access parses to a non-object JSON value', async () => {
    const user = userEvent.setup();
    const onPatchEntity = vi.fn().mockResolvedValue(undefined);
    renderInspector({
      node: eventNode(),
      ctxOverrides: { nodes: CTX_NODES, onPatchEntity },
    });

    fireEvent.change(screen.getByTestId('observation-access-input'), {
      target: { value: '[1,2]' },
    });
    await user.click(screen.getByTestId('timeline-inspector-save'));

    expect(onPatchEntity).not.toHaveBeenCalled();
    expect(screen.getByTestId('observation-section')).toHaveTextContent(
      'access must be a valid JSON object.',
    );
  });

  it('adds an observer by raw entry id (PD-18 — no fan-out fetch; raw id round-trips)', async () => {
    const user = userEvent.setup();
    const onPatchEntity = vi.fn().mockResolvedValue(undefined);
    renderInspector({
      node: eventNode(),
      ctxOverrides: { nodes: CTX_NODES, onPatchEntity },
    });

    await user.type(screen.getByTestId('observation-raw-input'), 'kb_char_9');
    await user.click(screen.getByTestId('observation-raw-add'));

    // The raw id renders as an extra checked row it can later be removed from.
    expect(screen.getByTestId('observation-observer-kb_char_9')).toBeChecked();

    await user.click(screen.getByTestId('timeline-inspector-save'));

    const [, patchArg] = onPatchEntity.mock.calls[0];
    expect(patchArg.modules.observation.observers).toEqual(['kb_char_9']);
  });

  it('maps daemon 422 observation prefixes onto the offending form fields (frozen grammar)', async () => {
    const user = userEvent.setup();
    const onPatchEntity = vi.fn().mockRejectedValue({
      status: 422,
      code: 'world_kb_validation_failed',
      details: {
        validation_summary: {
          errors: [
            'modules.observation.observers: must be an array of non-empty entry ids',
            'modules.mental.identity: must be an object',
          ],
        },
      },
    });
    renderInspector({
      node: eventNode({ ...EVENT_WITH_OBSERVERS }),
      ctxOverrides: { nodes: CTX_NODES, onPatchEntity },
    });

    await user.click(screen.getByTestId('observation-observer-kb_char_1'));
    await user.click(screen.getByTestId('timeline-inspector-save'));

    expect(onPatchEntity).toHaveBeenCalledTimes(1);
    // Known observation prefix → field-level message under the section.
    expect(screen.getByTestId('observation-section')).toHaveTextContent(
      'must be an array of non-empty entry ids',
    );
    // Unrelated dialect prefix stays section-level, verbatim.
    expect(screen.getByTestId('timeline-inspector-validation-errors')).toHaveTextContent(
      'modules.mental.identity: must be an object',
    );
  });

  it('keeps the edit affordance hidden on non-event nodes (S-4 event gate)', () => {
    renderInspector({
      node: eventNode({ ...EVENT_WITH_OBSERVERS, layoutHint: 'context' }),
      ctxOverrides: { nodes: CTX_NODES },
    });

    expect(screen.queryByTestId('observation-section')).not.toBeInTheDocument();
  });

  it('preserves unknown stored observation inner keys verbatim on whole-value write (PD-13)', async () => {
    const user = userEvent.setup();
    const onPatchEntity = vi.fn().mockResolvedValue(undefined);
    renderInspector({
      node: eventNode({
        modules: {
          observation: {
            observers: ['kb_char_1'],
            access: { read: true },
            future_dialect_key: { nested: ['kept'] },
          },
        },
      }),
      ctxOverrides: { nodes: CTX_NODES, onPatchEntity },
    });

    await user.click(screen.getByTestId('observation-observer-kb_char_2'));
    await user.click(screen.getByTestId('timeline-inspector-save'));

    const [, patchArg] = onPatchEntity.mock.calls[0];
    expect(patchArg.modules.observation).toEqual({
      observers: ['kb_char_1', 'kb_char_2'],
      access: { read: true },
      future_dialect_key: { nested: ['kept'] },
    });
  });

  it('an untouched seeded observation is not dirty — a title-only save never rewrites modules', async () => {
    const user = userEvent.setup();
    const onPatchEntity = vi.fn().mockResolvedValue(undefined);
    renderInspector({
      node: eventNode({ ...EVENT_WITH_OBSERVERS }),
      ctxOverrides: { nodes: CTX_NODES, onPatchEntity },
    });

    await user.type(screen.getByLabelText('Title'), '!');
    await user.click(screen.getByTestId('timeline-inspector-save'));

    expect(onPatchEntity).toHaveBeenCalledTimes(1);
    const [, patchArg, dirtyArg] = onPatchEntity.mock.calls[0];
    expect(patchArg).toEqual({ title: 'Coronation!' });
    expect(dirtyArg).not.toContain('modules');
  });

  // ── Fix Round 1 (L2 review C1/C2/C3) ──────────────────────────────────────

  it('C1 — a whitespace-containing stored observer id seeds verbatim: untouched seed + title-only save sends only title', async () => {
    const user = userEvent.setup();
    const onPatchEntity = vi.fn().mockResolvedValue(undefined);
    renderInspector({
      node: eventNode({
        modules: { observation: { observers: ['  kb_char_1  '] } },
      }),
      ctxOverrides: { nodes: CTX_NODES, onPatchEntity },
    });

    // The graph checkbox for the trimmed id is NOT selected — the stored
    // whitespace-containing id seeded verbatim, not coerced.
    expect(screen.getByTestId('observation-observer-kb_char_1')).not.toBeChecked();
    // Seed normalization must NOT mark modules dirty.
    expect(screen.getByTestId('timeline-inspector-save')).toBeDisabled();

    await user.type(screen.getByLabelText('Title'), '!');
    await user.click(screen.getByTestId('timeline-inspector-save'));

    expect(onPatchEntity).toHaveBeenCalledTimes(1);
    const [, patchArg, dirtyArg] = onPatchEntity.mock.calls[0];
    expect(patchArg).toEqual({ title: 'Coronation!' });
    expect(dirtyArg).not.toContain('modules');
  });

  it('C1 — a non-array stored observers member falls back to raw JSON: untouched seed + title-only save preserves the stored module', async () => {
    const user = userEvent.setup();
    const onPatchEntity = vi.fn().mockResolvedValue(undefined);
    const storedModules = { observation: { observers: 'kb_char_1' } };
    renderInspector({
      node: eventNode({ modules: storedModules }),
      ctxOverrides: { nodes: CTX_NODES, onPatchEntity },
    });

    // Raw fallback replaces the structured controls (C1 — no coercion).
    const raw = screen.getByTestId('observation-raw-json');
    expect(raw).toBeInTheDocument();
    expect((raw as HTMLTextAreaElement).value).toBe(
      JSON.stringify(storedModules.observation, null, 2),
    );
    expect(screen.queryByTestId('observation-observer-kb_char_1')).not.toBeInTheDocument();
    expect(screen.getByTestId('timeline-inspector-save')).toBeDisabled();

    await user.type(screen.getByLabelText('Title'), '!');
    await user.click(screen.getByTestId('timeline-inspector-save'));

    expect(onPatchEntity).toHaveBeenCalledTimes(1);
    const [, patchArg] = onPatchEntity.mock.calls[0];
    expect(patchArg).toEqual({ title: 'Coronation!' });
  });

  it('C1 — raw fallback editing preserves the original complete value until deliberately changed, then writes it verbatim', async () => {
    const user = userEvent.setup();
    const onPatchEntity = vi.fn().mockResolvedValue(undefined);
    const stored = { observers: [42, 'kb_char_1'], access: null, future_key: 'kept' };
    renderInspector({
      node: eventNode({ modules: { observation: stored } }),
      ctxOverrides: { nodes: CTX_NODES, onPatchEntity },
    });

    const raw = screen.getByTestId('observation-raw-json') as HTMLTextAreaElement;
    // Editing the JSON (adding a member) writes the complete edited value —
    // unknown inner keys (PD-13) and the previously nonrepresentable
    // members round-trip through the author's text.
    fireEvent.change(raw, {
      target: {
        value: JSON.stringify({ ...stored, note: 'edited' }, null, 2),
      },
    });
    await user.click(screen.getByTestId('timeline-inspector-save'));

    expect(onPatchEntity).toHaveBeenCalledTimes(1);
    const [, patchArg] = onPatchEntity.mock.calls[0];
    expect(patchArg).toEqual({
      modules: { observation: { ...stored, note: 'edited' } },
    });
  });

  it('C1 — invalid raw JSON blocks the write and surfaces a field error', async () => {
    const user = userEvent.setup();
    const onPatchEntity = vi.fn().mockResolvedValue(undefined);
    renderInspector({
      node: eventNode({ modules: { observation: { access: null } } }),
      ctxOverrides: { nodes: CTX_NODES, onPatchEntity },
    });

    fireEvent.change(screen.getByTestId('observation-raw-json'), {
      target: { value: 'not-json' },
    });
    await user.click(screen.getByTestId('timeline-inspector-save'));

    expect(onPatchEntity).not.toHaveBeenCalled();
    expect(screen.getByTestId('observation-section')).toHaveTextContent(
      'observation must be a valid JSON object.',
    );
  });

  it('C2 — a context node with an unusual stored observation shape never emits modules; a title-only edit sends only title', async () => {
    const user = userEvent.setup();
    const onPatchEntity = vi.fn().mockResolvedValue(undefined);
    const storedModules = { observation: { observers: 'kb_char_1' } };
    renderInspector({
      node: eventNode({ modules: storedModules, layoutHint: 'context' }),
      ctxOverrides: { nodes: CTX_NODES, onPatchEntity },
    });

    // The observation editor is hidden (existing JSX gate)…
    expect(screen.queryByTestId('observation-section')).not.toBeInTheDocument();
    expect(screen.queryByTestId('observation-raw-json')).not.toBeInTheDocument();
    // …and the stored module does not make the form dirty (C2 dirty gate).
    expect(screen.getByTestId('timeline-inspector-save')).toBeDisabled();

    await user.type(screen.getByLabelText('Title'), '!');
    await user.click(screen.getByTestId('timeline-inspector-save'));

    expect(onPatchEntity).toHaveBeenCalledTimes(1);
    const [, patchArg, dirtyArg] = onPatchEntity.mock.calls[0];
    expect(patchArg).toEqual({ title: 'Coronation!' });
    expect(dirtyArg).not.toContain('modules');
  });

  it('C3 — an explicit "no observers" claim from an unrecorded event submits observers: [] on the first save and reads back explicitly', async () => {
    const user = userEvent.setup();
    const onPatchEntity = vi.fn().mockResolvedValue(undefined);
    // No observers member at all — previously unrecorded.
    renderInspector({
      node: eventNode(),
      ctxOverrides: { nodes: CTX_NODES, onPatchEntity },
    });

    // Untouched unrecorded seed keeps omission (Save disabled, no modules).
    expect(screen.getByTestId('timeline-inspector-save')).toBeDisabled();

    // The explicit claim checkbox is available even with an empty selection.
    await user.click(screen.getByTestId('observation-claim-none'));
    expect(screen.getByTestId('timeline-inspector-save')).toBeEnabled();
    await user.click(screen.getByTestId('timeline-inspector-save'));

    expect(onPatchEntity).toHaveBeenCalledTimes(1);
    const [, patchArg] = onPatchEntity.mock.calls[0];
    expect(patchArg).toEqual({ modules: { observation: { observers: [] } } });

    // Read-back (L2 cannot-verify item): the post-save projection with
    // `observers: []` renders the explicit "No observers" claim line.
    const updated = eventNode({
      modules: { observation: { observers: [] } },
      version: 4,
    });
    const { unmount } = renderInspector({
      node: updated,
      ctxOverrides: { nodes: CTX_NODES, onPatchEntity },
    });
    const line = screen.getByTestId('event-observers-line');
    expect(within(line).getByText('No observers')).toBeInTheDocument();
    unmount();
  });

  it('C3 — Clear on an unrecorded event records the explicit empty claim (select-then-clear no longer collapses to omission)', async () => {
    const user = userEvent.setup();
    const onPatchEntity = vi.fn().mockResolvedValue(undefined);
    renderInspector({
      node: eventNode(),
      ctxOverrides: { nodes: CTX_NODES, onPatchEntity },
    });

    await user.click(screen.getByTestId('observation-observer-kb_char_1'));
    await user.click(screen.getByTestId('observation-clear'));
    await user.click(screen.getByTestId('timeline-inspector-save'));

    expect(onPatchEntity).toHaveBeenCalledTimes(1);
    const [, patchArg] = onPatchEntity.mock.calls[0];
    // The deliberate clear is the PD-9 "no observers" claim — NOT omission.
    expect(patchArg).toEqual({ modules: { observation: { observers: [] } } });
  });
});
