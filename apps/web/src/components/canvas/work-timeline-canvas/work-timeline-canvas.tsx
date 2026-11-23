/**
 *
 * Slim composition root for the Work Timeline surface. Coordinates:
 *   - Graph read via the shared `useWorkOutline(workId)` hook (V1.72
 *     `GET .../works/{work_id}/outline` — the single Work-outline read
 *     endpoint).
 *   - Adapter projection via `useCanvasSurface` (V1.114 P0 recipe).
 *   - Task 4 layer state + V1.156 P2 T2: Brief | Narrative | Moment tabs
 *     in the canvas header (Brief added V1.156 — read-only projection of
 *     the bound World's Brief, PD-2). Active layer drives
 *     `createWorkTimelineCanvasAdapter(ctxRef, layer, boundWorldGraph)`
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
import { useCallback, useMemo, useRef } from 'react';
import { useTranslation } from 'react-i18next';
import { Link, useNavigate, useSearchParams } from 'react-router';
import type { Node } from '@xyflow/react';

import { CanvasShell } from '@/components/canvas/canvas-shell';
import { LayerBreadcrumb } from '@/components/canvas/layer-breadcrumb';
import {
  useCanvasSurface,
  type CanvasSurfaceQueryResult,
} from '@/components/canvas/use-canvas-surface';
import { useInboundEventFocus } from '@/components/canvas/use-inbound-event-focus';
import { SemanticZoomBridge } from '@/components/canvas/use-semantic-zoom';
import { useWorkOutline } from '@/lib/canvas/use-outline-data';
import { useWorldKbGraph } from '@/lib/canvas/use-world-kb-data';
import { useWork } from '@/api/queries';
import { LoadingState, ErrorState, EmptyState } from '@/components/ui/states';
import { Button } from '@42ch/nexus-ui';
import type { SceneBeatFixturePayload } from '../outline-canvas/graph-projection';

import {
  createWorkTimelineCanvasAdapter,
  narrativeEventNodeId,
  type WorkTimelineCanvasAdapterContext,
  type WorkTimelineLayer,
  type WorkTimelineNodeData,
} from './work-timeline-canvas-adapter';
import { NleTimelineBandOverlay } from '../timeline-canvas/nle-timeline-band-overlay';
import { filterTimelineEntityNodes } from '../timeline-canvas/nle-timeline-projection';

export interface WorkTimelineCanvasProps {
  workId: string;
  /**
   * The fixture is forwarded to the adapter context so the adapter's
   * `projectGraphForLayer(graph, 'moment')` can read it at projection
   * time without breaking the `(graph: WorkOutline) => …` signature
   * locked by architect §7.1.
   */
  sceneBeatFixture?: SceneBeatFixturePayload;
}

