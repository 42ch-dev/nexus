/**
 * Work Timeline canvas — orchestrator facade (V1.123 P2 Task 2 + Task 4).
 *
 * Slim composition root for the Work Timeline surface. Coordinates:
 *   - Graph read via the shared `useWorkOutline(workId)` hook (V1.72
 *     `GET .../works/{work_id}/outline` — the single Work-outline read
 *     endpoint).
 *   - Adapter projection via `useCanvasSurface` (V1.114 P0 recipe).
 *   - Task 4 layer state: Narrative ↔ Moment tabs in the canvas header.
 *     Active layer drives `createWorkTimelineCanvasAdapter(ctxRef, layer)`
 *     so `useCanvasSurface`'s `[graph, adapter]` memo re-projects on layer
 *     swap (semantic discrete swap per layer-feel-differentiation.md §3.1
 *     — not continuous viewport zoom).
 *   - **Default layer: `'narrative'`** (architect UX-risk override §7.3).
 *     Unlike the V1.122 World Timeline (which flips Brief↔Narrative based
 *     on era data), the Work Timeline default is UNCONDITIONALLY
 *     `'narrative'` in V1.123 because the V1.72 `WorkOutline` wire has no
 *     Scene/Beat data today. When the wire extends (V1.124+
 *     `DF-V1123-MOMENT-WIRE`), the default may flip to Moment.
 *   - Write boundary: NONE in V1.123 P2 (read-only). Edits route through
 *     the Outline surface via `onEditInOutline`; the orchestrator owns
 *     the navigation. The Work Timeline adapter ships no `onPatch*`
 *     callback. Forbidden in V1.123 (architect §6): direct scene/beat
 *     writes from the Work Timeline surface, raw-file writes.
 *
 * Peer-surface navigation: the header surfaces Work Timeline ↔ Outline
 * links so the author can pivot to the Outline (where the writes live).
 * Work entry stays Outline (V1.118 regression gate) — Work Timeline is a
 * peer reachable from the Work Canvas shell (Task 5 owns the route + nav
 * registration; Task 2 ships the canvas facade with the `workId` prop
 * contract ready).
 */
