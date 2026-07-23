/**
 * Timeline canvas — orchestrator facade (V1.122 P1 T3 + T4 + V1.123 P1 T3).
 *
 * Slim composition root for the Timeline hero surface. Coordinates:
 *   - Graph read via the shared `useWorldKbGraph(worldId)` hook (V1.73
 *     `GET .../kb/graph` — the single World-spine read endpoint).
 *   - Adapter projection via `useCanvasSurface` (V1.114 P0 recipe).
 *   - V1.123 P1 T3 layer state: Brief ↔ Narrative tabs in the canvas
 *     header. Active layer drives `createTimelineCanvasAdapter(ctxRef, layer)`
 *     so `useCanvasSurface`'s `[graph, adapter]` memo re-projects on layer
 *     swap (semantic discrete swap per layer-feel-differentiation.md §3.1 —
 *     not continuous viewport zoom). Default layer is `'brief'` when graph
 *     has any `block_type=era` entity; `'narrative'` fallback otherwise
 *     (plan Global Constraints + architect §7/§8).
 *   - Write boundary: `usePatchWorldKbEntity(worldId)` is the ONLY write
 *     path. The inspector routes patches through `ctxRef.onPatchEntity`,
 *     which the orchestrator wires to `patchEntity.mutate(...)`. Forbidden
 *     in V1.122 (architect-locked §4.2): `timeline.patch_event`,
 *     `world_kb.patch_relationship`, `kb.promote_candidate`, raw-file writes.
 *   - Conflict UX (architect-locked §5): reuses `WorldKbConflictError` (409)
 *     + `WorldKbValidationError` (422); the orchestrator renders the
 *     world-kb-flavored `WorldKbEntityConflictModal` directly (no
 *     Timeline-specific conflict DTO).
 *   - Dirty-state guard: when there is an in-flight patch or an open
 *     conflict modal, the orchestrator warns on tab close / reload via
 *     `useBeforeUnload`. In-app route blocking is intentionally NOT wired
 *     here — the production app uses `<BrowserRouter>` (not a data router),
 *     and `useBlocker` requires a data router. A full in-app dirty guard is
 *     `simplify:` deferred to a future iteration that migrates the app to
 *     `createBrowserRouter` (DF-V1122-DIRTY-GUARD-INAPP). The `CanvasShell`
 *     does not currently ship a built-in guard either, so the Timeline
 *     surface owns its minimal guard surgically rather than retrofit the
 *     shared shell (additive-only per Global Constraints).
 *
 * Peer-surface navigation: the header surfaces Timeline / World KB / Strategy
 * links so the author can pivot to the peer surfaces from the hero. Work
 * entry stays Outline (V1.118 regression gate).
 *
 * Layer state: the orchestrator owns the active layer via the URL search
 * param `?layer=brief|narrative` (V1.123 P4 Task 6 — refresh-safe +
 * survives Timeline → World KB → back). A `useMemo` derives the default
 * layer from the graph data (Brief if any `block_type=era` entity, else
 * Narrative); when the URL carries no layer (or an invalid value like
 * `moment`, which is Work-Timeline-only), the default wins. The user can
 * override via the layer tabs, the breadcrumb segments, or the semantic
 * zoom bridge — every override writes back to the URL so the choice is
 * shareable and survives refresh. Task 7 owns the honest empty-state copy
 * per layer; Task 5 owns the breadcrumb cross-layer affordance.
 */
