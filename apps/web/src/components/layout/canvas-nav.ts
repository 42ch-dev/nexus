import { Globe, ListTree, Workflow } from 'lucide-react';

import type { ShellNavGroup, ShellNavItem } from './presentational/shell-sidebar-chrome';

/**
 * Canvas nav group — V1.111 P1 sidebar IA restructure.
 *
 * The three canvas surfaces (Outline / World KB / Strategy) are nested under a
 * single "Canvas" {@link ShellNavGroup} disclosure. Because the three surfaces
 * use **different route params** (`workId` / `worldId` / `presetId`), they
 * cannot all nest under one "active Work" id — see
 * `plans/2026-07-12-v1.111-sidebar-canvas-ia.md` § Architecture locks #1.
 *
 * Active-surface highlight derives from **route-pattern matching** via
 * {@link resolveActiveCanvasSurface}, NOT from a single Work param and NOT from
 * the chrome's built-in `item.to` prefix match (which is too broad — it would
 * light up "Outline" on plain `/works/:workId` work-detail).
 *
 * Consumed by T2 (`sidebar.tsx` restructure) and T3 (navigation wiring).
 */

/** The three canvas surfaces exposed under the "Canvas" nav group. */
export type CanvasSurfaceId = 'outline' | 'world-kb' | 'strategy';

/**
 * A {@link ShellNavItem} that also knows which canvas surface it represents.
 *
 * The extra `surfaceId` is invisible to the chrome (which reads only `to` /
 * `label` / `icon`) but lets T2 bridge the route-pattern resolver to per-item
 * highlight without a parallel `to → surfaceId` map that could drift.
 */
export interface CanvasNavItem extends ShellNavItem {
  surfaceId: CanvasSurfaceId;
}

/**
 * The three canvas nav items, in display order.
 *
 * Each `to` is the **entity-list entry point** for its surface — the place a
 * user lands to pick the context (Work / World / Preset) the surface renders:
 * - Outline   → `/works`      (pick a Work → `/works/:workId/outline`)
 * - World KB  → `/worlds`     (pick a World → `/worlds/:worldId/kb`)
 * - Strategy  → `/strategies` (pick a Preset → `/strategies/:presetId`)
 *
 * This mirrors P0's command palette: `go.strategy` navigates to `/strategies`,
 * while `go.outline` / `go.world-kb` navigate to the context-scoped surface
 * route when a `workId` / `worldId` is present in the current route params
 * (`components/canvas/canvas-nav-commands.tsx`).
 *
 * NOTE (T3 / roadmap handoff): the `/worlds` list route does not exist yet
 * (only `/worlds/:worldId/kb` is registered in `App.tsx`). Until a World picker
 * lands, the World KB item's click destination is resolved by T3 navigation
 * wiring (e.g. via the current work's world). This does not affect the
 * active-surface resolver, which keys off the `/worlds/.../kb` pattern.
 */
export const CANVAS_ITEMS: CanvasNavItem[] = [
  { to: '/works', label: 'Outline', icon: ListTree, surfaceId: 'outline' },
  { to: '/worlds', label: 'World KB', icon: Globe, surfaceId: 'world-kb' },
  { to: '/strategies', label: 'Strategy', icon: Workflow, surfaceId: 'strategy' },
];

/**
 * The "Canvas" nav group, ready to drop into a tab's group list.
 *
 * Typed as the plain {@link ShellNavGroup} the chrome expects; the richer
 * {@link CanvasNavItem} metadata is still reachable via {@link CANVAS_ITEMS}.
 * `defaultOpen` is intentionally unset to match the existing group convention
 * (the chrome defaults to open — `shell-sidebar-chrome.tsx:206`).
 */
export const CANVAS_NAV_GROUP: ShellNavGroup = {
  id: 'canvas',
  label: 'Canvas',
  items: CANVAS_ITEMS,
};

/**
 * Resolve the active canvas surface from a router pathname via route-pattern
 * matching (architect lock, plan § Architecture locks #1):
 * - Outline  → `/works/:workId/outline`     — `startsWith('/works/') && includes('/outline')`
 * - World KB → `/worlds/:worldId/kb`        — `startsWith('/worlds/') && includes('/kb')`
 * - Strategy → `/strategies/:presetId`      — `startsWith('/strategies/')`
 *
 * Returns `null` for non-canvas paths and for the canvas **list** routes
 * (`/works`, `/strategies`) — those are pickers, not canvas surfaces.
 *
 * Pure: no React, no router, no side effects. Expects a bare pathname as
 * produced by `useLocation().pathname` (no query string / hash).
 */
export function resolveActiveCanvasSurface(pathname: string): CanvasSurfaceId | null {
  if (!pathname) {
    return null;
  }
  if (pathname.startsWith('/works/') && pathname.includes('/outline')) {
    return 'outline';
  }
  if (pathname.startsWith('/worlds/') && pathname.includes('/kb')) {
    return 'world-kb';
  }
  // `/strategies` (list) does not start with `/strategies/` and correctly
  // resolves to null; only `/strategies/:presetId` matches.
  if (pathname.startsWith('/strategies/')) {
    return 'strategy';
  }
  return null;
}
