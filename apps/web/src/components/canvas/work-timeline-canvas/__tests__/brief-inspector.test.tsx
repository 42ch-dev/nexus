/**
 * WorkTimelineCanvas — V1.156 P2 T2 (read-only Work-Brief era inspector).
 *
 * Verifies the read-only Brief-era inspector contract locked by
 * `canvas-strategy-surface.md` §3.3.3 V1.156 amendment + plan
 * `2026-08-10-v1.156-p2-work-timeline-brief-layer.md`:
 *
 *   - Selecting a `timeline-brief-era` node (the bound World's Brief
 *     projection — PD-2) dispatches a Work-Brief-era inspector that
 *     surfaces era markers (`eraId`, `startHint`, `endHint`,
 *     `worldSummary`) extracted from `body.attributes` — mirroring the
 *     World Timeline's Brief-era inspector chrome.
 *   - The inspector is strictly READ-ONLY: no title/body editors, no Save,
 *     no `kb.patch_entity` write path (P1 fix-wave lesson W-1 applied
 *     proactively — the Work surface never patches; Brief is World spine),
 *     and no "Edit in Outline" CTA (the era is World-owned, not a Work
 *     manuscript node).
 *   - A "View on World Timeline" CTA navigates to the bound World's Brief
 *     layer (`/worlds/:worldId/timeline?layer=brief`) — source-World
 *     attribution (spec §3.3.3). Hidden when no World is bound.
 *
 * Inspector dispatch tests mirror the V1.123 P1 Brief-era inspector dispatch
 * test pattern (`brief-feel-differentiation.test.tsx` + the Work
 * `moment-feel-differentiation.test.tsx`): they call
 * `adapter.renderInspector(node)` directly and render the returned JSX in
 * isolation. jsdom does not render React Flow node text, so adapter-level
 * dispatch is the cleaner contract surface.
 *
 * `wire_contracts_changed: false` — frontend-only; reuses the V1.73
 * `WorldKbGraphResponse` carrier + the `timeline-brief-era` node type from
 * the World registry (no schema / codegen / daemon / contracts diff).
 */
import { afterEach, describe, expect, it, vi } from 'vitest';

import { renderInApp } from '@/test/test-providers';
import type {
  WorldKbEntityProjection,
  WorldKbGraphResponse,
  WorkOutline,
} from '@42ch/nexus-contracts';

import {
  createWorkTimelineCanvasAdapter,
  type WorkTimelineCanvasAdapterContext,
} from '../work-timeline-canvas-adapter';

// ─── Fixture builders ──────────────────────────────────────────────────────

function outline(overrides: Partial<WorkOutline> = {}): WorkOutline {
  return {
    work_id: 'work-1',
    outline_revision: 1,
    volumes: [],
    timeline_events: [],
    foreshadows: [],
    chapter_titles: {},
    updated_at: '2026-07-18T00:00:00Z',
    ...overrides,
  } as WorkOutline;
}

function worldEntity(
  overrides: Partial<WorldKbEntityProjection> &
    Pick<WorldKbEntityProjection, 'key_block_id' | 'block_type' | 'canonical_name'>,
): WorldKbEntityProjection {
  return {
    world_id: 'world-bi',
    status: 'confirmed',
    version: 3,
    ...overrides,
  } as WorldKbEntityProjection;
}

function eraEntity(
  overrides: Partial<WorldKbEntityProjection> &
    Pick<WorldKbEntityProjection, 'key_block_id' | 'canonical_name'>,
): WorldKbEntityProjection {
  const { key_block_id, canonical_name, body, ...rest } = overrides;
  return worldEntity({
    key_block_id,
    block_type: 'era',
    canonical_name,
    body: body ?? {
      attributes: {
        era_id: 'era-first',
        start_hint: '1000-01-01T00:00:00Z',
        end_hint: '1100-01-01T00:00:00Z',
        world_summary: 'A time of myth and legend.',
      },
    },
    ...rest,
  });
}

function worldGraph(entities: WorldKbEntityProjection[]): WorldKbGraphResponse {
  return { entities, source_anchors: [], relationships: [] };
}

