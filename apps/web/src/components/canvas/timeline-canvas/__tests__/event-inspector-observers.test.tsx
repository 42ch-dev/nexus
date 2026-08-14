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
 */
import { render, screen, within } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import type { Node } from '@xyflow/react';

import { makeQueryClient } from '@/test/test-providers';
import { QueryClientProvider } from '@tanstack/react-query';
import { ClientProvider } from '@/lib/client-context';
import { ToastProvider, Toaster } from '@/lib/use-toast';
import type { NexusClient } from '@/lib/nexus';
import type { WorldKbEntityProjection } from '@42ch/nexus-contracts';

import {
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
