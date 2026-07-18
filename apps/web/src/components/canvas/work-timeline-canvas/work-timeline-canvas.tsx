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
import { useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { CanvasShell } from '@/components/canvas/canvas-shell';
import {
  useCanvasSurface,
  type CanvasSurfaceQueryResult,
} from '@/components/canvas/use-canvas-surface';
import { useWorkOutline } from '@/lib/canvas/use-outline-data';
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
 *   - `useState<WorkTimelineLayer>` active layer (default `'narrative'`).
 *   - `WorkTimelineLayerSwitcher` inline component (segmented control).
 *   - `data-active-layer` testability hook on the root container.
 *   - Layer switcher hidden on the empty-state branch (Task 7 owns the
 *     visible empty-state copy).
 */
export function WorkTimelineCanvas({ workId, sceneBeatFixture }: WorkTimelineCanvasProps) {
  const { t } = useTranslation('canvas');
  const outlineQuery = useWorkOutline(workId);

  // ── V1.123 P2 Task 4 — layer state + default-layer logic ────────────────
  //
  // Architect §7.3 UX-risk override: default = 'narrative' UNCONDITIONALLY
  // in V1.123. Unlike the V1.122 World Timeline (which flips Brief↔Narrative
  // based on era data), the Work Timeline default does NOT consult the
  // scene/beat fixture — even when a fixture is wired (Design Studio / test),
  // the surface stays Narrative-default so the UX does not flip between
  // real Works (no wire-scene/beat data today) and fixture-driven tests.
  //
  // When the WorkOutline wire extends to expose scenes/beats (V1.124+
  // `DF-V1123-MOMENT-WIRE`), this default MAY flip to Moment; the architect
  // §7.3 lock will be revisited at that time. For V1.123, Narrative is the
  // unconditional entry layer.
  //
  // We track the layer via a single `useState` (no `useMemo` default
  // derivation needed because the default is constant in V1.123). The
  // initial value is `'narrative'` per the override.
  const [activeLayer, setActiveLayer] = useState<WorkTimelineLayer>('narrative');

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
  ctxRef.current = {
    workId,
    sceneBeatFixture,
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

  return (
    <div
      className="flex flex-col gap-3"
      data-testid="work-timeline-canvas"
      data-active-layer={activeLayer}
    >
      <WorkTimelineCanvasHeader
        activeLayer={activeLayer}
        onLayerChange={setActiveLayer}
        showLayerSwitcher={!isEmpty}
      />

      {isEmpty ? (
        <EmptyState
          title={t('workTimeline.empty.title', {
            defaultValue: 'This work’s timeline is empty',
          })}
          description={t('workTimeline.empty.description', {
            defaultValue:
              'Outline events you add through the Outline surface will appear here.',
          })}
        />
      ) : (
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
          relayout={surface.relayout}
        />
      )}
    </div>
  );
}

/**
 * Canvas header — surfaces the Work Timeline label + the Narrative ↔ Moment
 * layer switcher (Task 4). The peer-link to Outline lives in Task 5
 * (Work Canvas shell peer nav registration). Task 4 ships the header with
 * the layer switcher only.
 */
function WorkTimelineCanvasHeader({
  activeLayer,
  onLayerChange,
  showLayerSwitcher,
}: {
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
        {showLayerSwitcher ? (
          <WorkTimelineLayerSwitcher
            activeLayer={activeLayer}
            onLayerChange={onLayerChange}
          />
        ) : null}
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
