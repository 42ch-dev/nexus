/**
 * `canvas-nav` — Canvas nav items + active-surface resolver.
 *
 * Coverage: the pure {@link resolveActiveCanvasSurface} resolver across all
 * three surfaces, the non-canvas / list-route null cases, and partial-match /
 * case-sensitivity edge cases. Also pins the item data shape (V1.117 regroup:
 * Outline + World KB only — Strategy moved to Orchestration) so the sidebar can
 * rely on it. The resolvers still cover `strategy` (a canvas surface for
 * route-pattern matching) even though it is no longer a sidebar canvas item.
 */
import { describe, expect, it } from 'vitest';

import {
  CANVAS_ITEMS,
  resolveActiveCanvasSurface,
  resolveCanvasNavTarget,
  type CanvasSurfaceId,
} from './canvas-nav';

describe('resolveActiveCanvasSurface — canvas surface patterns', () => {
  it('resolves the Outline surface from a work-scoped outline route', () => {
    expect(resolveActiveCanvasSurface('/works/work-42/outline')).toBe<CanvasSurfaceId>('outline');
  });

  it('resolves the World KB surface from a world-scoped kb route', () => {
    expect(resolveActiveCanvasSurface('/worlds/world-7/kb')).toBe<CanvasSurfaceId>('world-kb');
  });

  it('resolves the Strategy surface from a preset-scoped strategy route', () => {
    expect(resolveActiveCanvasSurface('/strategies/preset-1')).toBe<CanvasSurfaceId>('strategy');
  });
});

describe('resolveActiveCanvasSurface — non-canvas and list routes return null', () => {
  it.each([
    ['works list', '/works'],
    ['work detail (no outline segment)', '/works/work-42'],
    ['work chapters', '/works/work-42/chapters'],
    ['worlds list (no kb segment)', '/worlds/world-7'],
    ['strategies list', '/strategies'],
    ['settings', '/settings'],
    ['memory', '/memory'],
    ['sessions', '/sessions'],
    ['root index', '/'],
    ['empty string', ''],
  ])('returns null for %s', (_label, pathname) => {
    expect(resolveActiveCanvasSurface(pathname)).toBeNull();
  });
});

describe('resolveActiveCanvasSurface — edge cases', () => {
  it('still recognises outline sub-routes as the outline surface', () => {
    // A nested path under outline remains the outline canvas surface.
    expect(resolveActiveCanvasSurface('/works/work-42/outline/chapters')).toBe('outline');
  });

  it('still recognises strategy sub-routes as the strategy surface', () => {
    expect(resolveActiveCanvasSurface('/strategies/preset-1/edit')).toBe('strategy');
  });

  it('matches World KB only when the /kb segment is present', () => {
    expect(resolveActiveCanvasSurface('/worlds/world-7')).toBeNull();
    expect(resolveActiveCanvasSurface('/worlds/world-7/kb')).toBe('world-kb');
    expect(resolveActiveCanvasSurface('/worlds/world-7/kb/nodes')).toBe('world-kb');
  });

  it('is case-sensitive (uppercase prefixes do not match)', () => {
    // Route paths are case-sensitive; the resolver mirrors that.
    expect(resolveActiveCanvasSurface('/WORKS/work-42/outline')).toBeNull();
    expect(resolveActiveCanvasSurface('/Strategies/preset-1')).toBeNull();
  });

  it('does not falsely match outline when /outline appears outside a work route', () => {
    // `/outline` without the `/works/` prefix is not the outline surface.
    expect(resolveActiveCanvasSurface('/something/outline')).toBeNull();
  });
});

describe('CANVAS_ITEMS — canvas surface definitions (V1.118 P1)', () => {
  it('lists exactly Outline + World KB in display order (Strategy moved to Orchestration)', () => {
    expect(CANVAS_ITEMS.map((item) => item.surfaceId)).toEqual(['outline', 'world-kb']);
  });

  it('gives every item a unique destination (the chrome keys nav items by `to`)', () => {
    // shell-sidebar-chrome.tsx uses `key={item.to}` within a group; duplicates
    // would break rendering. Outline (/works) and World KB (/worlds) differ.
    const destinations = CANVAS_ITEMS.map((item) => item.to);
    expect(new Set(destinations).size).toBe(destinations.length);
  });

  it('defines an icon and non-empty label for every item', () => {
    for (const item of CANVAS_ITEMS) {
      expect(typeof item.label).toBe('string');
      expect(item.label.length).toBeGreaterThan(0);
      expect(item.icon).toBeDefined();
    }
  });
});

describe('resolveCanvasNavTarget — Outline (workId-aware)', () => {
  it('routes to the work-scoped outline surface when a workId is present', () => {
    expect(resolveCanvasNavTarget('outline', { workId: 'w-42' })).toBe('/works/w-42/outline');
  });

  it('encodes the workId so a space-bearing id stays one path segment', () => {
    expect(resolveCanvasNavTarget('outline', { workId: 'w 4' })).toBe('/works/w%204/outline');
  });

  it('falls back to the /works picker when no workId is in the URL', () => {
    expect(resolveCanvasNavTarget('outline', {})).toBe('/works');
  });

  it('falls back to the /works picker when workId is undefined', () => {
    expect(resolveCanvasNavTarget('outline', { workId: undefined })).toBe('/works');
  });
});

describe('resolveCanvasNavTarget — World KB (worldId-aware, /worlds picker fallback)', () => {
  it('routes to the world-scoped kb surface when a worldId is present', () => {
    expect(resolveCanvasNavTarget('world-kb', { worldId: 'world-7' })).toBe('/worlds/world-7/kb');
  });

  it('encodes the worldId so a space-bearing id stays one path segment', () => {
    expect(resolveCanvasNavTarget('world-kb', { worldId: 'w 7' })).toBe('/worlds/w%207/kb');
  });

  it('falls back to the /worlds picker when no worldId is in the URL', () => {
    expect(resolveCanvasNavTarget('world-kb', {})).toBe('/worlds');
  });

  it('falls back to the /worlds picker when worldId is undefined', () => {
    expect(resolveCanvasNavTarget('world-kb', { worldId: undefined })).toBe('/worlds');
  });

  it('does not use the workId as a fallback for the world target', () => {
    // A workId is NOT a worldId; the resolver must not conflate them. Without a
    // worldId it falls back to the picker, not to the work-scoped route.
    expect(resolveCanvasNavTarget('world-kb', { workId: 'w-1' })).toBe('/worlds');
  });
});

describe('resolveCanvasNavTarget — Strategy (always the list)', () => {
  it('routes to /strategies regardless of context', () => {
    expect(resolveCanvasNavTarget('strategy', {})).toBe('/strategies');
    expect(resolveCanvasNavTarget('strategy', { workId: 'w-1', worldId: 'w-7' })).toBe(
      '/strategies',
    );
  });
});

describe('resolveCanvasNavTarget — context is not mutated', () => {
  it('treats workId/worldId independently (outline ignores worldId, world-kb ignores workId)', () => {
    // Outline target depends only on workId; a present worldId does not change it.
    expect(resolveCanvasNavTarget('outline', { workId: 'w-1', worldId: 'world-7' })).toBe(
      '/works/w-1/outline',
    );
    // World KB target depends only on worldId; a present workId does not
    // ungate it — it still falls back to the /worlds picker.
    expect(resolveCanvasNavTarget('world-kb', { workId: 'w-1' })).toBe('/worlds');
  });
});
