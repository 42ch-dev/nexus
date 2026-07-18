/**
 * Timeline canvas — orchestrator facade (V1.122 P1 T3 + T4).
 *
 * Slim composition root for the Timeline hero surface. Coordinates:
 *   - Graph read via the shared `useWorldKbGraph(worldId)` hook (V1.73
 *     `GET .../kb/graph` — the single World-spine read endpoint).
 *   - Adapter projection via `useCanvasSurface` (V1.114 P0 recipe).
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
 */
import { useEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Link, useBeforeUnload } from 'react-router-dom';
import type { Node } from '@xyflow/react';
import { Info } from 'lucide-react';

import { CanvasShell } from '@/components/canvas/canvas-shell';
import { useCanvasSurface, type CanvasSurfaceQueryResult } from '@/components/canvas/use-canvas-surface';
import { useWorldKbGraph, usePatchWorldKbEntity } from '@/lib/canvas/use-world-kb-data';
import { LoadingState, ErrorState, EmptyState } from '@/components/ui/states';
import type { WorldKbGraphResponse, WorldKbPatchEntityRequest } from '@42ch/nexus-contracts';

import {
  createTimelineCanvasAdapter,
  extractTimelineConflict,
  type TimelineCanvasAdapterContext,
  type TimelineConflictInfo,
  type TimelineEntityPatch,
  type TimelinePatchField,
} from './timeline-canvas-adapter';
import type { TimelineNodeData } from './timeline-canvas-adapter';
import { WorldKbEntityConflictModal, type WorldKbEntityConflictDraft } from '../world-kb/world-kb-conflict-modal';

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
  const graph = useWorldKbGraph(worldId);
  const patchEntity = usePatchWorldKbEntity(worldId);

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

  const adapter = useMemo(() => createTimelineCanvasAdapter(ctxRef), []);

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
    <div className="flex flex-col gap-3" data-testid="timeline-canvas">
      <TimelineCanvasHeader
        worldId={worldId}
        showAltView={showAltView}
        onToggleView={() => setShowAltView((v) => !v)}
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
        <CanvasShell
          nodes={surface.nodes}
          edges={surface.edges}
          nodeTypes={surface.nodeTypes}
          onNodesChange={surface.onNodesChange}
          summaryText={surface.summaryText}
          ariaLabel={t('timeline.canvasAriaLabel')}
          surfaceKey="timeline"
          relayout={surface.relayout}
        >
          {surface.inspector ? (
            <div className="pointer-events-auto absolute right-3 top-3 w-[340px] max-w-[calc(100%-1.5rem)] rounded-card border border-gray-alpha-400 bg-background-100 p-4 shadow-popover">
              {surface.inspector}
            </div>
          ) : null}
        </CanvasShell>
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
 */
function TimelineCanvasHeader({
  worldId,
  showAltView,
  onToggleView,
}: {
  worldId: string;
  showAltView: boolean;
  onToggleView: () => void;
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
