/**
 * `canvas-nav` — Canvas nav group data + active-surface resolver (V1.111 P1 T1).
 *
 * Coverage: the pure {@link resolveActiveCanvasSurface} resolver across all
 * three surfaces, the non-canvas / list-route null cases, and partial-match /
 * case-sensitivity edge cases. Also pins the group data shape (id, label,
 * item count, unique destinations, surface-id coverage) so T2/T3 can rely on it.
 */
import { describe, expect, it } from 'vitest';

import {
  CANVAS_ITEMS,
  CANVAS_NAV_GROUP,
  resolveActiveCanvasSurface,
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

describe('CANVAS_NAV_GROUP — group data shape', () => {
  it('exposes the "Canvas" group with a stable id and Title-Case label', () => {
    expect(CANVAS_NAV_GROUP.id).toBe('canvas');
    expect(CANVAS_NAV_GROUP.label).toBe('Canvas');
  });

  it('lists exactly the three canvas surfaces in display order', () => {
    expect(CANVAS_ITEMS.map((item) => item.surfaceId)).toEqual([
      'outline',
      'world-kb',
      'strategy',
    ]);
    expect(CANVAS_NAV_GROUP.items).toHaveLength(3);
  });

  it('gives every item a unique destination (the chrome keys nav items by `to`)', () => {
    // shell-sidebar-chrome.tsx uses `key={item.to}`; duplicates would break rendering.
    const destinations = CANVAS_NAV_GROUP.items.map((item) => item.to);
    expect(new Set(destinations).size).toBe(destinations.length);
  });

  it('defines an icon and non-empty label for every item', () => {
    for (const item of CANVAS_NAV_GROUP.items) {
      expect(typeof item.label).toBe('string');
      expect(item.label.length).toBeGreaterThan(0);
      expect(item.icon).toBeDefined();
    }
  });
});
