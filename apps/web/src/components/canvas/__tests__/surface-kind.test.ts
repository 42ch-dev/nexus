/**
 * CanvasSurfaceKind + canvas-nav — V1.122 P1 T1.
 *
 * Pins the additive `"timeline"` peer surface value at three layers:
 *   1. Type-level: `CanvasSurfaceKind` accepts `'timeline'` (compile-time).
 *   2. Adapter contract: a synthetic adapter declaring `surfaceKind: 'timeline'`
 *      satisfies `CanvasSurfaceAdapter` — this is the "CanvasShell accepts a
 *      Timeline registration" check (CanvasShell + useCanvasSurface route by
 *      adapter, not by an in-shell registry).
 *   3. canvas-nav: the Timeline surface id is recognised by the active-surface
 *      resolver + the click-target resolver, mirroring Outline / World KB.
 *
 * T2 ships `createTimelineCanvasAdapter`; T3 ships the route + sidebar entry.
 */
import { describe, expect, it } from 'vitest';

import type {
  CanvasSurfaceAdapter,
  CanvasSurfaceKind,
} from '../canvas-surface-adapter';
import {
  CANVAS_ITEMS,
  resolveActiveCanvasSurface,
  resolveCanvasNavTarget,
  type CanvasSurfaceId,
} from '@/components/layout/canvas-nav';

describe('CanvasSurfaceKind — V1.122 P1 T1 adds "timeline" peer surface', () => {
  it('accepts "timeline" as a CanvasSurfaceKind value (type + value)', () => {
    const value: CanvasSurfaceKind = 'timeline';
    expect(value).toBe('timeline');
  });

  it('preserves the existing surface kinds (additive only)', () => {
    // The shipped kinds MUST remain so existing adapters stay valid.
    const strategy: CanvasSurfaceKind = 'strategy';
    const outline: CanvasSurfaceKind = 'outline';
    const worldKbEntities: CanvasSurfaceKind = 'world-kb-entities';
    const worldKbRelationships: CanvasSurfaceKind = 'world-kb-relationships';
    expect(
      new Set([strategy, outline, worldKbEntities, worldKbRelationships, 'timeline']).size,
    ).toBe(5);
  });

  it('CanvasSurfaceAdapter accepts a synthetic adapter with surfaceKind: "timeline"', () => {
    // The contract CanvasShell + useCanvasSurface rely on to "register" a
    // surface is the shared `CanvasSurfaceAdapter` interface. A minimal
    // adapter declaring the Timeline surface MUST satisfy it.
    const synthetic: CanvasSurfaceAdapter<
      { entities: never[] },
      { [key: string]: unknown },
      { [key: string]: unknown }
    > = {
      surfaceKind: 'timeline',
      nodeTypes: {},
      projectGraph: () => ({ nodes: [], edges: [] }),
      summarizeGraph: () => 'empty timeline',
    };
    expect(synthetic.surfaceKind).toBe('timeline');
    expect(typeof synthetic.projectGraph).toBe('function');
    expect(typeof synthetic.summarizeGraph).toBe('function');
  });
});

describe('canvas-nav — Timeline surface id + resolver (V1.122 P1 T1)', () => {
  it('CanvasSurfaceId includes "timeline"', () => {
    const id: CanvasSurfaceId = 'timeline';
    expect(id).toBe('timeline');
  });

  it('resolveActiveCanvasSurface recognises /worlds/:worldId/timeline as timeline', () => {
    expect(resolveActiveCanvasSurface('/worlds/world-7/timeline')).toBe<'timeline'>('timeline');
  });

  it('resolveActiveCanvasSurface recognises nested timeline routes', () => {
    expect(resolveActiveCanvasSurface('/worlds/world-7/timeline/chapters')).toBe('timeline');
  });

  it('does not mis-classify sibling World KB routes (shared /worlds/ prefix)', () => {
    // Timeline + World KB share the /worlds/ prefix — the resolver MUST
    // distinguish them by the trailing segment, mirroring the World KB rule.
    expect(resolveActiveCanvasSurface('/worlds/world-7/kb')).toBe('world-kb');
    expect(resolveActiveCanvasSurface('/worlds/world-7')).toBeNull();
    expect(resolveActiveCanvasSurface('/worlds')).toBeNull();
  });

  it('resolveCanvasNavTarget routes timeline to the world-scoped timeline URL when worldId is present', () => {
    expect(resolveCanvasNavTarget('timeline', { worldId: 'world-7' })).toBe(
      '/worlds/world-7/timeline',
    );
  });

  it('encodes the worldId for the timeline target (space-bearing id stays one segment)', () => {
    expect(resolveCanvasNavTarget('timeline', { worldId: 'w 7' })).toBe('/worlds/w%207/timeline');
  });

  it('falls back to the /worlds picker when no worldId is in the URL', () => {
    expect(resolveCanvasNavTarget('timeline', {})).toBe('/worlds');
    expect(resolveCanvasNavTarget('timeline', { worldId: undefined })).toBe('/worlds');
  });

  it('does not use the workId as a fallback for the timeline target', () => {
    // A workId is NOT a worldId; the resolver MUST NOT conflate them.
    expect(resolveCanvasNavTarget('timeline', { workId: 'w-1' })).toBe('/worlds');
  });

  it('does NOT add Timeline to CANVAS_ITEMS (T3 owns the sidebar entry)', () => {
    // T1 is the type-level + resolver-level registration only. The sidebar
    // item + default-World-entry redirect land in T3 alongside the route.
    expect(CANVAS_ITEMS.map((item) => item.surfaceId)).toEqual(['outline', 'world-kb']);
  });
});
