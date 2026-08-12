/**
 * Timeline canvas — orchestrator facade (V1.122 P1 T3 + T4 + V1.123 P1 T3).
 *
 * Slim composition root for the Timeline hero surface. Coordinates:
 *   - Graph read via the shared `useWorldKbGraph(worldId)` hook (V1.73
 *     `GET .../kb/graph` — the single World-spine read endpoint).
 *   - Adapter projection via `useCanvasSurface` (V1.114 P0 recipe).
 *   - V1.123 P1 T3 + V1.156 P1 T2 layer state: Brief | Narrative | Moment
 *     tabs in the canvas header (Moment added V1.156). Active layer drives
 *     `createTimelineCanvasAdapter(ctxRef, layer)` so `useCanvasSurface`'s
 *     `[graph, adapter]` memo re-projects on layer swap (semantic discrete
 *     swap per layer-feel-differentiation.md §3.1 — not continuous viewport
 *     zoom). Default layer is `'brief'` when graph has any
 *     `block_type=era` entity; `'narrative'` fallback otherwise (plan
 *     Global Constraints + architect §7/§8; Moment is never the default —
 *     read-only projection per spec §3.3.3).
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
 * param `?layer=brief|narrative|moment` (V1.123 P4 Task 6 + V1.156 P1 T2 —
 * refresh-safe + survives Timeline → World KB → back). All three layer
 * values are valid on the World Timeline per spec §3.3.3 — the V1.123
 * "moment is Work-only" restriction is lifted (V1.156 amendment). A
 * `useMemo` derives the default layer from the graph data (Brief if any
 * `block_type=era` entity, else Narrative); when the URL carries no layer
 * (or an unknown value), the default wins. The user can override via the
 * layer tabs, the breadcrumb segments, or the semantic zoom bridge — every
 * override writes back to the URL so the choice is shareable and survives
 * refresh. Task 7 owns the honest empty-state copy per layer (V1.156 P1 T2
 * adds the World-Moment panel); Task 5 owns the breadcrumb cross-layer
 * affordance.
 */
import { useEffect, useMemo, useRef, useState } from 'react';
import { useCallback, useContext } from 'react';
import { useTranslation } from 'react-i18next';
import { Link, useBeforeUnload, useNavigate, useSearchParams } from 'react-router';
import type { Node } from '@xyflow/react';
import { useQueries } from '@tanstack/react-query';
import { Cpu, Info, Plus } from 'lucide-react';

