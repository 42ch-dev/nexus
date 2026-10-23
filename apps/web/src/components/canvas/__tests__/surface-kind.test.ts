/**
 * CanvasSurfaceKind — V1.122 P1 T1.
 *
 * Pins the additive `"timeline"` peer surface value at two layers:
 *   1. Type-level: `CanvasSurfaceKind` accepts `'timeline'` (compile-time).
 *   2. Adapter contract: a synthetic adapter declaring `surfaceKind: 'timeline'`
 *      satisfies `CanvasSurfaceAdapter` — this is the "CanvasShell accepts a
 *      Timeline registration" check (CanvasShell + useCanvasSurface route by
 *      adapter, not by an in-shell registry).
 */
import { describe, expect, it } from 'vitest';

import type {
  CanvasSurfaceAdapter,
  CanvasSurfaceKind,
} from '../canvas-surface-adapter';

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