/**
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

  // Work-Brief is a read-only **projection** of the bound World's Brief
  // (PD-2): Brief remains World spine; the Work does NOT gain an authored
  // Brief. The Brief layer composes from the bound World's
  // `GET /v1/daemon/worlds/{world_id}/kb/graph` (V1.73 — no new route)
  // via `Work.world_id` (already resolved above). `useWorldKbGraph` is
  // disabled when no World is bound (`enabled: Boolean(worldId)` inside
  // the hook), so `boundWorldGraph` stays undefined for unbound Works →
  // the Brief projection emits zero nodes (honest empty-state, T2 owns
  // the visible copy).
  const worldKbGraphQuery = useWorldKbGraph(boundWorldId);
  const boundWorldGraph = worldKbGraphQuery.data;

  // V1.163 P1 Task 3 — event-level forward deep-link (PD-5 three-state
  // matrix). The callback now receives the selected Work Timeline node and
  // reads `node.data.worldEventId` (projected from the Task 1 carrier
  // `WorkOutline.timeline_events[].world_event_id` — the World KB entity
  // `key_block_id`):
  //   1. Event-level bind → `/worlds/:worldId/timeline?layer=narrative&event=<worldEventId>`
  //      (the World Timeline selects `entity:<id>` on arrival — Task 2).
  //   2. Surface bind only → V1.123 fallback `?layer=narrative` (no event).
  //   3. No bind → callback not wired (`boundWorldId` undefined) → CTA hidden.
  const onViewOnWorldTimeline = useCallback(
    (node: Node<WorkTimelineNodeData>) => {
      if (!boundWorldId) return;
      const worldEventId = node.data.worldEventId;
      if (worldEventId) {
        navigate(
          `/worlds/${encodeURIComponent(boundWorldId)}/timeline?layer=narrative&event=${encodeURIComponent(worldEventId)}`,
        );
        return;
      }
      navigate(
        `/worlds/${encodeURIComponent(boundWorldId)}/timeline?layer=narrative`,
      );
    },
    [boundWorldId, navigate],
  );

  // Architect §7.3 UX-risk override: default = 'narrative' UNCONDITIONALLY
  // in V1.123. Unlike the V1.122 World Timeline (which flips Brief↔Narrative
  // based on era data), the Work Timeline default does NOT consult the
  // scene/beat fixture — even when a fixture is wired (Design Studio / test),
  // the surface stays Narrative-default so the UX does not flip between
  // real Works (no wire-scene/beat data today) and fixture-driven tests.
  //
  // V1.123 P4 Task 6 + V1.156 P2 T2 — layer-state persistence. The
  // user-chosen layer is encoded in the URL search param
  // `?layer=brief|narrative|moment` (layer-feel-differentiation.md §5 +
  // spec §3.3.3 layer-state persistence). The URL survives Work Timeline →
  // Outline → back round-trips + refresh.
  //
  // V1.156 spec §3.3.3: all three layer values are valid on the Work
  // Timeline — the V1.123 "Brief is World-only" restriction is lifted
  // (Work×Brief cell closed). Unknown layer values (`?layer=foo`) fall
  // back to the surface default.
  //
  // When the user swaps back to the default Narrative layer, the URL param
  // is dropped so the URL stays minimal.
  const [searchParams, setSearchParams] = useSearchParams();
  const urlLayerRaw = searchParams.get('layer');
  const activeLayer: WorkTimelineLayer = useMemo(() => {
    if (
      urlLayerRaw === 'brief' ||
      urlLayerRaw === 'narrative' ||
      urlLayerRaw === 'moment'
    ) {
      return urlLayerRaw;
    }
    return 'narrative';
  }, [urlLayerRaw]);

  const handleLayerChange = useCallback(
    (layer: WorkTimelineLayer) => {
      if (layer === 'narrative') {
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

  // V1.123 P3 Task 4 — also wires the cross-surface navigation slots
  // (`worldId` + `onViewOnWorldTimeline`) so the Narrative event inspector
  // can render the "View on World Timeline" affordance. The callback is
  // only forwarded when a bound World exists (the inspector hides the CTA
  // otherwise — honest scope cut).
  //
  // V1.156 P2 T2 — also wires the `boundWorldGraph` slot (the bound World's
  // KB graph). The Brief layer reads era entities from it; the adapter
  // factory's captured value takes precedence, so this slot is the ctxRef
  // fallback (tests / direct wiring).
  ctxRef.current = {
    workId,
    sceneBeatFixture,
    worldId: boundWorldId,
    boundWorldGraph,
    onViewOnWorldTimeline: boundWorldId ? onViewOnWorldTimeline : undefined,
  };

  // V1.156 P2 T2 — the adapter ALSO captures the bound World's KB graph
  // (P1 fix-wave lesson F-3 applied proactively): data-driven re-projection
  // requires the graph in the memo deps — a graph identity change (refetch)
  // recreates the adapter and re-projects the Brief layer without a layer
  // swap. Mirrors the World adapter's captured-fixture pattern.
  const adapter = useMemo(
    () => createWorkTimelineCanvasAdapter(ctxRef, activeLayer, boundWorldGraph),
    [activeLayer, boundWorldGraph],
  );

  const surface = useCanvasSurface(adapter, surfaceQuery);

  // The REVERSE-direction destination: a World→Work deep-link
  // (`/worlds/... → /works/:workId/timeline?layer=narrative&event=<workEventId>`,
  // Task 2) lands here. Reads `?event=` via the existing `useSearchParams()`
  // plumbing and selects the React Flow node `wt-event:${eventParam}` after
  // projection (the same node id `projectNarrativeLayer` emits via
  // `narrativeEventNodeId(event_id)`). Selection drives the existing
  // inspector — no new focus primitive.
  //
  // Honest empty focus + the per-URL one-shot guard live in the shared hook
  // (the same one the World Timeline's Task 2 effect uses). Its `wt-event:`
  // id namespace is event-exclusive, so no extra target rule is needed.
  const eventParamRaw = searchParams.get('event');
  const inboundEventNodeId =
    eventParamRaw && eventParamRaw.length > 0 ? narrativeEventNodeId(eventParamRaw) : null;
  useInboundEventFocus(surface, inboundEventNodeId);

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

  const isBriefGraphPending =
    !isEmpty && activeLayer === 'brief' && Boolean(boundWorldId);

  if (isBriefGraphPending && worldKbGraphQuery.isLoading) {
    return <LoadingState label={t('workTimeline.loading', { defaultValue: 'Loading Work Timeline…' })} />;
  }
  if (isBriefGraphPending && worldKbGraphQuery.isError) {
    return (
      <ErrorState
        description={t('workTimeline.loadError', {
          defaultValue: 'Could not load the work timeline.',
        })}
        onRetry={() => worldKbGraphQuery.refetch()}
      />
    );
  }

  const isBriefEmpty =
    !isEmpty && activeLayer === 'brief' && surface.nodes.length === 0;

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
      ) : isBriefEmpty ? (
        <BriefEmptyState onSwitchToNarrative={() => handleLayerChange('narrative')} />
      ) : isMomentEmpty ? (
        <MomentEmptyState onSwitchToNarrative={() => handleLayerChange('narrative')} />
      ) : activeLayer === 'brief' && surface.briefTimeBands ? (
        // This branch sits BEFORE the spatial-canvas branch so the
        // time-band panel is the Brief-layer rendering model (the
        // Work canvas has no alt-view, so the World T2-M2 precedence fix
        // does not apply here).
        <div
          key="brief-time-bands"
          className="nexus-layer-enter"
          data-testid="work-timeline-canvas-layer-transition"
        >
          <div className="grid gap-3 lg:grid-cols-[1fr_360px]">
            <div className="rounded-card border border-gray-alpha-400 bg-background-100 p-4 shadow-elevation-1">
              {surface.briefTimeBands}
            </div>
            {surface.inspector ? (
              <div className="rounded-card border border-gray-alpha-400 bg-background-100 p-4 shadow-popover">
                {surface.inspector}
              </div>
            ) : null}
          </div>
          {/* Screen-reader graph summary — parity with CanvasShell (A8 #3). */}
          <div className="sr-only" role="status" aria-live="polite" aria-atomic="true">
            {surface.summaryText}
          </div>
        </div>
      ) : (
        <div key={activeLayer} className="nexus-layer-enter" data-testid="work-timeline-canvas-layer-transition">
          <CanvasShell
            nodes={filterTimelineEntityNodes(surface.nodes)}
            edges={surface.edges}
            nodeTypes={surface.nodeTypes}
            onNodesChange={surface.onNodesChange}
            summaryText={surface.summaryText}
            ariaLabel={t('workTimeline.canvasAriaLabel', {
              defaultValue: 'Work timeline canvas',
            })}
            surfaceKey="work-timeline"
            surfaceKind="work-timeline"
            minimapAccent="var(--color-canvas-timeline-accent)"
            relayout={surface.relayout}
            fitViewOptions={{
              nodes: filterTimelineEntityNodes(surface.nodes),
            }}
          >
            <NleTimelineBandOverlay
              nodes={surface.nodes}
              surface="work"
              activeLayer={activeLayer}
              scrollAriaLabel={t('workTimeline.nleBandScrollAriaLabel', {
                defaultValue: 'Work Timeline scrub area',
              })}
            />
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

function WorkTimelineCanvasHeader({
  workId,
  activeLayer,
  onLayerChange,
  showLayerSwitcher,
}: {
  workId: string;
  activeLayer: WorkTimelineLayer;
  onLayerChange: (layer: WorkTimelineLayer) => void;
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
            defaultValue:
              'The work’s timeline across Brief, Narrative, and Moment layers. Pivots to Outline for edits.',
          })}
        </p>
      </div>
      <div className="flex flex-wrap items-center gap-2">
        {/* V1.123 P4 Task 5 + V1.156 P2 T2 — layer breadcrumb. Shows the
            layer path (Brief, or Brief > Narrative when drilled). Mirrors
            the Timeline surface's Brief ↔ Narrative breadcrumb pattern —
            Brief is the coarsest (world-shape) layer, Narrative the layer
            below it; Moment sits outside the coarse/fine pair (same as the
            World Timeline's Moment — reached via the switcher tabs). */}
        {showLayerSwitcher ? (
          <LayerBreadcrumb
            surfaceKey="work-timeline"
            coarseSegment={{
              layer: 'brief',
              label: t('workTimeline.layerSwitcher.brief', {
                defaultValue: 'Brief',
              }),
            }}
            fineSegment={{
              layer: 'narrative',
              label: t('workTimeline.layerSwitcher.narrative', {
                defaultValue: 'Narrative',
              }),
            }}
            activeLayer={activeLayer}
            onLayerChange={onLayerChange}
            ariaLabel={t('workTimeline.breadcrumb.ariaLabel', {
              defaultValue: 'Work Timeline layer path',
            })}
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
          {/* V1.151 P1 (DF-76) — Assembly Inspector peer link. Read-only
              moment-level debug surface at `/works/:workId/inspector`; the
              same peer-nav pattern as the Outline link. */}
          <Link
            to={`/works/${encodeURIComponent(workId)}/inspector`}
            className="rounded-control border border-gray-alpha-400 bg-background-100 px-3 py-1.5 text-button-12 text-gray-900 shadow-elevation-2 hover:bg-gray-alpha-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-700 focus-visible:ring-offset-2"
          >
            {t('workTimeline.header.inspectorLink', { defaultValue: 'Assembly Inspector' })}
          </Link>
        </nav>
      </div>
    </div>
  );
}

