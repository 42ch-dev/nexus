/**
 * Canvas navigation commands — registers the "Go to <surface>" palette actions
 * (V1.111 P0 T4 + V1.123 P2 T5).
 *
 * Mounted once in {@link RootLayout} so the canvas-surface entry routes
 * are reachable from the command palette (⌘K) wherever the user is:
 *
 *   - **Go to Strategies** → `/strategies` (always available; the list is the
 *     entry point to the Strategy canvas, mirroring the sidebar).
 *   - **Go to Outline** → `/works/:workId/outline` (only when a `workId` is in
 *     the current URL).
 *   - **Go to World KB** → `/worlds/:worldId/kb` (only when a `worldId` is in
 *     the current URL).
 *   - **Go to Work Timeline** → `/works/:workId/timeline` (V1.123 P2 T5; only
 *     when a `workId` is in the current URL). Work Timeline is a peer surface
 *     of Outline from the Work Canvas shell; the command is workId-gated so
 *     it surfaces only on Work-scoped routes (mirrors `go.outline`).
 *
 * **Context model:** the app has no global "active work / active world" — ids
 * are URL-scoped (`useParams`). The Outline, Work Timeline, and World KB
 * routes require an id, so those commands are gated by `available?()` and
 * hidden when the id is not present on the current match. This mirrors the
 * Work detail page, which links into the same routes with
 * `encodeURIComponent(id)`.
 *
 * **Live-handler pattern:** {@link useRegisterCommand} captures a command once
 * per mount (keyed by `id`); field changes after mount are ignored by design
 * (registry thrash avoidance — see `command-registry.ts` docblock). Because the
 * user navigates between routes without this component unmounting, the handlers
 * and predicates read the latest ids through a ref updated on every render, so
 * "Go to Outline" always targets the current work, not the one captured at
 * first mount.
 */
import { useRef } from 'react';
import { useNavigate, useParams } from 'react-router-dom';
import { CalendarRange, ListTree, Network, Sparkles, Clock } from 'lucide-react';

import { useRegisterCommand } from '@/lib/canvas/command-registry';

export function CanvasNavCommands(): null {
  const navigate = useNavigate();
  // `useParams` in a layout-level component returns the leaf route's params,
  // so `workId`/`worldId` are populated on Work- and World-scoped routes and
  // `undefined` elsewhere.
  const { workId, worldId } = useParams<{ workId?: string; worldId?: string }>();

  // Ref so handlers/predicates (captured once on mount) read current values.
  const idsRef = useRef({ workId, worldId });
  idsRef.current = { workId, worldId };

  // V1.123 P3 Task 2 — global Timeline entry in the command palette.
  // Registered FIRST so authors discover the central instrument from any
  // route (one-click reachability per AC-V1123-16). Always available —
  // `/timeline` is a primary-nav route with no path params.
  useRegisterCommand({
    id: 'go.timeline',
    labelKey: 'go.timeline.label',
    groupKey: 'group.navigate',
    keywordKeys: [
      'go.timeline.keywords.worlds',
      'go.timeline.keywords.brief',
      'go.timeline.keywords.narrative',
      'go.timeline.keywords.recent',
    ],
    icon: Clock,
    handler: () => navigate('/timeline'),
  });

  useRegisterCommand({
    id: 'go.strategy',
    labelKey: 'go.strategy.label',
    groupKey: 'group.navigate',
    keywordKeys: [
      'go.strategy.keywords.preset',
      'go.strategy.keywords.state-machine',
      'go.strategy.keywords.canvas',
      'go.strategy.keywords.sparkles',
    ],
    icon: Sparkles,
    handler: () => navigate('/strategies'),
  });

  useRegisterCommand({
    id: 'go.outline',
    labelKey: 'go.outline.label',
    groupKey: 'group.navigate',
    keywordKeys: [
      'go.outline.keywords.chapters',
      'go.outline.keywords.timeline',
      'go.outline.keywords.work-canvas',
      'go.outline.keywords.structure',
    ],
    icon: ListTree,
    handler: () => {
      const { workId: w } = idsRef.current;
      if (w) navigate(`/works/${encodeURIComponent(w)}/outline`);
    },
    available: () => Boolean(idsRef.current.workId),
  });

  useRegisterCommand({
    id: 'go.world-kb',
    labelKey: 'go.world-kb.label',
    groupKey: 'group.navigate',
    keywordKeys: [
      'go.world-kb.keywords.entities',
      'go.world-kb.keywords.relationships',
      'go.world-kb.keywords.world-canvas',
      'go.world-kb.keywords.lore',
    ],
    icon: Network,
    handler: () => {
      const { worldId: w } = idsRef.current;
      if (w) navigate(`/worlds/${encodeURIComponent(w)}/kb`);
    },
    available: () => Boolean(idsRef.current.worldId),
  });

  // V1.123 P2 T5 — Work Timeline peer surface. Mirrors `go.outline`'s
  // workId-gating so the command surfaces only on Work-scoped routes. Work
  // entry stays Outline (V1.118 regression) — the command does NOT flip the
  // `/works/:workId` index redirect; it exposes Work Timeline as a peer at
  // `/works/:workId/timeline` reachable from ⌘K.
  useRegisterCommand({
    id: 'go.work-timeline',
    labelKey: 'go.work-timeline.label',
    groupKey: 'group.navigate',
    keywordKeys: [
      'go.work-timeline.keywords.narrative',
      'go.work-timeline.keywords.moment',
      'go.work-timeline.keywords.scenes',
      'go.work-timeline.keywords.beats',
    ],
    icon: CalendarRange,
    handler: () => {
      const { workId: w } = idsRef.current;
      if (w) navigate(`/works/${encodeURIComponent(w)}/timeline`);
    },
    available: () => Boolean(idsRef.current.workId),
  });

  return null;
}
