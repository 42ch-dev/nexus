/**
 * Global Timeline page — route entry for the cross-World Timeline overview
 * (V1.123 P3 Task 1).
 *
 * Renders {@link GlobalTimelineView} — recent Timeline activity composed
 * client-side across all Worlds. The page is thin so the view can be tested
 * in isolation. Per-World Timeline entry stays at
 * `/worlds/:worldId/timeline` (V1.122 P1 T3 — the hero surface); this global
 * view is the primary-nav entry that complements it.
 *
 * Route-split: this page is part of the Control Room bootstrap chunk (no
 * `@xyflow/react` import), so it is NOT lazy-loaded alongside the canvas
 * routes. The global view is a list, not a spatial canvas — keeping it in
 * the bootstrap chunk lets the primary-nav entry land instantly.
 */
import { GlobalTimelineView } from '@/components/global-timeline/global-timeline-view';

export function GlobalTimelinePage() {
  return <GlobalTimelineView />;
}
