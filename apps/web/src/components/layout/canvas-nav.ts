import { Globe, ListTree } from 'lucide-react';

import type { ShellNavItem } from './presentational/shell-sidebar-chrome';

/**
 * Canvas nav items + active-surface resolver.
 *
 * V1.111 grouped the three canvas surfaces (Outline / World KB / Strategy)
 * under a single "Canvas" {@link ShellNavGroup} disclosure. V1.117 removes that
 * group (AC-P2-3): Outline + World KB fold under the Creation (Creator) tab as
 * resolver-driven canvas items; Strategy moves to the Orchestration tab as a
 * plain `/strategies` link (AC-P2-4).
 *
 * Active-surface highlight derives from **route-pattern matching** via
 * {@link resolveActiveCanvasSurface}, NOT from a single Work param and NOT from
 * the chrome's built-in `item.to` prefix match (which is too broad — it would
 * light up "Outline" on plain `/works/:workId` work-detail).
 *
 * {@link CANVAS_ITEMS} holds the two surfaces still rendered via the resolver
 * path in the sidebar (Outline, World KB). The {@link CanvasSurfaceId} type and
 * resolvers still cover `strategy` (the `/strategies/:presetId` route remains a
 * canvas surface conceptually), but Strategy is no longer a sidebar canvas
 * item — it renders as a plain Orchestrator link.
 */

/** The canvas surfaces. Only `outline` + `world-kb` are sidebar items (V1.117);
 * `strategy` is still a canvas surface for the resolver, but renders as a plain
 * Orchestrator link. */
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
 * The canvas nav items rendered via the resolver path in the sidebar, in
 * display order (V1.117 regroup).
 *
 * Strategy is intentionally absent: it moved to the Orchestration tab as a
 * plain `/strategies` link (AC-P2-4). Outline + World KB remain
 * resolver-driven because their click target depends on the URL-scoped
 * Work/World context (see {@link resolveCanvasNavTarget}).
 *
 * Each `to` is the **entity-list entry point** for its surface — the place a
 * user lands to pick the context (Work / World) the surface renders:
 * - Outline   → `/works`      (pick a Work → `/works/:workId/outline`)
 * - World KB  → `/worlds`     (pick a World → `/worlds/:worldId/kb`)
 *
 * The static `to` is the chrome-keyed identity (and the resolver's domain is
 * the route pattern, not `to`). The actual click destination is computed by
 * {@link resolveCanvasNavTarget} (T3), which preserves the active Work/World
 * context when one is in the URL and falls back to the list picker otherwise.
 * This mirrors the command palette (`components/canvas/canvas-nav-commands.tsx`).
 *
 * World KB (V1.115 T3): the `/worlds` picker route IS registered in `App.tsx`.
 * When no `worldId` is in the URL, {@link resolveCanvasNavTarget} falls back to
 * `/worlds` so the sidebar item is always focusable and navigates to the world
 * picker. This does not affect the active-surface resolver, which keys off the
 * `/worlds/.../kb` pattern (the `/worlds` list itself resolves to null — it is
 * a picker, not a canvas surface).
 */
export const CANVAS_ITEMS: CanvasNavItem[] = [
  { to: '/works', label: 'Outline', icon: ListTree, surfaceId: 'outline' },
  { to: '/worlds', label: 'World KB', icon: Globe, surfaceId: 'world-kb' },
];

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

/**
 * The URL-scoped context a Canvas nav item navigates with. The app has no
 * global "active work / active world" — ids live in the route (`useParams`),
 * so this is exactly what {@link useParams} returns on the current match
 * (`undefined` when the id is not on the active route). Mirrors the context
 * model documented in `canvas-nav-commands.tsx`.
 */
export interface CanvasNavContext {
  workId?: string;
  worldId?: string;
}

/**
 * Compute the click destination for a Canvas nav item, preserving the active
 * Work/World context when one is in the URL (T3 navigation wiring).
 *
 * Contract — mirrors P0's command palette (`canvas-nav-commands.tsx`):
 * - **Outline**   → `/works/:workId/outline` when a `workId` is present;
 *   otherwise `/works` (the Work picker — a valid, always-registered route).
 * - **World KB**  → `/worlds/:worldId/kb` when a `worldId` is present;
 *   otherwise `/worlds` (the World picker — registered in V1.115 T3). The
 *   item is always focusable; it never renders disabled.
 * - **Strategy**  → `/strategies` always (the list is the always-valid entry
 *   point to the Strategy canvas).
 *
 * Ids are `encodeURIComponent`-encoded so a space-bearing id stays one path
 * segment (same as the palette).
 *
 * Pure: no React, no router, no side effects.
 */
export function resolveCanvasNavTarget(
  surfaceId: CanvasSurfaceId,
  ctx: CanvasNavContext,
): string | null {
  switch (surfaceId) {
    case 'outline':
      return ctx.workId
        ? `/works/${encodeURIComponent(ctx.workId)}/outline`
        : '/works';
    case 'world-kb':
      // No worldId in the URL → fall back to the `/worlds` picker (registered
      // in V1.115 T3). The item is always focusable; it never returns null.
      return ctx.worldId ? `/worlds/${encodeURIComponent(ctx.worldId)}/kb` : '/worlds';
    case 'strategy':
      return '/strategies';
  }
}