/**
 * Inline segmented control (three buttons with `aria-pressed`). Built inline
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
      layer: 'brief',
      testId: 'work-timeline-layer-tab-brief',
      labelKey: 'workTimeline.layerSwitcher.brief',
      defaultValue: 'Brief',
    },
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
          <Button
            type="button"
            variant="primary"
            data-testid="work-timeline-moment-empty-cta"
            onClick={onSwitchToNarrative}
          >
            {t('workTimeline.moment.emptyState.cta', {
              defaultValue: 'Switch to Narrative',
            })}
          </Button>
        }
      />
    </div>
  );
}

/**
 *
 * Renders when the active layer is Brief but the projection has zero nodes
 * (no bound World; or the bound World's KB graph has no `block_type=era`
 * entities). Work-Brief is a read-only **projection** of the bound World's
 * Brief (PD-2): Brief is World spine, the Work does NOT gain an authored
 * Brief, and there is NO Work-owned Brief write flow. The panel says exactly
 * that (honest copy: world-shape context comes from the bound World's Brief)
 * and offers a CTA back to Narrative — there is NO "create Brief" CTA,
 * because this is NOT a Work Brief authoring surface.
 *
 * Mirrors the V1.123 P1 `BriefEmptyState` escape-hatch pattern (shared
 * `EmptyState` primitive; primary-action CTA so keyboard + SR users have a
 * direct escape hatch) with Work-specific copy per spec §3.3.3 empty-state
 * honesty ("World-shape context appears here when this Work is bound to a
 * World with era markers." + CTA toward Narrative).
 */
function BriefEmptyState({
  onSwitchToNarrative,
}: {
  onSwitchToNarrative: () => void;
}) {
  const { t } = useTranslation('canvas');
  return (
    <div
      data-testid="work-timeline-brief-empty-state"
      className="rounded-card border border-gray-alpha-400 bg-background-100"
    >
      <EmptyState
        title={t('workTimeline.brief.emptyState.title', {
          defaultValue: 'No world-shape context yet',
        })}
        description={t('workTimeline.brief.emptyState.message', {
          defaultValue:
            'World-shape context appears here when this Work is bound to a World with era markers. Brief is a read-only projection of the bound World’s Brief.',
        })}
        action={
          <Button
            type="button"
            variant="primary"
            data-testid="work-timeline-brief-empty-cta"
            onClick={onSwitchToNarrative}
          >
            {t('workTimeline.brief.emptyState.cta', {
              defaultValue: 'Switch to Narrative',
            })}
          </Button>
        }
      />
    </div>
  );
}