import { CanvasShell } from '@/components/canvas/canvas-shell';
import { LayerBreadcrumb } from '@/components/canvas/layer-breadcrumb';
import { useCanvasSurface, type CanvasSurfaceQueryResult } from '@/components/canvas/use-canvas-surface';
import { SemanticZoomBridge } from '@/components/canvas/use-semantic-zoom';
import { useWorldKbGraph, usePatchWorldKbEntity } from '@/lib/canvas/use-world-kb-data';
import {
  flattenPages,
  useComputeModules,
  useForkLineage,
  useNarrativeWorlds,
  useWorldTimelineEvents,
  useWorks,
} from '@/api/queries';
import { useNexusClient } from '@/lib/client-context';
import { SettingsModalContext } from '@/components/layout/settings-modal-context';
import { LoadingState, ErrorState, EmptyState } from '@/components/ui/states';
import { Button } from '@42ch/nexus-ui';
import { useToast } from '@/lib/use-toast';
import { shortId } from '@/lib/format';
import type {
  CreateForkResponse,
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
import type { SceneBeatFixturePayload } from '../outline-canvas/graph-projection';
import { NleTimelineBandOverlay } from './nle-timeline-band-overlay';
import { filterTimelineEntityNodes } from './nle-timeline-projection';
import { EraCreateDialog } from './era-create-dialog';
import { ForkCreateDialog } from './fork-create-dialog';
import { ForkLineageBadge } from './fork-lineage-badge';

export interface TimelineCanvasProps {
  worldId: string;
  /**
   * Optional V1.108 Scene/Beat fixture for the Moment layer (V1.156 P1 T2 —
   * architect spec §3.3.3 V1.156 amendment). The `WorkOutline` wire exposes
   * no scenes/beats today (DR-26 tracks the extension); Design Studio / test
   * fixtures inject scene/beat payloads at the projection layer. When
   * undefined or empty, the Moment layer emits honest empty-state (zero
   * nodes) per product semantics PD-3 — World Timeline Moment is a
   * READ/projection layer: scenes come from Works bound to this World, and
   * Moments remain Work-owned (no World Moment authoring flow).
   *
   * The fixture is captured by the adapter factory (alongside the active
   * layer) so the adapter memo deps include it — a fixture identity change
   * re-projects the Moment layer without a layer swap. It is also forwarded
   * to the adapter context as the `sceneBeatFixture` slot (mirrors the Work
   * Timeline orchestrator's slot); the captured value takes precedence at
   * projection time.
   */
  sceneBeatFixture?: SceneBeatFixturePayload;
}

/**
 * V1.147 P2 T3 — hard ceiling for the auto-fetched compute-event projection
 * (review F1). The canvas consumes the events log as a SNAPSHOT source — the
 * complete canon `compute_result` projection, not a browse list — so remaining
 * pages are auto-fetched while `hasNextPage`. This constant is the safety
 * valve: a pathological World log (> 500 events) stops fetching and logs a
 * dev warning instead of issuing unbounded refetches. Pages beyond the cap are
 * honestly not rendered (the projection renders only what it fetched).
 */
const TIMELINE_EVENTS_PROJECTION_CAP = 500;

/**
 * Bugbot 2 — root-branch fallback when `narrative_worlds.root_fork_branch_id`
 * is unset. Mirrors the daemon's root resolution (`resolve_run_branch` /
 * timeline-events root fallback in `compute_runs.rs` + `timeline_events.rs`).
 * Root is otherwise represented as NO `?branch=` param; comparing against
 * this id lets the parent hop normalize a root parent to `undefined` instead
 * of writing the root id into the URL (the dual-representation bug).
 */
const ROOT_BRANCH_ID_FALLBACK = 'fbk_root';

/**
 * V1.156 P1 fix-wave 1 (F3) — stable empty fixture payload for Worlds with
 * no injected scene/beat fixture. Module-level so the adapter memo deps stay
 * referentially stable across re-renders — no new object identity per render
 * (mirrors the Outline Canvas `EMPTY_SCENE_BEAT_FIXTURE` pattern).
 */
const EMPTY_SCENE_BEAT_FIXTURE: SceneBeatFixturePayload = { scenes: [], beats: [] };

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

export function TimelineCanvas({ worldId, sceneBeatFixture }: TimelineCanvasProps) {
  const { t } = useTranslation('canvas');
  const navigate = useNavigate();
  const graph = useWorldKbGraph(worldId);
  const patchEntity = usePatchWorldKbEntity(worldId);
  const { toast } = useToast();

  // ── V1.162 P2 T1 + fix wave W-1 — active branch context (PD-6
  // enabler; LOCKED) ──────────────────────────────────────────────────────
  //
  // `activeBranchId` is the World Timeline's branch context: `undefined` =
  // the World's root/default branch; a `fbk_…` id = the forked branch the
  // canvas is showing. It is the ONLY branch state the canvas holds (no
  // general branch switcher — plan Global Constraints) and threads into
  // `useWorldTimelineEvents` below as the existing `branch_id` query param
  // (no daemon change — the read route already honors it; the query key
  // includes the filter, so a branch switch auto-fetches the new branch's
  // events).
  //
  // W-1 fix (QC tri common blocker) — `?branch=` is the SSOT, mirroring
  // the `?layer=` model below (~:509): the active branch is DERIVED from
  // `searchParams` every render, so browser Back/Forward (history
  // navigation) updates the rendered branch — the URL and the view can
  // never diverge. `handleBranchChange` only writes the URL (`replace:
  // false` keeps a history entry per hop — browser Back returns to the
  // previous branch view). `undefined` = no param = the World's root.
  const [searchParams, setSearchParams] = useSearchParams();
  const branchRaw = searchParams.get('branch');
  const activeBranchId: string | undefined =
    branchRaw && branchRaw.length > 0 ? branchRaw : undefined;
  const handleBranchChange = useCallback(
    (branchId: string | undefined) => {
      const next = new URLSearchParams(searchParams);
      if (branchId === undefined || branchId.length === 0) {
        next.delete('branch');
      } else {
        next.set('branch', branchId);
      }
      setSearchParams(next, { replace: false });
    },
    [searchParams, setSearchParams],
  );

  // Bugbot 2 — the World's ROOT branch id, from the narrative-worlds DTO
  // (`World.root_fork_branch_id`; daemon fallback `fbk_root` when unset).
  // `handleOpenParentBranch` compares the parent against this so a hop to
  // the World's root CLEARS `?branch=` (root = no param) instead of setting
  // the root id — the degraded-badge guard (`forkLineageUnavailable`) must
  // never see a truthy root `activeBranchId`. The shared worlds query key
  // means the list is reused (not re-fetched) when the app already loaded it.
  const worlds = useNarrativeWorlds();
  const rootBranchId = useMemo(
    () =>
      worlds.data?.find((w) => w.world_id === worldId)?.root_fork_branch_id ??
      ROOT_BRANCH_ID_FALLBACK,
    [worlds.data, worldId],
  );

  // ── V1.147 P2 T3 — compute events + module registry ──────────────────────
  //
  // The Narrative projection merges the World's canon compute_result log
  // events (T1 route) with the KB graph. The events query hard-filters
  // `event_type=compute_result&status=canon` and forwards the canvas's
  // `activeBranchId` as `branch_id` (V1.162 P2 T1); when `undefined` the
  // daemon resolves the World's current branch (root fallback). Accept
  // invalidation flows through `queryKeys.timeline.all` (already wired in
  // `useAcceptRun`), so an accepted Run's node appears without a manual
  // refresh. PD-6: after a fork create, `handleBranchChange(new_branch_id)`
  // changes the filter → a fresh query key → the forked branch's Timeline
  // renders immediately (no extra click, no manual refetch).
  const timelineEvents = useWorldTimelineEvents(worldId, {
    branch_id: activeBranchId,
  });
  const computeModules = useComputeModules();

  // ── V1.162 P2 T2 + fix wave W-2 — read-only fork lineage chrome
  // (carrier B) ───────────────────────────────────────────────────────────
  //
  // The chrome derives the ACTIVE branch's fork state from its
  // `fork_created` canon marker (branch-level, spec §6.6.3) — never the
  // world-level WorldState fork fields. The hook re-keys on branch change
  // (same query-key mechanism as `useWorldTimelineEvents`), so after a
  // PD-6 landing OR a parent hop the badge reflects the new branch without
  // a manual refetch.
  //
  // W-2 fix — loading and error are NEVER collapsed into "not a fork":
  // `isPending` (loading) keeps the chrome hidden (a root read returns
  // zero markers → no chrome); an ERROR on a NON-ROOT branch renders a
  // degraded badge ("fork — lineage unavailable") with a retry, so a
  // transient marker-query failure can never remove the only in-canvas
  // return path (PD-6 parent hop). Root reads hide on error too — the
  // root carries no fork chrome to degrade.
  const forkLineage = useForkLineage(worldId, activeBranchId);
  const forkLineageUnavailable = forkLineage.isError && Boolean(activeBranchId);
  const forkLineageData = useMemo(
    () =>
      forkLineageUnavailable
        ? { is_fork: true }
        : forkLineage.data ?? { is_fork: false },
    [forkLineage.data, forkLineageUnavailable],
  );
  // One-hop parent hand-off (plan §2 branch-context lock): reuses T1's
  // `handleBranchChange` so the timeline-events query re-keys to the
  // parent branch and the parent Timeline renders immediately. No merge /
  // edit / multi-branch workspace — parent hop ONLY.
  //
  // Bugbot 2 — a parent that IS the World's root branch hops to `undefined`
  // (clears `?branch=`), never to the root id string: root is canonically
  // NO param, and the degraded-badge guard treats any truthy
  // `activeBranchId` as a non-root fork (a lineage read failure on a root
  // `?branch=<rootId>` URL would otherwise render the degraded fork badge
  // on the root Timeline).
  const handleOpenParentBranch = useCallback(() => {
    const parentId = forkLineageData.parent_branch_id;
    if (!parentId) return;
    handleBranchChange(parentId === rootBranchId ? undefined : parentId);
  }, [forkLineageData.parent_branch_id, handleBranchChange, rootBranchId]);

  const eventsList = useMemo(
    () => flattenPages(timelineEvents.data),
    [timelineEvents.data],
  );

  // Review F1 — bounded auto-fetch of remaining events pages. The projection
  // is a snapshot source (the canvas needs the COMPLETE canon compute_result
  // log, not a browsable list), so pages are fetched automatically while
  // `hasNextPage`, up to the hard `TIMELINE_EVENTS_PROJECTION_CAP` (500).
  // The effect fires at most once per resolved page: its deps change only
  // when a page lands (`fetchNextPage` is stable; `hasNextPage` flips when
  // the last page resolves), so there is no refetch loop.
  useEffect(() => {
    if (!timelineEvents.hasNextPage) return;
    if (eventsList.length >= TIMELINE_EVENTS_PROJECTION_CAP) {
      // Cap reached — stop fetching and warn in dev. The projection renders
      // the pages it has; older events stay hidden until the cap is raised
      // (or the log is pruned). `simplify:` 500 is a deliberate ceiling for
      // V1.147; a virtualized canvas would lift it without UI churn.
      // V1.147 P3 T2: the honest cap note (plan Global Constraints
      // "500-cap honesty") renders from the same condition — see `isCapped`.
      if (import.meta.env.DEV) {
        console.warn(
          `[timeline] compute-event projection capped at ${TIMELINE_EVENTS_PROJECTION_CAP} ` +
            `events (has_more still true for world ${worldId}); older events are not rendered.`,
        );
      }
      return;
    }
    void timelineEvents.fetchNextPage();
  }, [timelineEvents.hasNextPage, timelineEvents.fetchNextPage, eventsList.length, worldId]);

  // 500-cap honesty (V1.147 P3 T2): when the projection hit the ceiling while
  // more pages existed (`hasNextPage` stays true after the effect stops
  // fetching), the canvas says so instead of silently dropping older events.
  // A World with exactly 500 events and nothing more (`hasNextPage` false)
  // shows no note — nothing is hidden.
  const isProjectionCapped =
    eventsList.length >= TIMELINE_EVENTS_PROJECTION_CAP && timelineEvents.hasNextPage;

  // Module display names for compute node provenance (module_id fallback
  // when the registry has not loaded the module — honest for preset-path
  // modules absent locally).
  const computeModuleNames = useMemo(() => {
    const map = new Map<string, string>();
    for (const m of computeModules.data ?? []) map.set(m.module_id, m.name);
    return map;
  }, [computeModules.data]);

  // ── V1.147 P2 T3 — Settings → Modules Run Studio deep links ─────────────
  //
  // The Run Module entry (toolbar + empty-state) and the compute inspector's
  // Open Run hand-off both open the Settings modal with the Modules section.
  // The context is consumed null-safely: surfaces rendered outside
  // `SettingsModalProvider` (isolated canvas tests) degrade to no entry
  // chrome; production always provides the provider (App wraps AppRoutes).
  const settingsModal = useContext(SettingsModalContext);

  // Timeline-scoped Run Studio: World pre-filled via `?world=` (behavior
  // spec §1 P2 — context pre-fill + entry chrome, not a second studio).
  // Bugbot 1 — when the canvas shows a FORKED branch, the entry ALSO
  // pre-fills `?branch=<activeBranchId>` so the run request carries
  // `branch_id` and compute run/accept writes the fork (not the world
  // root). Root (undefined) → no branch param, unchanged.
  const onRunModule = useCallback(() => {
    settingsModal?.openSettings(
      'modules',
      undefined,
      undefined,
      `?world=${encodeURIComponent(worldId)}${
        activeBranchId ? `&branch=${encodeURIComponent(activeBranchId)}` : ''
      }`,
    );
  }, [settingsModal, worldId, activeBranchId]);

  // Open Run from a Compute result node → Settings → Modules run detail
  // (deep link selects the module + opens the run inspector).
  const onOpenRun = useCallback(
    (runId: string, moduleId: string) => {
      settingsModal?.openSettings(
        'modules',
        undefined,
        undefined,
        `?module=${encodeURIComponent(moduleId)}&run=${encodeURIComponent(runId)}`,
      );
    },
    [settingsModal],
  );

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
  // V1.123 P4 Task 6 + V1.156 P1 T2 — layer-state persistence. The
  // user-chosen layer is encoded in the URL search param
  // `?layer=brief|narrative|moment` (layer-feel-differentiation.md §5 +
  // spec §3.3.3 layer-state persistence). The URL is the shareable,
  // refresh-safe source of truth for the active layer: it survives
  // Timeline → World KB → back round-trips and refresh. On mount, the URL
  // is read; on layer swap (via the layer tabs, breadcrumb, or semantic
  // zoom), the URL is updated. All three layer values are valid on the
  // World Timeline — the V1.123 restriction ("`?layer=moment` ignored on
  // World Timeline — Moment is Work-only") is LIFTED by the V1.156
  // amendment; only unknown values (e.g. `?layer=foo`) are ignored so the
  // default-derived layer wins.
  //
  // When the user picks the default layer (e.g., clicks Brief on a World
  // with era data), the URL param is dropped so the surface can resume
  // tracking graph changes (e.g., if the user clears all eras via World
  // KB, the default would flip to Narrative — a sticky `?layer=brief`
  // would prevent that).
  const urlLayerRaw = searchParams.get('layer');
  const urlLayerOverride: TimelineLayer | null = useMemo(() => {
    if (
      urlLayerRaw === 'brief' ||
      urlLayerRaw === 'narrative' ||
      urlLayerRaw === 'moment'
    ) {
      return urlLayerRaw;
    }
    // Unknown / absent → fall back to the era-derived default.
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
  //
  // V1.147 P2 T3 — the adapter also rebuilds when the compute events array or
  // the module-name map change (data-driven re-projection: `useCanvasSurface`
  // memoises on `[graph, adapter]`, so a new adapter identity re-runs the
  // Narrative merge with the fresh events).
  //
  // V1.156 P1 fix-wave 1 (F3) — the adapter ALSO captures the Moment
  // scene/beat fixture (`fixture` — the stable module-level empty constant
  // when the prop is absent, so real Worlds never churn the memo). A fixture
  // identity change recreates the adapter and re-projects the Moment layer
  // without a layer swap (previously the fixture was only read via
  // `ctxRef.current` AFTER this memo, so the projection stayed stale until
  // a layer swap / graph refetch).
  const fixture = sceneBeatFixture ?? EMPTY_SCENE_BEAT_FIXTURE;
  const adapter = useMemo(
    () =>
      createTimelineCanvasAdapter(
        ctxRef,
        activeLayer,
        eventsList,
        computeModuleNames,
        fixture,
      ),
    [activeLayer, eventsList, computeModuleNames, fixture],
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
    if (conflictInfo?.kind !== 'conflict') return;
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
          if (next?.kind === 'conflict') {
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

  // V1.159 P1 T3 + V1.160 P1 T2 — "新建 era" entry point. SHIPPED: the
  // backend create-on-absent path landed in V1.160 P1 T1 (`patch-entity`
  // creates when the minted entity id is absent; F-001 / R-V1159P1-001
  // closed), so the create path is LIVE — see `showCreateEra` below. The
  // create dialog owns the era entity + optional parent-relationship
  // mutations; this state only gates the dialog mount.
  const [eraCreateOpen, setEraCreateOpen] = useState(false);

  // Existing era entities for the dialog's optional parent picker (spec
  // §3.3.3 V1.159 "Create entry"). Derived from the same graph read the
  // Brief time-bands consume.
  const existingEras = useMemo(
    () =>
      (graph.data?.entities ?? [])
        .filter((e) => e.block_type === 'era')
        .map((e) => ({ entity_id: e.key_block_id, canonical_name: e.canonical_name })),
    [graph.data],
  );

  // ── V1.162 P2 T1 — fork creation flow (World-Timeline-scoped) ────────────
  //
  // The fork-point picker entry lives on Compute result nodes: selecting a
  // node opens the compute inspector, whose "Branch this world's timeline
  // from here" affordance fires `handleCreateFork(eventId)` with the node's
  // timeline event id. The parent branch is derived from the picked event's
  // OWN `branch_id` (the event rendered from the current branch's events
  // query — root when `activeBranchId` is undefined, else the fork branch).
  const [forkDialogOpen, setForkDialogOpen] = useState(false);
  const [pendingFork, setPendingFork] = useState<{
    eventId: string;
    branchId: string;
    label?: string;
  } | null>(null);

  const handleCreateFork = useCallback(
    (eventId: string) => {
      const event = eventsList.find((e) => e.id === eventId);
      if (!event) return; // defensive — the node rendered from eventsList
      setPendingFork({
        eventId,
        branchId: event.branch_id,
        label: event.title ?? undefined,
      });
      setForkDialogOpen(true);
    },
    [eventsList],
  );

  // PD-6 post-create landing (MANDATORY): on 200 the canvas switches the
  // active branch context to the new fork's `branch_id`, which re-keys the
  // `useWorldTimelineEvents` query → the forked branch's World Timeline
  // renders immediately (no extra click). The success notice carries the
  // label when provided, else the branch id. A toast-only parent landing is
  // the rejected alternative (dead-ends the authoring loop — plan §2).
  const handleForkCreated = useCallback(
    (response: CreateForkResponse, label?: string) => {
      handleBranchChange(response.branch_id);
      toast({
        variant: 'success',
        title: t('timeline.forkCreateDialog.createdTitle'),
        description: label ?? shortId(response.branch_id),
      });
    },
    [handleBranchChange, t, toast],
  );

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
    // V1.156 P1 T2 — bound-Works Scene/Beat fixture slot. Forwarded to the
    // adapter context so the Moment projection reads it at projection time
    // (`ctxRef.current.sceneBeatFixture`). Production passes nothing
    // (honest empty-state); Design Studio / tests inject fixture payloads.
    sceneBeatFixture,
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
    // V1.147 P2 T3 — Open Run hand-off from the compute node inspector.
    // Wired only when the Settings modal context exists (production always
    // provides it; isolated canvas tests degrade to read-only inspectors).
    onOpenRun: settingsModal ? onOpenRun : undefined,
    // V1.162 P2 T1 — fork-point hand-off from the compute node inspector.
    // Opens the fork-create dialog pre-seeded with the picked event (the
    // fork point); always wired — the button only renders on compute nodes.
    onCreateFork: handleCreateFork,
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

  // V1.147 P2 T3 — emptiness spans BOTH projection families: the KB graph AND
  // the merged compute log events. A World whose only Narrative content is an
  // accepted compute Run must render its Compute result node, not the
  // global empty state.
  //
  // V1.162 fix wave S-1 (qc3) — the events query is included in the
  // loading gate: during a branch-switch refetch the re-keyed
  // `useInfiniteQuery` has no data (`flattenPages → []`), so without this
  // gate a World with zero KB entities would flash "This World's timeline
  // is empty" + the run-module CTA on a branch that HAS events.
  const isEmpty =
    !timelineEvents.isFetching &&
    ((!graph.data || (graph.data.entities ?? []).length === 0) &&
      eventsList.length === 0);

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

  // V1.156 P1 T2 — World-Moment empty detection. Mirrors the Work Timeline
  // orchestrator's `isMomentEmpty` pattern: the active layer is Moment AND
  // the projection returned zero nodes (no bound-Works scene/beat fixture /
  // empty fixture). The graph itself is NOT globally empty here (the global
  // empty branch above owns zero-entity graphs); this branch only fires
  // when there is content on Brief/Narrative but nothing projectable on
  // Moment — the honest panel (PD-3) explains scenes come from bound Works'
  // Outline data, with a CTA back to Narrative.
  const isMomentEmpty =
    !isEmpty && activeLayer === 'moment' && surface.nodes.length === 0;

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
        // V1.159 P1 T3 + T4 — "新建 era" entry in the Brief-layer chrome
        // (spec §3.3.3 "Create entry", sibling to the layer switcher tabs).
        // V1.160 P1 T2 — SHIPPED: the backend create-on-absent path landed
        // in V1.160 P1 T1 (`patch-entity` creates when the minted entity id
        // is absent; F-001 / R-V1159P1-001 closed), so the gate is live:
        //   `showCreateEra={activeLayer === 'brief'}`
        // which shows it whenever Brief is active — INCLUDING the Brief
        // empty state (the "create your first era" path, T4 DoD) — and
        // hides it on Narrative/Moment (Work-Brief stays read-only per
        // spec §3.3.3). era_type editing on existing eras is UNAFFECTED
        // (the edit path pre-reads an existing entity).
        showCreateEra={activeLayer === 'brief'}
        onCreateEra={() => setEraCreateOpen(true)}
        // V1.159 P1 T3 (T2-M2 carry-forward fix) — the alt-view toggle is
        // not available on the Brief layer when the time-band panel is
        // present (the bands are the Brief rendering model; the toggle would
        // only hide them).
        showAltViewToggle={
          !(activeLayer === 'brief' && surface.briefTimeBands !== null)
        }
        onRunModule={onRunModule}
      />

      {/* V1.162 P2 T2 — read-only fork lineage chrome. Renders only when
          the ACTIVE branch carries a fork_created marker (`is_fork` —
          marker-derived, NOT a world-level WorldState field). Shows the
          parent branch + fork-point event read-only and offers the one-hop
          "open parent branch" control. Mounted above the empty-state
          branch so a freshly created (still empty) fork still tells the
          author where they are + how to return (PD-6 landing context). */}
      <ForkLineageBadge
        lineage={forkLineageData}
        unavailable={forkLineageUnavailable}
        onRetry={
          forkLineageUnavailable ? () => void forkLineage.refetch() : undefined
        }
        onOpenParent={
          forkLineageData.parent_branch_id ? handleOpenParentBranch : undefined
        }
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

      {isProjectionCapped ? (
        <div
          // 500-cap honesty (V1.147 P3 T2): same visible-note pattern as the
          // ordering disclaimer — an honest statement that older compute
          // events are not rendered, not a silent truncation.
          role="note"
          data-testid="timeline-compute-projection-cap-note"
          className="flex items-start gap-2 rounded-card border border-gray-alpha-400 bg-background-100 px-3 py-2 text-copy-13 text-gray-700 shadow-elevation-2"
        >
          <Info className="mt-0.5 h-4 w-4 flex-shrink-0 text-gray-700" aria-hidden />
          <span>
            {t('timeline.computeProjectionCapNote', { count: TIMELINE_EVENTS_PROJECTION_CAP })}
          </span>
        </div>
      ) : null}

      {isEmpty ? (
        <EmptyState
          title={t('timeline.empty.title')}
          description={t('timeline.empty.description')}
          action={
            settingsModal ? (
              <Button
                type="button"
                variant="primary"
                data-testid="timeline-run-module-empty-cta"
                onClick={onRunModule}
              >
                <Cpu className="h-4 w-4" aria-hidden />
                {t('timeline.runModuleEntry.button')}
              </Button>
            ) : undefined
          }
        />
      ) : isBriefEmpty ? (
        <BriefEmptyState onSwitchToNarrative={() => handleLayerChange('narrative')} />
      ) : isMomentEmpty ? (
        // V1.156 P1 T2 — World-Moment honest empty-state (PD-3): scenes come
        // from bound Works' Outline data; Moments remain Work-owned. No
        // World-owned Moment authoring CTA.
        <MomentEmptyState onSwitchToNarrative={() => handleLayerChange('narrative')} />
      ) : activeLayer === 'brief' && surface.briefTimeBands ? (
        // V1.159 P1 T2 — Brief layer vertical time-bands (spec §3.3.3 V1.159
        // amendment). The time-band panel SUPERSEDES the V1.123 horizontal
        // era sweep as the Brief-layer rendering model: eras stack as
        // indented, type-colored bands (`<BriefTimeBands />`, adapter-built
        // from `buildEraTree`). Band selection opens the era inspector via
        // the adapter's `onSelectNode` hand-off (read-only rendering — no
        // inline edit; era creation is V1.159 Task 3). Narrative/Moment keep
        // the spatial canvas below; the layer tabs remain the primary
        // affordance. `simplify:` the semantic-zoom bridge is canvas-bound
        // and intentionally absent on the band panel — the layer tabs carry
        // Brief ↔ Narrative switching (per plan Global Constraints §"Semantic
        // zoom feasibility" the tabs are the primary affordance).
        //
        // V1.159 P1 T3 (T2-M2 carry-forward fix): this branch sits BEFORE the
        // alt-view branch so the Brief time-bands take precedence over the
        // spatial↔list toggle — a Brief-layer author sees the band model even
        // if `showAltView` was left on from a Narrative visit (the toggle is
        // also hidden on this branch via the header `showAltViewToggle`
        // gate, so the two mechanisms agree).
        <div
          key="brief-time-bands"
          className="nexus-layer-enter"
          data-testid="timeline-canvas-layer-transition"
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

      {/* V1.159 P1 T3 + V1.160 P1 T2 — "新建 era" create dialog. SHIPPED:
          the entry gate (`showCreateEra={activeLayer === 'brief'}`) is live
          since the V1.160 P1 T1 backend create-on-absent path (F-001 /
          R-V1159P1-001 closed). The mutation hooks (`usePatchWorldKbEntity`
          / `usePatchWorldKbRelationship`) already invalidate the World KB
          graph query on success, so NO manual `graph.refetch()` is needed
          here (QC3-S-002). */}
      <EraCreateDialog
        open={eraCreateOpen}
        onOpenChange={setEraCreateOpen}
        worldId={worldId}
        existingEras={existingEras}
      />

      {/* V1.162 P2 T1 — fork-create dialog (World Timeline only). Opened
          from a compute node's "Branch this world's timeline from here";
          the fork point is the picked event. On success `handleForkCreated`
          lands on the forked branch (PD-6). */}
      <ForkCreateDialog
        open={forkDialogOpen}
        onOpenChange={(open) => {
          setForkDialogOpen(open);
          // S-4 fix (qc3): clear the stale pending fork point when the
          // dialog closes without success. Reopening always re-derives it
          // via `handleCreateFork`, so nothing lingers between flows.
          if (!open) setPendingFork(null);
        }}
        worldId={worldId}
        parentBranchId={pendingFork?.branchId}
        forkedFromEventId={pendingFork?.eventId ?? ''}
        forkPointLabel={pendingFork?.label}
        onSuccess={handleForkCreated}
      />
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
 * V1.123 P1 T3 + V1.156 P1 T2: the header also surfaces the
 * Brief | Narrative | Moment layer switcher (layer-feel-differentiation.md
 * §3.2 — explicit layer control; Moment added V1.156 §3.3.3). The switcher
 * renders inside the header only when the canvas branch is active
 * (non-empty graph); the empty-state branch owns its own surface.
 */
function TimelineCanvasHeader({
  worldId,
  showAltView,
  onToggleView,
  activeLayer,
  onLayerChange,
  showLayerSwitcher,
  showCreateEra,
  onCreateEra,
  showAltViewToggle,
  onRunModule,
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
  /**
   * V1.159 P1 T3 + V1.160 P1 T2 — "新建 era" entry visibility
   * (Brief-layer chrome). SHIPPED: the V1.160 P1 T1 backend create-on-absent
   * path closed F-001 / R-V1159P1-001, so the orchestrator passes
   * `activeLayer === 'brief'`. The entry shows whenever the Brief layer is
   * active — INCLUDING the Brief empty state (create-first-era path) — and
   * hides on Narrative/Moment (Work-Brief stays read-only).
   */
  showCreateEra: boolean;
  /** V1.159 P1 T3 + V1.160 P1 T2 — opens the era create dialog (live). */
  onCreateEra: () => void;
  /**
   * V1.159 P1 T3 (T2-M2 carry-forward fix) — gates the spatial↔list toggle.
   * Hidden on the Brief layer when the time-band panel is the rendering
   * model (bands take precedence over the alt view).
   */
  showAltViewToggle: boolean;
  /**
   * V1.147 P2 T3 — Run Module entry (behavior spec §1 P2 hero shortcut).
   * Opens the shared Run Studio (Settings → Modules) with the World
   * pre-filled. The orchestrator omits it when the Settings modal context
   * is absent (isolated canvas tests).
   */
  onRunModule?: () => void;
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
        {/* V1.159 P1 T3 + V1.160 P1 T2 — "新建 era" entry (spec §3.3.3
            "Create entry"), sibling to the layer switcher tabs. Brief-layer
            chrome only: Work-Brief stays a read-only projection (spec
            §3.3.3). SHIPPED: rendered while `showCreateEra` is true — the
            orchestrator passes `activeLayer === 'brief'` since the V1.160
            P1 T1 backend create-on-absent path (F-001 / R-V1159P1-001
            closed). */}
        {showCreateEra ? (
          <Button
            type="button"
            variant="secondary"
            size="small"
            onClick={onCreateEra}
            data-testid="timeline-create-era-entry"
            aria-label={t('timeline.eraCreateDialog.buttonAria')}
          >
            <Plus className="h-3.5 w-3.5" aria-hidden />
            {t('timeline.eraCreateDialog.button')}
          </Button>
        ) : null}
        {showAltViewToggle ? (
          <button
            type="button"
            onClick={onToggleView}
            className="rounded-control border border-gray-alpha-400 bg-background-100 px-3 py-1.5 text-button-12 text-gray-900 shadow-elevation-2 hover:bg-gray-alpha-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-700 focus-visible:ring-offset-2"
            aria-pressed={showAltView}
          >
            {showAltView ? t('timeline.header.showGraph') : t('timeline.header.showList')}
          </button>
        ) : null}
        {/* V1.147 P2 T3 — Run Module entry (hero shortcut per behavior spec
            §1 P2). Opens the shared Run Studio with the World pre-filled;
            the Cpu icon marks the compute affordance family (spec §5
            iconography). */}
        {onRunModule ? (
          <Button
            type="button"
            variant="secondary"
            size="small"
            onClick={onRunModule}
            data-testid="timeline-run-module-entry"
            aria-label={t('timeline.runModuleEntry.aria')}
          >
            <Cpu className="h-3.5 w-3.5" aria-hidden />
            {t('timeline.runModuleEntry.button')}
          </Button>
        ) : null}
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
 * V1.123 P1 T3 + V1.156 P1 T2 — Brief | Narrative | Moment layer switcher
 * (layer-feel-differentiation.md §3.2 explicit layer control; Moment added
 * by the V1.156 spec amendment §3.3.3 — 3×2 matrix completion).
 *
 * Inline segmented control (three buttons with `aria-pressed`). Built inline
 * rather than promoting to `packages/nexus-ui` because:
 *   - The set of layers + the active-layer discriminator are Timeline-
 *     surface-specific (not a generic primitive).
 *   - YAGNI — no other surface consumes a generic SegmentedControl today
 *     (the World KB / Strategy / Outline surfaces each ship their own header
 *     toggle patterns; the Work Timeline ships its own 2-way inline control
 *     for Narrative ↔ Moment). The per-surface inline control is the
 *     durable slice.
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
    {
      // V1.156 P1 T2 — Moment tab. Additive third segment; the existing
      // Brief / Narrative semantics are unchanged. Moment is never the
      // default (read-only projection — spec §3.3.3).
      layer: 'moment',
      testId: 'timeline-layer-tab-moment',
      labelKey: 'timeline.layerSwitcher.moment',
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

/**
 * V1.156 P1 T2 — World-Moment honest empty-state.
 *
 * Renders when the active layer is Moment but the projection has zero nodes
 * (no bound-Works scene/beat fixture; or fixture is empty). Per product
 * semantics PD-3 + spec §3.3.3 empty-state honesty (V1.156 amendment), the
 * World Timeline Moment layer is a READ/projection layer: scenes come from
 * Works bound to this World (their Outline scene/beat data), and Moments
 * remain Work-owned. The panel says exactly that (honest copy + how to
 * produce scene precision) and offers a CTA back to Narrative — there is NO
 * "create Moment" CTA, because this is NOT a World Moment authoring surface
 * (no World-owned Moment write flow).
 *
 * Mirrors the Work Timeline's `MomentEmptyState` copy pattern (V1.123 P2
 * Task 7) + the World Timeline's `BriefEmptyState` escape-hatch pattern
 * (built on the shared `EmptyState` primitive; primary-action CTA so
 * keyboard + SR users have a direct escape hatch).
 */
function MomentEmptyState({
  onSwitchToNarrative,
}: {
  onSwitchToNarrative: () => void;
}) {
  const { t } = useTranslation('canvas');
  return (
    <div data-testid="timeline-moment-empty-state" className="rounded-card border border-gray-alpha-400 bg-background-100">
      <EmptyState
        title={t('timeline.moment.emptyState.title', {
          defaultValue: 'No scene or beat data yet',
        })}
        description={t('timeline.moment.emptyState.message', {
          defaultValue:
            'Scene-precision is available when bound Works have scene/beat data in their Outline. Add scenes and beats to a bound Work, or switch to Narrative for events.',
        })}
        action={
          <Button
            type="button"
            variant="primary"
            data-testid="timeline-moment-empty-cta"
            onClick={onSwitchToNarrative}
          >
            {t('timeline.moment.emptyState.cta', {
              defaultValue: 'Switch to Narrative',
            })}
          </Button>
        }
      />
    </div>
  );
}