import { useMemo, useRef } from 'react';
import { useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import { Link, useNavigate, useSearchParams } from 'react-router-dom';

import { CanvasShell } from '@/components/canvas/canvas-shell';
import { LayerBreadcrumb } from '@/components/canvas/layer-breadcrumb';
import {
  useCanvasSurface,
  type CanvasSurfaceQueryResult,
} from '@/components/canvas/use-canvas-surface';
import { SemanticZoomBridge } from '@/components/canvas/use-semantic-zoom';
import { useWorkOutline } from '@/lib/canvas/use-outline-data';
import { useWork } from '@/api/queries';
import { LoadingState, ErrorState, EmptyState } from '@/components/ui/states';
import type { SceneBeatFixturePayload } from '../outline-canvas/graph-projection';

import {
  createWorkTimelineCanvasAdapter,
  type WorkTimelineCanvasAdapterContext,
  type WorkTimelineLayer,
} from './work-timeline-canvas-adapter';

export interface WorkTimelineCanvasProps {
  workId: string;
  /**
   * Optional V1.108 Scene/Beat fixture for the Moment layer (architect §3.2
   * + §3.4 — the V1.72 `WorkOutline` wire has no Scene/Beat data today;
   * Design Studio / test fixtures inject scene/beat payloads at the
   * projection layer). When undefined or empty, the Moment layer emits
   * honest empty-state (zero nodes) per architect §3.2.
   *
   * The fixture is forwarded to the adapter context so the adapter's
   * `projectGraphForLayer(graph, 'moment')` can read it at projection
   * time without breaking the `(graph: WorkOutline) => …` signature
   * locked by architect §7.1.
   */
  sceneBeatFixture?: SceneBeatFixturePayload;
}

/**
 * Work Timeline canvas facade.
 *
 * Task 2 shipped the minimal facade (loading / error / empty / CanvasShell
 * + adapter wiring with a fixed `'narrative'` active layer). Task 4 adds:
 *   - `useState<WorkTimelineLayer>` active layer (default `'narrative'`;
 *     V1.123 P4 Task 6 — now URL-driven via `?layer=` search param).
 *   - `WorkTimelineLayerSwitcher` inline component (segmented control).
 *   - `data-active-layer` testability hook on the root container.
 *   - Layer switcher hidden on the empty-state branch (Task 7 owns the
 *     visible empty-state copy).
 */
export function WorkTimelineCanvas({ workId, sceneBeatFixture }: WorkTimelineCanvasProps) {
  const { t } = useTranslation('canvas');
  const navigate = useNavigate();
  const outlineQuery = useWorkOutline(workId);

  // ── V1.123 P3 Task 4 — Work's bound World (cross-surface navigation) ────
  //
  // The Work detail carries an optional `world_id` (V1.72 WorkDetailResponse).
  // When present, the Narrative event inspector surfaces a "View on World
  // Timeline" affordance that navigates to the bound World's Timeline
  // Narrative layer. When absent, the affordance hides (honest scope cut per
  // plan §"If binding is missing or unreliable, P3 hides the affordance").
  //
  // `useWork` shares the TanStack Query cache with every other Work reader
  // (sidebar, Work detail page, etc.) so the extra fetch is typically a
  // cache hit on Work-scoped routes — no real network cost on the Work
  // Timeline entry. Stays enabled even while the outline is loading because
  // the world_id is independent of the outline projection.
  const workDetailQuery = useWork(workId);
  const boundWorldId = workDetailQuery.data?.world_id ?? undefined;

  // Cross-surface navigation hand-off. Composed once per render via
  // `useCallback`; the adapter context ref captures the latest closure so
  // the inspector's CTA always targets the current `boundWorldId`. The
  // callback is only referenced by the adapter context when `boundWorldId`
  // is present, so a stale-closure risk would not arise in practice — but
  // the memo keeps the referential shape stable for the ctxRef assignment.
  const onViewOnWorldTimeline = useCallback(() => {
    if (!boundWorldId) return;
    navigate(
      `/worlds/${encodeURIComponent(boundWorldId)}/timeline?layer=narrative`,
    );
  }, [boundWorldId, navigate]);

  // ── V1.123 P2 Task 4 + P4 Task 6 — layer state + URL persistence ────────
  //
  // Architect §7.3 UX-risk override: default = 'narrative' UNCONDITIONALLY
  // in V1.123. Unlike the V1.122 World Timeline (which flips Brief↔Narrative
  // based on era data), the Work Timeline default does NOT consult the
  // scene/beat fixture — even when a fixture is wired (Design Studio / test),
  // the surface stays Narrative-default so the UX does not flip between
  // real Works (no wire-scene/beat data today) and fixture-driven tests.
  //
  // V1.123 P4 Task 6 — layer-state persistence. The user-chosen layer is
  // encoded in the URL search param `?layer=narrative|moment`
  // (layer-feel-differentiation.md §5). The URL survives Work Timeline →
  // Outline → back round-trips + refresh. Invalid layer values for the
  // surface (`?layer=brief` — Brief is World-only) are ignored.
  //
  // When the user swaps back to the default Narrative layer, the URL param
  // is dropped so the URL stays minimal.
  const [searchParams, setSearchParams] = useSearchParams();
  const urlLayerRaw = searchParams.get('layer');
  const activeLayer: WorkTimelineLayer = useMemo(() => {
    if (urlLayerRaw === 'narrative' || urlLayerRaw === 'moment') {
      return urlLayerRaw;
    }
    // Invalid / absent → fall back to Narrative default. `brief` is
    // World-Timeline-only and is silently ignored here.
    return 'narrative';
  }, [urlLayerRaw]);

  const handleLayerChange = useCallback(
    (layer: WorkTimelineLayer) => {
      if (layer === 'narrative') {
        // Default layer — drop the URL param if present so the URL stays
        // minimal and the surface can resume tracking the (future)
        // architect-override default.
        if (searchParams.has('layer')) {
          const next = new URLSearchParams(searchParams);
          next.delete('layer');
          setSearchParams(next, { replace: false });
        }
      } else {
        const next = new URLSearchParams(searchParams);
        next.set('layer', layer);
        setSearchParams(next, { replace: false });
      }
    },
    [searchParams, setSearchParams],
  );

  const surfaceQuery = useMemo<CanvasSurfaceQueryResult<NonNullable<typeof outlineQuery.data>>>(
    () => ({
      data: outlineQuery.data,
      isLoading: outlineQuery.isLoading,
      isError: outlineQuery.isError,
      error: outlineQuery.error,
      refetch: () => {
        void outlineQuery.refetch();
      },
    }),
    [
      outlineQuery.data,
      outlineQuery.isLoading,
      outlineQuery.isError,
      outlineQuery.error,
      outlineQuery.refetch,
    ],
  );

  const ctxRef = useRef<WorkTimelineCanvasAdapterContext>({
    workId,
  });

  // Keep the adapter context current. The adapter object stays
  // referentially stable; only the values inside ctxRef.current change.
  // Task 4 wires the `sceneBeatFixture` slot so the Moment projection
  // reads from the orchestrator-supplied fixture (V1.108 carrier pattern).
  //
  // V1.123 P3 Task 4 — also wires the cross-surface navigation slots
  // (`worldId` + `onViewOnWorldTimeline`) so the Narrative event inspector
  // can render the "View on World Timeline" affordance. The callback is
  // only forwarded when a bound World exists (the inspector hides the CTA
  // otherwise — honest scope cut).
  ctxRef.current = {
    workId,
    sceneBeatFixture,
    worldId: boundWorldId,
    onViewOnWorldTimeline: boundWorldId ? onViewOnWorldTimeline : undefined,
  };

  // Rebuild the adapter on layer swap so `useCanvasSurface`'s `[graph,
  // adapter]` memo re-projects (semantic discrete swap per layer-feel §3.1).
  // The ctxRef stays mutable; the adapter just captures the new layer
  // value, so this is a cheap factory re-run, not a full layout rebuild.
  const adapter = useMemo(
    () => createWorkTimelineCanvasAdapter(ctxRef, activeLayer),
    [activeLayer],
  );

  const surface = useCanvasSurface(adapter, surfaceQuery);

  // ── Render ────────────────────────────────────────────────────────────────

  if (outlineQuery.isLoading) {
    return <LoadingState label={t('workTimeline.loading', { defaultValue: 'Loading Work Timeline…' })} />;
  }
  if (outlineQuery.isError) {
    return (
      <ErrorState
        description={t('workTimeline.loadError', {
          defaultValue: 'Could not load the work timeline.',
        })}
        onRetry={() => outlineQuery.refetch()}
      />
    );
  }

  const outline = outlineQuery.data;
  const isEmpty = !outline || (outline.timeline_events ?? []).length === 0;

  // V1.123 P2 Task 7 — Moment-empty detection. Active layer is Moment AND
  // the projection returned zero nodes (no scene/beat fixture / empty
  // fixture). The graph itself is NOT globally empty here (the global empty
  // branch above owns zero-event outlines). The Moment-empty branch only
  // fires when there are events on Narrative but no scenes/beats on Moment —
  // the CTA "Switch to Narrative" surfaces the events.
  //
  // layer-feel §2.4 + §7 + plan Task 7: render an honest Moment-empty panel
  // with a CTA back to Narrative instead of an empty spatial canvas.
  const isMomentEmpty =
    !isEmpty && activeLayer === 'moment' && surface.nodes.length === 0;

  return (
    <div
      className="flex flex-col gap-3"
      data-testid="work-timeline-canvas"
      data-active-layer={activeLayer}
    >
      <WorkTimelineCanvasHeader
        workId={workId}
        activeLayer={activeLayer}
        onLayerChange={handleLayerChange}
        showLayerSwitcher={!isEmpty}
      />

      {isEmpty ? (
        <EmptyState
          title={t('workTimeline.empty.title', {
            defaultValue: "This work's timeline is empty",
          })}
          description={t('workTimeline.empty.description', {
            defaultValue:
              'Outline events you add through the Outline surface will appear here.',
          })}
        />
      ) : isMomentEmpty ? (
        <MomentEmptyState onSwitchToNarrative={() => handleLayerChange('narrative')} />
      ) : (
        // V1.123 P4 Task 4 — layer transition animation. The `key` forces a
        // remount on layer swap so the CSS keyframe animation replays; the
        // `nexus-layer-enter` class carries the keyframe (fade + subtle scale
        // per layer-feel-differentiation.md §4 "changing instrument"). The
        // global `prefers-reduced-motion` rule in `apps/web/src/index.css`
        // collapses animation-duration to 0.01ms so reduced-motion users get
        // an instant swap. Viewport continuity survives via
        // `useCanvasViewport`'s module-level cache (surfaceKey="work-timeline"
        // is constant across layers).
        <div key={activeLayer} className="nexus-layer-enter" data-testid="work-timeline-canvas-layer-transition">
          <CanvasShell
            nodes={surface.nodes}
            edges={surface.edges}
            nodeTypes={surface.nodeTypes}
            onNodesChange={surface.onNodesChange}
            summaryText={surface.summaryText}
            ariaLabel={t('workTimeline.canvasAriaLabel', {
              defaultValue: 'Work timeline canvas',
            })}
            surfaceKey="work-timeline"
            surfaceKind="work-timeline"
            relayout={surface.relayout}
          >
            {/* V1.123 P4 Task 3 — semantic zoom bridge. Mounts inside
                CanvasShell so it lives within the ReactFlowProvider; observes
                viewport zoom and fires `handleLayerChange` when the user crosses
                the architect-locked 0.55–0.70 hysteresis band
                (layer-feel-differentiation.md §3.3). The bridge renders
                nothing visible — purely a hook host. Coexists with the
                explicit Narrative ↔ Moment layer tabs (the primary
                affordance per plan Global Constraints). */}
            <SemanticZoomBridge
              activeLayer={activeLayer}
              onLayerChange={handleLayerChange}
              chain={{ coarseLayer: 'narrative', fineLayer: 'moment' }}
            />
            {/* Task 6 — Work Timeline inspector overlay. Renders when
                `useCanvasSurface`'s selection state resolves to a node; the
                adapter's `renderInspector` dispatches by `nodeKind`
                (event / scene / beat). Read-only in V1.123 (architect §6). */}
            {surface.inspector ? (
              <div className="pointer-events-auto absolute right-3 top-3 w-[340px] max-w-[calc(100%-1.5rem)] rounded-card border border-gray-alpha-400 bg-background-100 p-4 shadow-popover">
                {surface.inspector}
              </div>
            ) : null}
          </CanvasShell>
        </div>
      )}
    </div>
  );
}

/**
 * Canvas header — surfaces the Work Timeline label + the Narrative ↔ Moment
 * layer switcher (Task 4) + the Outline peer-link (Task 5 — Work Canvas shell
 * peer nav). The Outline link is the canonical escape hatch back to where
 * the writes live (architect §6 — Work Timeline is read-only in V1.123).
 */
function WorkTimelineCanvasHeader({
  workId,
  activeLayer,
  onLayerChange,
  showLayerSwitcher,
}: {
  workId: string;
  activeLayer: WorkTimelineLayer;
  onLayerChange: (layer: WorkTimelineLayer) => void;
  /**
   * Gates the layer switcher visibility. The empty-state branch owns its
   * own surface; the layer tabs add noise without value when the outline
   * is empty (Task 7 owns the per-layer empty-state copy).
   */
  showLayerSwitcher: boolean;
}) {
  const { t } = useTranslation('canvas');
  return (
    <div className="flex flex-wrap items-center justify-between gap-2">
      <div>
        <h2 className="text-heading-16 font-heading text-gray-1000">
          {t('workTimeline.header.title', { defaultValue: 'Work Timeline' })}
        </h2>
        <p className="text-copy-13 text-gray-700">
          {t('workTimeline.header.description', {
            defaultValue: 'The work’s narrative + moments. Pivots to Outline for edits.',
          })}
        </p>
      </div>
      <div className="flex flex-wrap items-center gap-2">
        {/* V1.123 P4 Task 5 — layer breadcrumb. Shows the layer path
            (Narrative, or Narrative > Moment when drilled). Mirrors the
            Timeline surface's Brief ↔ Narrative breadcrumb pattern. */}
        {showLayerSwitcher ? (
          <LayerBreadcrumb
            surfaceKey="work-timeline"
            coarseSegment={{
              layer: 'narrative',
              labelKey: 'workTimeline.layerSwitcher.narrative',
              defaultValue: 'Narrative',
            }}
            fineSegment={{
              layer: 'moment',
              labelKey: 'workTimeline.layerSwitcher.moment',
              defaultValue: 'Moment',
            }}
            activeLayer={activeLayer}
            onLayerChange={onLayerChange}
            ariaLabelKey="workTimeline.breadcrumb.ariaLabel"
            ariaLabelDefaultValue="Work Timeline layer path"
          />
        ) : null}
        {showLayerSwitcher ? (
          <WorkTimelineLayerSwitcher
            activeLayer={activeLayer}
            onLayerChange={onLayerChange}
          />
        ) : null}
        {/* Task 5 — peer-link to Outline. Work Timeline is read-only in
            V1.123 (architect §6); edits route through the Outline surface.
            The link preserves the active `workId` so the pivot is
            zero-friction. Mirrors the V1.122 Timeline peer-nav pattern
            (worldKbLink / strategyLink). */}
        <nav
          className="flex flex-wrap items-center gap-2"
          aria-label={t('workTimeline.header.peerNavAria', {
            defaultValue: 'Peer surfaces',
          })}
        >
          <Link
            to={`/works/${encodeURIComponent(workId)}/outline`}
            className="rounded-control border border-gray-alpha-400 bg-background-100 px-3 py-1.5 text-button-12 text-gray-900 shadow-elevation-2 hover:bg-gray-alpha-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-700 focus-visible:ring-offset-2"
          >
            {t('workTimeline.header.outlineLink', { defaultValue: 'Outline' })}
          </Link>
        </nav>
      </div>
    </div>
  );
}

/**
 * V1.123 P2 Task 4 — Narrative ↔ Moment layer switcher
 * (layer-feel-differentiation.md §3.3 explicit layer control).
 *
 * Inline segmented control (two buttons with `aria-pressed`). Built inline
 * rather than promoting to `packages/nexus-ui` because:
 *   - The set of layers + the active-layer discriminator are Work-Timeline-
 *     surface-specific (not a generic primitive).
 *   - YAGNI — the V1.122 World Timeline ships its own inline
 *     `TimelineLayerSwitcher` for Brief ↔ Narrative; P4 may promote a
 *     generic `SegmentedControl` if more layers arrive, but the per-surface
 *     inline control is the durable slice for V1.123.
 *
 * Accessibility: each button carries `aria-pressed` so screen readers
 * announce the active layer as a toggle state (WCAG 2.1 — semantic
 * pressed state for toggle buttons). The group wraps in a `role="group"`
 * with an i18n label so SR users can navigate to the switcher by name.
 *
 * `simplify:` the inline buttons reuse the existing header button styling
 * (border + bg + shadow + focus ring) for visual consistency with the
 * V1.122 Timeline layer switcher. A bespoke segmented-control visual
 * (sliding indicator, etc.) is P4 polish territory (layer-feel §4 motion
 * contract).
 */
function WorkTimelineLayerSwitcher({
  activeLayer,
  onLayerChange,
}: {
  activeLayer: WorkTimelineLayer;
  onLayerChange: (layer: WorkTimelineLayer) => void;
}) {
  const { t } = useTranslation('canvas');
  const layers: Array<{
    layer: WorkTimelineLayer;
    testId: string;
    labelKey: string;
    defaultValue: string;
  }> = [
    {
      layer: 'narrative',
      testId: 'work-timeline-layer-tab-narrative',
      labelKey: 'workTimeline.layerSwitcher.narrative',
      defaultValue: 'Narrative',
    },
    {
      layer: 'moment',
      testId: 'work-timeline-layer-tab-moment',
      labelKey: 'workTimeline.layerSwitcher.moment',
      defaultValue: 'Moment',
    },
  ];
  return (
    <div
      role="group"
      aria-label={t('workTimeline.layerSwitcher.ariaLabel', {
        defaultValue: 'Work Timeline layer',
      })}
      className="flex items-center gap-1 rounded-control border border-gray-alpha-400 bg-background-100 p-0.5"
    >
      {layers.map(({ layer, testId, labelKey, defaultValue }) => {
        const pressed = activeLayer === layer;
        return (
          <button
            key={layer}
            type="button"
            data-testid={testId}
            aria-pressed={pressed}
            onClick={() => onLayerChange(layer)}
            className={
              pressed
                ? 'rounded-control bg-gray-alpha-200 px-3 py-1 text-button-12 font-semibold text-gray-1000 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-700 focus-visible:ring-offset-2'
                : 'rounded-control px-3 py-1 text-button-12 text-gray-700 hover:bg-gray-alpha-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-700 focus-visible:ring-offset-2'
            }
          >
            {t(labelKey, { defaultValue })}
          </button>
        );
      })}
    </div>
  );
}

/**
 * V1.123 P2 Task 7 — Moment-layer honest empty-state.
 *
 * Renders when the active layer is Moment but the projection has zero nodes
 * (no V1.108 scene/beat fixture; or fixture is empty). The V1.72 `WorkOutline`
 * wire has no scene/beat data today (architect §3.4), so any Work without a
 * Design Studio / test fixture surfaces this panel when the user clicks the
 * Moment tab.
 *
 * Surfaces the layer-feel §7 copy + a CTA back to Narrative — the actionable
 * escape hatch from an empty Moment world. Built on the shared `EmptyState`
 * primitive so the visual treatment matches every other authoring empty-state
 * in the app. Mirrors the V1.123 P1 BriefEmptyState pattern verbatim
 * (canvas/timeline-canvas/timeline-canvas.tsx §`BriefEmptyState`).
 *
 * The CTA uses a primary-action button so keyboard + SR users have a direct
 * escape hatch; it calls `onSwitchToNarrative` (the orchestrator's
 * `setActiveLayer('narrative')`) — no navigation, no write.
 */
function MomentEmptyState({
  onSwitchToNarrative,
}: {
  onSwitchToNarrative: () => void;
}) {
  const { t } = useTranslation('canvas');
  return (
    <div
      data-testid="work-timeline-moment-empty-state"
      className="rounded-card border border-gray-alpha-400 bg-background-100"
    >
      <EmptyState
        title={t('workTimeline.moment.emptyState.title', {
          defaultValue: 'No scene or beat data yet',
        })}
        description={t('workTimeline.moment.emptyState.message', {
          defaultValue:
            'Moment is scene-precise and manuscript-anchored. Add scenes and beats in Outline, or switch to Narrative for events.',
        })}
        action={
          <button
            type="button"
            data-testid="work-timeline-moment-empty-cta"
            onClick={onSwitchToNarrative}
            className="rounded-control bg-blue-700 px-4 py-2 text-button-14 font-semibold text-white-100 shadow-elevation-2 hover:bg-blue-800 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-700 focus-visible:ring-offset-2"
          >
            {t('workTimeline.moment.emptyState.cta', {
              defaultValue: 'Switch to Narrative',
            })}
          </button>
        }
      />
    </div>
  );
}