import { useEffect, useMemo, useRef, useState } from 'react';
import { useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import { Link, useBeforeUnload, useNavigate, useSearchParams } from 'react-router-dom';
import type { Node } from '@xyflow/react';
import { useQueries } from '@tanstack/react-query';
import { Info } from 'lucide-react';

import { CanvasShell } from '@/components/canvas/canvas-shell';
import { LayerBreadcrumb } from '@/components/canvas/layer-breadcrumb';
import { useCanvasSurface, type CanvasSurfaceQueryResult } from '@/components/canvas/use-canvas-surface';
import { SemanticZoomBridge } from '@/components/canvas/use-semantic-zoom';
import { useWorldKbGraph, usePatchWorldKbEntity } from '@/lib/canvas/use-world-kb-data';
import { flattenPages, useWorks } from '@/api/queries';
import { useNexusClient } from '@/lib/client-context';
import { LoadingState, ErrorState, EmptyState } from '@/components/ui/states';
import { Button } from '@42ch/nexus-ui';
import type {
  WorkDetailResponse,
  WorkSummary,
  WorldKbGraphResponse,
  WorldKbPatchEntityRequest,
} from '@42ch/nexus-contracts';

import {
  createTimelineCanvasAdapter,
  extractTimelineConflict,
  type TimelineCanvasAdapterContext,
  type TimelineConflictInfo,
  type TimelineEntityPatch,
  type TimelineLayer,
  type TimelinePatchField,
} from './timeline-canvas-adapter';
import type { TimelineNodeData } from './timeline-canvas-adapter';
import { WorldKbEntityConflictModal, type WorldKbEntityConflictDraft } from '../world-kb/world-kb-conflict-modal';
import { NleTimelineBandOverlay } from './nle-timeline-band-overlay';
import { filterTimelineEntityNodes } from './nle-timeline-projection';

export interface TimelineCanvasProps {
  worldId: string;
}

/**
 * Build the V1.73 `WorldKbPatchEntityRequest` envelope from the adapter's
 * structured patch + the selected node's per-row OCC version. The daemon is
 * the authority on validation; the orchestrator only forwards the patch.
 *
 * Exported for unit testing — the orchestrator's write wiring is a thin
 * callback over `usePatchWorldKbEntity`, and the request shape is the
 * contract surface that must stay V1.73-aligned (`entity_id` +
 * `expected_version` from the node + `patch` carrying only the dirty
 * `WorldKbEntityPatch` fields).
 */
export function buildPatchEntityRequest(
  node: Node<TimelineNodeData>,
  patch: TimelineEntityPatch,
): WorldKbPatchEntityRequest {
  return {
    entity_id: node.data.key_block_id,
    expected_version: node.data.version,
    patch,
  };
}

/**
 * Build the world-kb-flavored conflict modal draft from a captured Timeline
 * conflict. Reuses the V1.73/V1.74 copy tokens verbatim (no Timeline-specific
 * copy); the modal itself is the existing `WorldKbEntityConflictModal`.
 *
 * `simplify:` the modal's "What changed" panel needs canonical field values to
 * be truly useful; the orchestrator does not keep a canonical entity snapshot
 * (the daemon's `details.conflicting_path` is a free-form string, not a
 * field-level diff). The panel renders the OCC version + the daemon's
 * conflicting_path verbatim. A field-level canonical diff is deferred to
 * post-MVP (DF-V1122-DEEPER-WB) — it requires the daemon to return a
 * structured field diff, which is out of V1.122 scope.
 */
function buildConflictDraft(
  info: Extract<TimelineConflictInfo, { kind: 'conflict' }>,
  node: Node<TimelineNodeData> | null,
): WorldKbEntityConflictDraft {
  const entityName = node?.data.canonical_name ?? info.entityId;
  const fields = info.dirtyFields;
  const draftValues: Partial<Record<TimelinePatchField, string>> = {};
  if (info.draftPatch.title !== undefined) draftValues.title = info.draftPatch.title;
  if (info.draftPatch.body !== undefined) {
    draftValues.body =
      typeof info.draftPatch.body === 'string'
        ? info.draftPatch.body
        : JSON.stringify(info.draftPatch.body);
  }
  return {
    entityName,
    fields,
    changedFields: [],
    draftValues,
  };
}

export function TimelineCanvas({ worldId }: TimelineCanvasProps) {
  const { t } = useTranslation('canvas');
  const navigate = useNavigate();
  const graph = useWorldKbGraph(worldId);
  const patchEntity = usePatchWorldKbEntity(worldId);

  // ── V1.123 P3 Task 4 — bound Work (cross-surface navigation) ───────────
  //
  // The World's realizing Work is NOT directly indexed by the wire (the V1.72
  // WorkDetailResponse carries `world_id`, but the inverse lookup — World →
  // Work — requires per-Work detail fan-out). We compose it client-side from
  // the existing `useWorks()` list + a capped per-Work detail fan-out. When
  // exactly one Work binds to the active World, the World Timeline event
  // inspector surfaces "View in Work Timeline" → navigates to
  // `/works/:workId/timeline?layer=narrative`.
  //
  // Honest scope cut: when no Work binds to the World, the orchestrator
  // supplies neither `boundWorkId` nor the callback; the inspector hides the
  // CTA (per plan §"If binding is missing or unreliable, P3 hides the
  // affordance"). When MULTIPLE Works bind (rare in V1.123 — most Worlds are
  // single-Work), the orchestrator picks the most-recent by `updated_at` and
  // notes the simplify: ceiling — a Work-picker is P4 polish.
  //
  // `simplify:` the fan-out is capped at N=20 most-recent Works (per the
  // global Timeline's N=5 pattern). Workspaces with >20 Works see the CTA
  // only when the realizing Work is among the 20 most-recent. A daemon-side
  // composite endpoint (`GET .../worlds/{world_id}/works`) would lift the
  // cap without UI churn — tracked as residual R-V1123P3-005.
  const worksQuery = useWorks({ limit: 20 });
  const worksList = useMemo<WorkSummary[]>(
    () => flattenPages<WorkSummary>(worksQuery.data),
    [worksQuery.data],
  );
  const nexusClient = useNexusClient();

  // Per-Work detail fan-out (parallel). Each `getWork` returns a
  // `WorkDetailResponse` carrying `world_id`; we filter to those matching the
  // active World. Cached at the TanStack Query level — Work-detail readers
  // (sidebar, Work detail page, Work Timeline orchestrator) share keys.
  const workDetailQueries = useQueries({
    queries: worksList.map((w: WorkSummary) => ({
      queryKey: ['work-detail', w.work_id],
      queryFn: (): Promise<WorkDetailResponse> => nexusClient.getWork(w.work_id),
      staleTime: 30_000,
    })),
  });

  // Find the Work that realizes this World (most-recent by `updated_at`).
  // `simplify:` first-match by recency; multi-Work Worlds are rare in
  // V1.123 and a Work-picker is P4 polish.
  const boundWorkId = useMemo<string | undefined>(() => {
    const candidates: Array<{ workId: string; updatedAt: string }> = [];
    workDetailQueries.forEach((q, idx) => {
      const detail = q.data as WorkDetailResponse | undefined;
      if (!detail) return;
      if (detail.world_id !== worldId) return;
      const summary = worksList[idx];
      candidates.push({
        workId: detail.work_id,
        updatedAt: summary?.updated_at ?? '',
      });
    });
    if (candidates.length === 0) return undefined;
    candidates.sort((a, b) => (a.updatedAt < b.updatedAt ? 1 : a.updatedAt > b.updatedAt ? -1 : 0));
    return candidates[0].workId;
  }, [workDetailQueries, worksList, worldId]);

  // Cross-surface navigation hand-off. Composed once per render via
  // `useCallback`; the adapter context ref captures the latest closure so
  // the inspector's CTA always targets the current `boundWorkId`. The
  // callback is only referenced by the adapter context when a realizing
  // Work exists, so a stale-closure risk would not arise in practice.
  const onViewInWorkTimeline = useCallback(() => {
    if (!boundWorkId) return;
    navigate(
      `/works/${encodeURIComponent(boundWorkId)}/timeline?layer=narrative`,
    );
  }, [boundWorkId, navigate]);

  // Captured conflict info (T4) — set by the mutation `onError` when the
  // daemon returns 409 / 422. The node ref lets the modal re-submit on
  // "Reapply" against a fresh `expected_version`.
  const [conflictInfo, setConflictInfo] = useState<TimelineConflictInfo | null>(null);
  const [conflictNode, setConflictNode] = useState<Node<TimelineNodeData> | null>(null);
  const [conflictVersion, setConflictVersion] = useState<number>(0);
  const [validationBanner, setValidationBanner] = useState<string[] | null>(null);

  // Dirty-state guard (T4) — keys on a non-empty draft patch list. The
  // orchestrator owns the list because the inspector is a controlled form
  // per render: a draft is "pending" between an editor change and the
  // mutation's settle. For the MVP guard we treat an active in-flight patch
  // OR an open conflict modal as "unsaved" — both states would lose user
  // intent on a tab close / reload.
  const hasUnsavedEdits =
    patchEntity.isPending || conflictInfo !== null;

  // `useBeforeUnload` works with any Router (the production app uses
  // `<BrowserRouter>`, not a data router). The browser-native prompt covers
  // tab close, reload, and external navigation. In-app route blocking is
  // `simplify:` deferred — see the module doc + DF-V1122-DIRTY-GUARD-INAPP.
  useBeforeUnload(
    (event) => {
      if (hasUnsavedEdits) {
        event.preventDefault();
        event.returnValue = '';
      }
    },
    { capture: true },
  );

  const surfaceQuery = useMemo<CanvasSurfaceQueryResult<WorldKbGraphResponse>>(() => {
    const data = graph.data;
    return {
      data,
      isLoading: graph.isLoading,
      isError: graph.isError,
      error: graph.error,
      refetch: () => {
        void graph.refetch();
      },
    };
  }, [graph.data, graph.isLoading, graph.isError, graph.error, graph.refetch]);

  const ctxRef = useRef<TimelineCanvasAdapterContext>({
    worldId,
  });

  // ── V1.123 P1 T3 — layer state + default-layer logic ────────────────────
  //
  // The orchestrator owns the active layer. The default layer is derived
  // from the graph data (Brief if any `block_type=era` entity, else
  // Narrative) per plan Global Constraints + architect §7/§8.
  //
  // V1.123 P4 Task 6 — layer-state persistence. The user-chosen layer is
  // encoded in the URL search param `?layer=brief|narrative|moment`
  // (layer-feel-differentiation.md §5). The URL is the shareable,
  // refresh-safe source of truth for the active layer: it survives
  // Timeline → World KB → back round-trips and refresh. On mount, the URL
  // is read; on layer swap (via the layer tabs, breadcrumb, or semantic
  // zoom), the URL is updated. Invalid layer values for the surface
  // (e.g., `?layer=moment` on the World Timeline — Moment is Work-only)
  // are ignored so the default-derived layer wins.
  //
  // When the user picks the default layer (e.g., clicks Brief on a World
  // with era data), the URL param is dropped so the surface can resume
  // tracking graph changes (e.g., if the user clears all eras via World
  // KB, the default would flip to Narrative — a sticky `?layer=brief`
  // would prevent that).
  const [searchParams, setSearchParams] = useSearchParams();
  const urlLayerRaw = searchParams.get('layer');
  const urlLayerOverride: TimelineLayer | null = useMemo(() => {
    if (urlLayerRaw === 'brief' || urlLayerRaw === 'narrative') {
      return urlLayerRaw;
    }
    // Invalid / absent → fall back to the era-derived default. `moment`
    // is Work-Timeline-only and is silently ignored here.
    return null;
  }, [urlLayerRaw]);

  const defaultLayer = useMemo<TimelineLayer>(() => {
    const entities = graph.data?.entities ?? [];
    const hasEra = entities.some((e) => e.block_type === 'era');
    return hasEra ? 'brief' : 'narrative';
  }, [graph.data]);

  const activeLayer: TimelineLayer = urlLayerOverride ?? defaultLayer;

  // Layer swap callback — single source of truth for layer changes from
  // the layer tabs, the breadcrumb, and the semantic zoom bridge. Writes
  // the choice back to the URL so it survives refresh + surface switches.
  const handleLayerChange = useCallback(
    (layer: TimelineLayer) => {
      if (layer === defaultLayer) {
        // Dropping the param (rather than writing `?layer=<default>`)
        // keeps the URL minimal and lets the default track graph changes.
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
    [defaultLayer, searchParams, setSearchParams],
  );

  // Rebuild the adapter on layer swap so `useCanvasSurface`'s `[graph,
  // adapter]` memo re-projects (semantic discrete swap per layer-feel §3.1).
  // The ctxRef stays mutable; the adapter just captures the new layer
  // value, so this is a cheap factory re-run, not a full layout rebuild.
  const adapter = useMemo(
    () => createTimelineCanvasAdapter(ctxRef, activeLayer),
    [activeLayer],
  );

  const surface = useCanvasSurface(adapter, surfaceQuery);

  // ── Write boundary wiring (T4) ────────────────────────────────────────────

  /**
   * The ONLY write path the Timeline surface exposes. Routes a structured
   * patch through `usePatchWorldKbEntity` (V1.73 `kb.patch_entity`). The
   * adapter's inspector calls this; the orchestrator owns the mutation,
   * invalidation, and conflict hand-off.
   *
   * Returns a promise that settles with the underlying React Query mutation
   * (`mutateAsync`) so the inspector can reset its `isSubmitting` flag in a
   * `finally` block on every outcome — success AND error (PR #156 fix).
   * The per-call `onError` still fires for conflict / validation hand-off
   * before the promise rejects; the inspector swallows the rejection
   * (conflict / toast UX is already surfaced here).
   *
   * Forbidden methods that MUST NOT be wired here (negative-asserted in
   * `timeline-write-boundary.test.tsx`):
   *   - `client.patchTimelineEvent` (Work-scoped outline markdown).
   *   - `client.worldKbPatchRelationship` (relationships read-only on
   *     Timeline in V1.122).
   *   - `client.worldKbPromoteCandidate` (World KB surface).
   *   - Raw file writes (no `fetch PUT` to a file route, no Tauri `invoke`
   *     to disk).
   */
  async function handlePatchEntity(
    node: Node<TimelineNodeData>,
    patch: TimelineEntityPatch,
    dirtyFields: TimelinePatchField[],
  ): Promise<void> {
    setValidationBanner(null);
    await patchEntity.mutateAsync(buildPatchEntityRequest(node, patch), {
      onError: (error) => {
        const info = extractTimelineConflict(error, {
          draftPatch: patch,
          dirtyFields,
        });
        if (info === null) {
          // Non-conflict / non-validation errors are surfaced as a toast by
          // the hook's global onError. Nothing else to do here.
          return;
        }
        setConflictInfo(info);
        setConflictNode(node);
        if (info.kind === 'conflict') {
          setConflictVersion(info.currentVersion);
          // The hook does NOT auto-refetch on entity-patch 409 (only the
          // relationship hook does). Refetch the canonical graph so the
          // "Use current" / "Review side-by-side" actions operate against
          // fresh state — mirrors the V1.73/V1.74 entity conflict flow.
          void graph.refetch();
        } else if (info.kind === 'validation') {
          setValidationBanner(info.errors);
        }
      },
    });
  }

  /**
   * Re-submit the captured draft against the canonical version the daemon
   * just reported. Wired to the conflict modal's "Reapply" action.
   */
  function handleReapply() {
    if (conflictInfo === null || conflictInfo.kind !== 'conflict') return;
    if (conflictNode === null) return;
    const draft = conflictInfo.draftPatch;
    if (Object.keys(draft).length === 0) {
      setConflictInfo(null);
      return;
    }
    patchEntity.mutate(
      {
        entity_id: conflictNode.data.key_block_id,
        expected_version: conflictVersion,
        patch: draft,
      },
      {
        onSuccess: () => {
          setConflictInfo(null);
          setConflictNode(null);
        },
        onError: (error) => {
          const next = extractTimelineConflict(error, {
            draftPatch: draft,
            dirtyFields: conflictInfo.dirtyFields,
          });
          if (next && next.kind === 'conflict') {
            setConflictInfo(next);
            setConflictVersion(next.currentVersion);
            void graph.refetch();
          } else {
            setConflictInfo(null);
          }
        },
      },
    );
  }

  // T5 — alt-view toggle. Mirrors the V1.114 World KB `showList` pattern: a
  // header button flips between the spatial when-axis canvas and the
  // non-spatial sortable table companion. The toggle is hidden on the
  // empty-state branch (there are no rows to list).
  const [showAltView, setShowAltView] = useState(false);

  // Keep the adapter context current. The adapter object stays referentially
  // stable; only the values inside ctxRef.current change.
  //
  // T5: the context also carries the projected `nodes` + `selectedNodeId` +
  // `onSelectNode` so the alt-view table reads the same rows the canvas
  // renders and can drive React Flow selection from row clicks. Selection
  // opens the inspector that owns the `kb.patch_entity` write path — the
  // alt-view itself performs NO writes (architect-locked §4.2).
  ctxRef.current = {
    worldId,
    onPatchEntity: handlePatchEntity,
    onConflict: (info) => setConflictInfo(info),
    nodes: surface.nodes,
    selectedNodeId: surface.selectedNodeId,
    onSelectNode: (nodeId) => {
      // T5 — alt-view row → React Flow selection. Dispatch a `select` change
      // for every node (matching id → selected, others → deselected) so the
      // `useCanvasSurface` derived `selectedNode` updates and the inspector
      // opens. This is exactly how a canvas node click flows through RF.
      // `simplify:` if selection semantics grow (range / multi), lift into
      // `useCanvasSurface` (DF-V1122-ALT-VIEW-SELECT).
      const changes = surface.nodes.map((n) => ({
        type: 'select' as const,
        id: n.id,
        selected: n.id === nodeId,
      }));
      surface.onNodesChange(changes);
    },
    // V1.123 P3 Task 4 — cross-surface navigation slots. Forwarded only when
    // a realizing Work is bound (the inspector hides the CTA otherwise —
    // honest scope cut).
    boundWorkId,
    onViewInWorkTimeline: boundWorkId ? onViewInWorkTimeline : undefined,
  };

  // When the user navigates to a different node, clear a stale validation
  // banner — the new selection starts clean.
  useEffect(() => {
    setValidationBanner(null);
  }, [surface.selectedNodeId]);

  // ── Render ────────────────────────────────────────────────────────────────

  if (graph.isLoading) {
    return <LoadingState label={t('timeline.loading')} />;
  }
  if (graph.isError) {
    return (
      <ErrorState
        description={t('timeline.loadError')}
        onRetry={() => graph.refetch()}
      />
    );
  }

  const isEmpty =
    !graph.data || (graph.data.entities ?? []).length === 0;

  // V1.123 P1 T5 — Brief-empty detection. The active layer is Brief but the
  // graph carries zero `block_type=era` entities (the user clicked the Brief
  // tab on a World that has no era data; Batch A T3's default-layer memo
  // defaults such Worlds to Narrative, so this branch only triggers via an
  // explicit user override). Per `layer-feel-differentiation.md` §2.2 + §7,
  // the surface renders an honest Brief-empty panel with a CTA back to
  // Narrative instead of an empty spatial canvas.
  //
  // The graph itself is NOT globally empty here (the global empty branch
  // below owns zero-entity graphs). The Brief-empty branch only fires when
  // there are non-era entities to show on the Narrative layer.
  const eraCount = (graph.data?.entities ?? []).filter(
    (e) => e.block_type === 'era',
  ).length;
  const isBriefEmpty = !isEmpty && activeLayer === 'brief' && eraCount === 0;

  // Visible ordering-disclaimer gate (PR #156 fix 3 — Greptile P1). Mirrors
  // the adapter's `summarizeTimelineGraph` a11y-disclaimer condition: present
  // whenever any `block_type=event` entity is rendered, omitted for zero-event
  // graphs. A graph with only Context entities (no events) does NOT surface
  // the disclaimer — there is no when-axis ordering to disclaim. The a11y
  // live region in `CanvasShell` carries the same disclaimer for SR users;
  // this visible notice is the sighted-user counterpart.
  const hasEvents =
    (graph.data?.entities ?? []).some((e) => e.block_type === 'event');

  return (
    <div
      className="flex flex-col gap-3"
      data-testid="timeline-canvas"
      data-active-layer={activeLayer}
    >
      <TimelineCanvasHeader
        worldId={worldId}
        showAltView={showAltView}
        onToggleView={() => setShowAltView((v) => !v)}
        activeLayer={activeLayer}
        onLayerChange={handleLayerChange}
        showLayerSwitcher={!isEmpty}
      />

      {validationBanner && validationBanner.length > 0 ? (
        <ul
          className="rounded-card border border-red-700/30 bg-red-700/10 p-3 text-copy-13 text-red-1000"
          aria-live="polite"
          data-testid="timeline-validation-banner"
        >
          {validationBanner.map((err, i) => (
            <li key={i}>{err}</li>
          ))}
        </ul>
      ) : null}

      {hasEvents ? (
        <div
          // `role="note"` (no aria-live): the screen-reader live region in
          // <CanvasShell> already carries the ordering disclaimer for SR
          // users. This visible notice targets sighted users; SR users can
          // still discover it via DOM navigation without a duplicate live
          // announcement.
          role="note"
          data-testid="timeline-ordering-disclaimer"
          className="flex items-start gap-2 rounded-card border border-gray-alpha-400 bg-background-100 px-3 py-2 text-copy-13 text-gray-700 shadow-elevation-2"
        >
          <Info className="mt-0.5 h-4 w-4 flex-shrink-0 text-gray-700" aria-hidden />
          <span>{t('timeline.orderingDisclaimer')}</span>
        </div>
      ) : null}

      {isEmpty ? (
        <EmptyState
          title={t('timeline.empty.title')}
          description={t('timeline.empty.description')}
        />
      ) : isBriefEmpty ? (
        <BriefEmptyState onSwitchToNarrative={() => handleLayerChange('narrative')} />
      ) : showAltView ? (
        <div className="grid gap-3 lg:grid-cols-[1fr_360px]">
          {surface.altView}
          {surface.inspector ? (
            <div className="rounded-card border border-gray-alpha-400 bg-background-100 p-4 shadow-popover">
              {surface.inspector}
            </div>
          ) : null}
        </div>
      ) : (
        // V1.123 P4 Task 4 — layer transition animation. The `key` forces a
        // remount on layer swap so the CSS keyframe animation replays; the
        // `nexus-layer-enter` class carries the keyframe (fade + subtle scale
        // per layer-feel-differentiation.md §4 "changing instrument"). The
        // global `prefers-reduced-motion` rule in `apps/web/src/index.css`
        // collapses animation-duration to 0.01ms so reduced-motion users get
        // an instant swap. Viewport continuity survives via `useCanvasViewport`'s
        // module-level cache (surfaceKey="timeline" is constant across layers).
        <div key={activeLayer} className="nexus-layer-enter" data-testid="timeline-canvas-layer-transition">
          <CanvasShell
            nodes={filterTimelineEntityNodes(surface.nodes)}
            edges={surface.edges}
            nodeTypes={surface.nodeTypes}
            onNodesChange={surface.onNodesChange}
            summaryText={surface.summaryText}
            ariaLabel={t('timeline.canvasAriaLabel')}
            surfaceKey="timeline"
            surfaceKind="timeline"
            relayout={surface.relayout}
            fitViewOptions={{
              nodes: filterTimelineEntityNodes(surface.nodes),
            }}
          >
            <NleTimelineBandOverlay
              nodes={surface.nodes}
              surface="world"
              activeLayer={activeLayer}
              scrollAriaLabel={t('timeline.nleBandScrollAriaLabel')}
            />
            {/* V1.123 P4 Task 3 — semantic zoom bridge. Mounts inside
                CanvasShell so it lives within the ReactFlowProvider; observes
                viewport zoom and fires `handleLayerChange` when the user
                crosses the architect-locked 0.55–0.70 hysteresis band
                (layer-feel-differentiation.md §3.2). The bridge renders
                nothing visible — purely a hook host. Coexists with the
                explicit Brief ↔ Narrative layer tabs (the primary affordance
                per plan Global Constraints §"Semantic zoom feasibility"). */}
            <SemanticZoomBridge
              activeLayer={activeLayer}
              onLayerChange={handleLayerChange}
              chain={{ coarseLayer: 'brief', fineLayer: 'narrative' }}
            />
            {surface.inspector ? (
              <div className="pointer-events-auto absolute right-3 top-3 w-[340px] max-w-[calc(100%-1.5rem)] rounded-card border border-gray-alpha-400 bg-background-100 p-4 shadow-popover">
                {surface.inspector}
              </div>
            ) : null}
          </CanvasShell>
        </div>
      )}

      {conflictInfo && conflictInfo.kind === 'conflict' && conflictNode ? (
        <WorldKbEntityConflictModal
          open
          draft={buildConflictDraft(conflictInfo, conflictNode)}
          currentVersion={conflictVersion}
          onUseCurrent={() => {
            setConflictInfo(null);
            setConflictNode(null);
            void graph.refetch();
          }}
          onReapply={handleReapply}
          onDismiss={() => {
            setConflictInfo(null);
            setConflictNode(null);
          }}
        />
      ) : null}
    </div>
  );
}

/**
 * Canvas header — surfaces the Timeline hero label + peer-surface navigation
 * (World KB + Strategy) so the author can pivot from the hero. The peer links
 * preserve the active `worldId` (World KB) or drop to the list picker
 * (Strategy), matching `resolveCanvasNavTarget` semantics.
 *
 * The Work entry is intentionally NOT linked from here — Work entry stays
 * Outline (V1.118 regression gate), and Timeline is the World-entry hero.
 *
 * T5: the header also surfaces the spatial ↔ list toggle (mirrors the V1.73
 * World KB `WorldKbHeader` show-list button). Hidden when the Timeline has
 * zero entities (the empty-state branch owns its own CTA).
 *
 * V1.123 P1 T3: the header also surfaces the Brief ↔ Narrative layer
 * switcher (layer-feel-differentiation.md §3.2 — explicit layer control).
 * The switcher renders inside the header only when the canvas branch is
 * active (non-empty graph); the empty-state branch owns its own surface.
 */
function TimelineCanvasHeader({
  worldId,
  showAltView,
  onToggleView,
  activeLayer,
  onLayerChange,
  showLayerSwitcher,
}: {
  worldId: string;
  showAltView: boolean;
  onToggleView: () => void;
  activeLayer: TimelineLayer;
  onLayerChange: (layer: TimelineLayer) => void;
  /**
   * V1.123 P1 T3 — gates the layer switcher visibility. The empty-state
   * branch owns its own surface; the layer tabs add noise without value
   * when the graph is empty (Task 5 owns the per-layer empty-state copy).
   */
  showLayerSwitcher: boolean;
}) {
  const { t } = useTranslation('canvas');
  return (
    <div className="flex flex-wrap items-center justify-between gap-2">
      <div>
        <h2 className="text-heading-16 font-heading text-gray-1000">
          {t('timeline.header.title')}
        </h2>
        <p className="text-copy-13 text-gray-700">
          {t('timeline.header.description')}
        </p>
      </div>
      <div className="flex flex-wrap items-center gap-2">
        {/* V1.123 P4 Task 5 — layer breadcrumb. Shows the layer path
            (Brief, or Brief > Narrative when drilled). The parent segment
            is a clickable zoom-out affordance; the active segment is
            static text. Renders only on the non-empty branch (the
            empty-state branch owns its own surface). */}
        {showLayerSwitcher ? (
          <LayerBreadcrumb
            surfaceKey="timeline"
            coarseSegment={{
              layer: 'brief',
              label: t('timeline.layerSwitcher.brief', {
                defaultValue: 'Brief',
              }),
            }}
            fineSegment={{
              layer: 'narrative',
              label: t('timeline.layerSwitcher.narrative', {
                defaultValue: 'Narrative',
              }),
            }}
            activeLayer={activeLayer}
            onLayerChange={onLayerChange}
            ariaLabel={t('timeline.breadcrumb.ariaLabel', {
              defaultValue: 'Timeline layer path',
            })}
          />
        ) : null}
        {/* V1.123 P1 T3 — Brief ↔ Narrative layer switcher (layer-feel §3.2).
            Hidden when the empty-state branch owns the surface. */}
        {showLayerSwitcher ? (
          <TimelineLayerSwitcher
            activeLayer={activeLayer}
            onLayerChange={onLayerChange}
          />
        ) : null}
        <button
          type="button"
          onClick={onToggleView}
          className="rounded-control border border-gray-alpha-400 bg-background-100 px-3 py-1.5 text-button-12 text-gray-900 shadow-elevation-2 hover:bg-gray-alpha-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-700 focus-visible:ring-offset-2"
          aria-pressed={showAltView}
        >
          {showAltView ? t('timeline.header.showGraph') : t('timeline.header.showList')}
        </button>
        <nav
          className="flex flex-wrap items-center gap-2"
          aria-label={t('timeline.header.peerNavAria')}
        >
          <Link
            to={`/worlds/${encodeURIComponent(worldId)}/kb`}
            className="rounded-control border border-gray-alpha-400 bg-background-100 px-3 py-1.5 text-button-12 text-gray-900 shadow-elevation-2 hover:bg-gray-alpha-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-700 focus-visible:ring-offset-2"
          >
            {t('timeline.header.worldKbLink')}
          </Link>
          <Link
            to="/strategies"
            className="rounded-control border border-gray-alpha-400 bg-background-100 px-3 py-1.5 text-button-12 text-gray-900 shadow-elevation-2 hover:bg-gray-alpha-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-700 focus-visible:ring-offset-2"
          >
            {t('timeline.header.strategyLink')}
          </Link>
        </nav>
      </div>
    </div>
  );
}

/**
 * V1.123 P1 T3 — Brief ↔ Narrative layer switcher (layer-feel-differentiation.md
 * §3.2 explicit layer control).
 *
 * Inline segmented control (two buttons with `aria-pressed`). Built inline
 * rather than promoting to `packages/nexus-ui` because:
 *   - The set of layers + the active-layer discriminator are Timeline-
 *     surface-specific (not a generic primitive).
 *   - YAGNI — no other surface consumes a generic SegmentedControl today
 *     (the World KB / Strategy / Outline surfaces each ship their own header
 *     toggle patterns). P4 may promote a generic primitive if more layers
 *     arrive (e.g. Work Timeline Narrative ↔ Moment); the per-surface
 *     inline control is the durable slice for V1.123 P1.
 *
 * Accessibility: each button carries `aria-pressed` so screen readers
 * announce the active layer as a toggle state (WCAG 2.1 — semantic
 * pressed state for toggle buttons). The group wraps in a `role="group"`
 * with an i18n label so SR users can navigate to the switcher by name.
 *
 * `simplify:` the inline buttons reuse the existing header button styling
 * (border + bg + shadow + focus ring) for visual consistency. A bespoke
 * segmented-control visual (sliding indicator, etc.) is P4 polish territory
 * (layer-feel §4 motion contract).
 */
function TimelineLayerSwitcher({
  activeLayer,
  onLayerChange,
}: {
  activeLayer: TimelineLayer;
  onLayerChange: (layer: TimelineLayer) => void;
}) {
  const { t } = useTranslation('canvas');
  const layers: Array<{
    layer: TimelineLayer;
    testId: string;
    labelKey: string;
  }> = [
    {
      layer: 'brief',
      testId: 'timeline-layer-tab-brief',
      labelKey: 'timeline.layerSwitcher.brief',
    },
    {
      layer: 'narrative',
      testId: 'timeline-layer-tab-narrative',
      labelKey: 'timeline.layerSwitcher.narrative',
    },
  ];
  return (
    <div
      role="group"
      aria-label={t('timeline.layerSwitcher.ariaLabel')}
      className="flex items-center gap-1 rounded-control border border-gray-alpha-400 bg-background-100 p-0.5"
    >
      {layers.map(({ layer, testId, labelKey }) => {
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
            {t(labelKey)}
          </button>
        );
      })}
    </div>
  );
}

/**
 * V1.123 P1 T5 — Brief-layer honest empty-state.
 *
 * Renders when the active layer is Brief but the graph has zero
 * `block_type=era` entities (the user clicked the Brief tab on a World that
 * has no era data; the default layer for such Worlds is Narrative per Batch
 * A T3's memo). The panel surfaces the layer-feel §7 copy + a CTA back to
 * Narrative — the actionable escape hatch from an empty Brief world.
 *
 * Built on the shared `EmptyState` primitive (DESIGN.md §Voice & Content —
 * empty-state headlines on authoring surfaces) so the visual treatment
 * matches every other authoring empty-state in the app. The CTA uses a
 * primary-action button so keyboard + SR users have a direct escape hatch.
 *
 * Reuses the V1.121 header button styling so the CTA reads as part of the
 * Timeline chrome family (not a generic link). The `action` slot on
 * `EmptyState` keeps the CTA semantically grouped with the empty-state
 * copy.
 */
function BriefEmptyState({
  onSwitchToNarrative,
}: {
  onSwitchToNarrative: () => void;
}) {
  const { t } = useTranslation('canvas');
  return (
    <div data-testid="timeline-brief-empty-state" className="rounded-card border border-gray-alpha-400 bg-background-100">
      <EmptyState
        title={t('timeline.brief.emptyState.title')}
        description={t('timeline.brief.emptyState.message')}
        action={
          <Button
            type="button"
            variant="primary"
            data-testid="timeline-brief-empty-cta"
            onClick={onSwitchToNarrative}
          >
            {t('timeline.brief.emptyState.cta')}
          </Button>
        }
      />
    </div>
  );
}