function makeContext(
  overrides: Partial<WorkTimelineCanvasAdapterContext> = {},
): WorkTimelineCanvasAdapterContext {
  return {
    workId: 'work-1',
    ...overrides,
  };
}

// ─── Brief-era inspector dispatch (V1.156 P2 T2) ─────────────────────────

describe('WorkTimelineCanvasAdapter.renderInspector — read-only Brief-era dispatch', () => {
  afterEach(() => {
    vi.clearAllMocks();
  });

  it('routes a timeline-brief-era node to the Work-Brief inspector (distinct from Narrative event inspector)', () => {
    // T2 Step: Brief-era selections must surface a Brief-era-distinct
    // inspector — NOT the generic Work Timeline inspector shell.
    const era = eraEntity({
      key_block_id: 'kb-era-1',
      canonical_name: 'The First Age',
    });
    const ctxRef = { current: makeContext() };
    const adapter = createWorkTimelineCanvasAdapter(
      ctxRef as React.MutableRefObject<WorkTimelineCanvasAdapterContext>,
      'brief',
      worldGraph([era]),
    );
    const { nodes } = adapter.projectGraph(outline());
    const eraNode = nodes.find((n) => n.type === 'timeline-brief-era')!;

    const inspector = adapter.renderInspector?.(eraNode as never);
    expect(inspector).not.toBeNull();

    const { container } = renderInApp(inspector as React.ReactElement);
    expect(
      container.querySelector('[data-testid="work-timeline-brief-era-inspector"]'),
    ).not.toBeNull();
    // NOT the generic shell (dispatch contract distinct).
    expect(
      container.querySelector('[data-testid="work-timeline-inspector"]'),
    ).toBeNull();
  });

  it('surfaces era markers (era id, time span, world summary) + read-only note', () => {
    // Mirrors the World Timeline Brief-era inspector marker chrome: era id
    // pill + time-span label + full world summary.
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
    const ctxRef = { current: makeContext() };
    const adapter = createWorkTimelineCanvasAdapter(
      ctxRef as React.MutableRefObject<WorkTimelineCanvasAdapterContext>,
      'brief',
      worldGraph([era]),
    );
    const { nodes } = adapter.projectGraph(outline());
    const eraNode = nodes.find((n) => n.type === 'timeline-brief-era')!;

    const inspector = adapter.renderInspector?.(eraNode as never);
    const { container } = renderInApp(inspector as React.ReactElement);

    expect(
      container.querySelector('[data-testid="work-timeline-brief-era-inspector-era-id"]')
        ?.textContent,
    ).toContain('era-first');
    expect(
      container.querySelector('[data-testid="work-timeline-brief-era-inspector-span"]')
        ?.textContent,
    ).toContain('1000-01-01T00:00:00Z');
    expect(
      container.querySelector('[data-testid="work-timeline-brief-era-inspector-world-summary"]')
        ?.textContent,
    ).toContain('A time of myth and legend.');
    // Read-only note (PD-2) surfaces.
    expect(container.textContent ?? '').toContain('Read-only');
    // Version badge surfaces (identity chrome).
    expect(container.textContent ?? '').toContain('v3');
  });

  it('renders NO editable fields, NO Save, NO Edit-in-Outline CTA (read-only projection — PD-2)', () => {
    // P1 fix-wave lesson W-1 applied proactively: Brief-era nodes on the
    // Work Timeline must NOT dispatch a write-capable inspector. The World
    // surface owns Brief writes (`kb.patch_entity`); the Work surface is a
    // read-only projection — no text inputs, no submit, no manuscript
    // edit hand-off.
    const era = eraEntity({
      key_block_id: 'kb-era-1',
      canonical_name: 'The First Age',
    });
    const ctxRef = { current: makeContext() };
    const adapter = createWorkTimelineCanvasAdapter(
      ctxRef as React.MutableRefObject<WorkTimelineCanvasAdapterContext>,
      'brief',
      worldGraph([era]),
    );
    const { nodes } = adapter.projectGraph(outline());
    const eraNode = nodes.find((n) => n.type === 'timeline-brief-era')!;

    const inspector = adapter.renderInspector?.(eraNode as never);
    const { container } = renderInApp(inspector as React.ReactElement);

    // No editable controls.
    expect(container.querySelectorAll('input, textarea')).toHaveLength(0);
    // No Save/submit button.
    expect(
      [...container.querySelectorAll('button')].some((b) =>
        /save|submit/i.test(b.textContent ?? ''),
      ),
    ).toBe(false);
    // No "Edit in Outline" hand-off (the era is World-owned).
    expect(
      container.querySelector('[data-testid="work-timeline-inspector-edit-in-outline"]'),
    ).toBeNull();
  });

  it('renders the "View on World Timeline" CTA to the bound World\u2019s Brief layer when a World is bound', () => {
    // Spec §3.3.3: full bound-World Brief with source-World attribution.
    // The CTA targets the bound World's Timeline Brief layer
    // (`?layer=brief`) — the era's home surface.
    const era = eraEntity({
      key_block_id: 'kb-era-1',
      canonical_name: 'The First Age',
    });
    const ctxRef = { current: makeContext({ worldId: 'world-bi' }) };
    const adapter = createWorkTimelineCanvasAdapter(
      ctxRef as React.MutableRefObject<WorkTimelineCanvasAdapterContext>,
      'brief',
      worldGraph([era]),
    );
    const { nodes } = adapter.projectGraph(outline());
    const eraNode = nodes.find((n) => n.type === 'timeline-brief-era')!;

    const inspector = adapter.renderInspector?.(eraNode as never);
    const { container } = renderInApp(inspector as React.ReactElement);

    const cta = container.querySelector(
      '[data-testid="work-timeline-brief-era-view-on-world-timeline"]',
    );
    expect(cta).not.toBeNull();
    expect(cta?.getAttribute('href')).toContain('/worlds/world-bi/timeline');
    expect(cta?.getAttribute('href')).toContain('layer=brief');
  });

  it('hides the cross-surface CTA when no World is bound (honest scope cut)', () => {
    const era = eraEntity({
      key_block_id: 'kb-era-1',
      canonical_name: 'The First Age',
    });
    // No `worldId` in the context → the CTA must not render.
    const ctxRef = { current: makeContext() };
    const adapter = createWorkTimelineCanvasAdapter(
      ctxRef as React.MutableRefObject<WorkTimelineCanvasAdapterContext>,
      'brief',
      worldGraph([era]),
    );
    const { nodes } = adapter.projectGraph(outline());
    const eraNode = nodes.find((n) => n.type === 'timeline-brief-era')!;

    const inspector = adapter.renderInspector?.(eraNode as never);
    const { container } = renderInApp(inspector as React.ReactElement);

    expect(
      container.querySelector(
        '[data-testid="work-timeline-brief-era-view-on-world-timeline"]',
      ),
    ).toBeNull();
  });

  it('still dispatches Narrative event nodes to the event inspector (Brief dispatch does not shadow)', () => {
    // Regression: the Brief-era branch must not intercept event nodes.
    const ctxRef = { current: makeContext() } as React.MutableRefObject<WorkTimelineCanvasAdapterContext>;
    const adapter = createWorkTimelineCanvasAdapter(ctxRef, 'narrative');
    const fakeEventNode = {
      id: 'wt-event:evt-1',
      type: 'work-timeline-narrative-event',
      position: { x: 0, y: 0 },
      data: {
        workId: 'work-1',
        nodeKind: 'event' as const,
        nodeId: 'evt-1',
        eventId: 'evt-1',
        label: 'Inciting Incident',
        realizesChapterId: 1,
        manuscriptAnchor: { chapterId: 1 },
      },
    };

    const inspector = adapter.renderInspector?.(fakeEventNode as never);
    expect(inspector).not.toBeNull();

    const { container } = renderInApp(inspector as React.ReactElement);
    // Generic inspector shell (event branch), NOT the Brief-era inspector.
    expect(
      container.querySelector('[data-testid="work-timeline-inspector"]'),
    ).not.toBeNull();
    expect(
      container.querySelector('[data-testid="work-timeline-brief-era-inspector"]'),
    ).toBeNull();
    expect(container.textContent ?? '').toContain('Inciting Incident');
  });
});
