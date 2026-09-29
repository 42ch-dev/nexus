/**
 * Outline+Timeline canvas — interactive structure surface for a Work (V1.72 β;
 * V1.108 P0 spatial React Flow parity).
 *
 * Thin orchestrator + public re-export facade. V1.73 B5 (`R-V172P0-QC1-002`)
 * split the 825-line monolith into focused sibling modules ≤250 lines per the
 * V1.71 `strategy-canvas.tsx` pattern. V1.108 P0 mounts the shared
 * `CanvasShell` with the RF projection so the outline opens as a spatial graph.
 */
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useQueryClient } from '@tanstack/react-query';
import { Clapperboard, Plus, Trash2 } from 'lucide-react';

import { CanvasShell } from '@/components/canvas/canvas-shell';
import { EmptyState, ErrorState, LoadingState } from '@/components/ui/states';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { INPUT_CLASS, MetaField } from './outline-canvas/inspectors/chapter-meta-field';
import { useChapters, useWork, flattenPages } from '@/api/queries';
import { useRegisterCommand } from '@/lib/canvas/command-registry';
import { queryKeys } from '@/lib/nexus/query-keys';
import {
  isOutlineConflictError,
  usePatchOutlineChapter,
  usePatchOutlineStructure,
  usePatchTimelineEvent,
  useWorkOutline,
} from '@/lib/canvas/use-outline-data';
import { useCanvasSurface, type CanvasSurfaceQueryResult } from '@/components/canvas/use-canvas-surface';

import { CanvasHeader } from './outline-canvas/canvas-layout';
import { OutlineConflictDialog } from './outline-canvas/conflict-modal';
import { BeatInspector } from './outline-canvas/inspectors/beat-inspector';
import { ChapterInspector } from './outline-canvas/inspectors/chapter-inspector';
import { TimelinePanel } from './outline-canvas/inspectors/event-inspector';
import { SceneInspector } from './outline-canvas/inspectors/scene-inspector';
import { OutlineStructurePanel } from './outline-canvas/inspectors/structure-inspector';
import type { ConflictState, SceneBeatFixturePayload } from './outline-canvas/graph-projection';
import { chapterDisplayTitle, sceneBeatPayloadFromOutline } from './outline-canvas/graph-projection';
import { outlineGraphSummary } from './outline-canvas/rf-projection';
import {
  selectedBeatIdFromNodes,
  selectedChapterIdFromNodes,
  selectedSceneIdFromNodes,
} from './outline-canvas/rf-projection';
import { OutlineAltView } from './outline-canvas/outline-alt-view';
import {
  createOutlineCanvasAdapter,
  type OutlineCanvasAdapterContext,
  type OutlineSurfaceGraph,
} from './outline-canvas/outline-canvas-adapter';
import type { Node } from '@xyflow/react';
import type {
  ChapterSummary,
  OutlinePatchChapterRequest,
  OutlinePatchStructureRequest,
  TimelinePatchEventRequest,
  WorkOutline,
} from '@42ch/nexus-contracts';

/**
 * Stable empty projection payload — used while the Work has no scene/beat data
 * (or before the outline read resolves). Module-level so the projection memo
 * deps stay referentially stable across re-renders — no new object identity
 * per render.
 */
const EMPTY_SCENE_BEAT_FIXTURE: SceneBeatFixturePayload = { scenes: [], beats: [] };

export interface OutlineCanvasProps {
  workId: string;
  /**
   * Optional chapter id to preselect on mount (V1.75 F-QC3-001). Read once from
   * the route's `?chapter=N` query param by {@link OutlinePage} and used to
   * seed {@link selectedChapterId}; later user clicks override it normally.
   */
  initialSelectedChapterId?: number | null;
  /**
   * Optional Scene/Beat fixture payload (V1.109 C2 T4 — FB-C2-000/004).
   *
   * Explicit injection mode for Design Studio / component tests — a populated
   * or deliberately empty payload. Production callers omit it: the surface then
   * projects the Work's canonical `WorkOutline.scenes[]` / `.beats[]` carrier
   * (V1.200 DR-26), so authored scenes/beats are visible without a fixture.
   */
  sceneBeatFixture?: SceneBeatFixturePayload;
}

export function OutlineCanvas({
  workId,
  initialSelectedChapterId = null,
  sceneBeatFixture,
}: OutlineCanvasProps) {
  const { t } = useTranslation('canvas');
  const work = useWork(workId);
  const chaptersQuery = useChapters(workId);
  const outline = useWorkOutline(workId);

  const patchStructure = usePatchOutlineStructure(workId);
  const patchChapter = usePatchOutlineChapter(workId);
  const patchTimeline = usePatchTimelineEvent(workId);

  const [conflict, setConflict] = useState<ConflictState | null>(null);
  const [showAlt, setShowAlt] = useState(false);
  const qc = useQueryClient();
  // Bumped after a successful refetch so the inspector's content editor resets
  // its local dirty state (e.g. following conflict resolution / reapply).
  const [contentVersion, setContentVersion] = useState(0);

  // V1.111 P0 T4 — register the Outline graph↔list toggle in the palette. The
  // functional `setShowAlt(v => !v)` updater is used so the handler (captured
  // once on mount by `useRegisterCommand`) reads current state rather than the
  // mount-time value. No node-create command: the Outline canvas exposes no
  // chapter-creation entrypoint (the structure panel is select/move only).
  useRegisterCommand({
    id: 'outline.toggle-view',
    labelKey: 'outline.toggle-view.label',
    groupKey: 'group.outline',
    keywordKeys: [
      'outline.toggle-view.keywords.graph',
      'outline.toggle-view.keywords.list',
      'outline.toggle-view.keywords.alt-view',
      'outline.toggle-view.keywords.switch',
    ],
    handler: () => setShowAlt((v) => !v),
  });

  const chapters = useMemo(() => flattenPages(chaptersQuery.data), [chaptersQuery.data]);

  // I-QC1-002 — auto-fetch all chapter pages so the spatial graph projects the
  // complete outline structure. Without this, paginated chapter data (20/page)
  // leaves volume→chapter edges pointing at unloaded chapter nodes. The graph
  // is a structural overview, so completeness matters more than lazy loading.
  useEffect(() => {
    if (chaptersQuery.hasNextPage && !chaptersQuery.isFetchingNextPage) {
      void chaptersQuery.fetchNextPage();
    }
  }, [chaptersQuery.hasNextPage, chaptersQuery.isFetchingNextPage, chaptersQuery.fetchNextPage]);
  const chapterById = useMemo(() => {
    const map = new Map<number, ChapterSummary>();
    chapters.forEach((c) => map.set(c.chapter, c));
    return map;
  }, [chapters]);

  // V1.200 DR-26 (Greptile wave A) — the Work's canonical `WorkOutline.scenes[]`
  // / `.beats[]` arrays are authoritative: with no explicit Studio/test fixture
  // the projection, the Scene/Beat inspectors and the alt view read those
  // arrays (the same WorkOutline read the authoring panel and the Timeline
  // Moment layer consume). Design Studio / tests still inject an explicit
  // payload via the `sceneBeatFixture` prop, which wins.
  const canonicalSceneBeatFixture = useMemo<SceneBeatFixturePayload>(() => {
    if (!outline.data) return EMPTY_SCENE_BEAT_FIXTURE;
    const payload = sceneBeatPayloadFromOutline(outline.data);
    // Keep the module-level identity while the Work genuinely has no
    // scene/beat data so the projection memo deps don't churn on re-render.
    return payload.scenes.length === 0 && payload.beats.length === 0
      ? EMPTY_SCENE_BEAT_FIXTURE
      : payload;
  }, [outline.data]);

  const fixture = sceneBeatFixture ?? canonicalSceneBeatFixture;

  // The alt view tells "no fixture" (undefined → chapters render with no
  // scene/beat chrome) apart from an active fixture (nested rows +
  // empty-under-chapter helper). Forward a canonical payload only once it has
  // content, so a genuinely scene-less Work keeps its honest empty chrome.
  const altViewSceneBeatFixture =
    sceneBeatFixture ??
    (fixture.scenes.length > 0 || fixture.beats.length > 0 ? fixture : undefined);

  // V1.115 P1 T5 (R-V1109-P0-QC3-W002) — index scene titles for O(1) lookup so
  // `beatParentSceneTitle` is a `Map.get`, not an `Array.find` per render.
  // Mirrors the `chapterById` pattern; rebuilt only when `fixture.scenes`
  // changes (stable across renders on real Works where `fixture` is the
  // module-level `EMPTY_SCENE_BEAT_FIXTURE`).
  const sceneTitleById = useMemo(() => {
    const map = new Map<string, string | null>();
    for (const scene of fixture.scenes) {
      map.set(scene.sceneId, scene.title);
    }
    return map;
  }, [fixture.scenes]);

  // V1.115 P0 T1b — the orchestrator now consumes the shared `useCanvasSurface`
  // hook + `OutlineCanvasAdapter` (T1a). The projection memo, rfNodes/rfEdges
  // state, and the position-merge sync effect that previously lived in the
  // surface-specific `useOutlineCanvasGraph` hook are now provided by
  // `useCanvasSurface` (same merge logic, same selection-key derivation). The
  // orchestrator retains ownership of: the conflict modal (surface-specific
  // `ConflictState` shape), the chapter/scene/beat inspector routing, the
  // alt-view toggle, and the structure/timeline panels.
  const translateFallback = useCallback(
    (chapter: number) => t('chapter.fallback', { chapter }),
    [t],
  );

  const surfaceQuery = useMemo<CanvasSurfaceQueryResult<OutlineSurfaceGraph>>(() => {
    const outlineData = outline.data;
    const workData = work.data;
    // `data` is assembled only when all three queries have loaded so the
    // adapter's projectGraph always receives a complete graph payload.
    if (!outlineData || !workData) {
      return {
        data: undefined,
        isLoading: outline.isLoading || chaptersQuery.isLoading || work.isLoading,
        isError: outline.isError || chaptersQuery.isError || work.isError,
        error: outline.error ?? chaptersQuery.error ?? work.error,
        refetch: () => {
          void outline.refetch();
          void chaptersQuery.refetch();
          void work.refetch();
        },
      };
    }
    return {
      data: {
        outline: outlineData,
        chapters,
        sceneBeatFixture: fixture,
      },
      isLoading: outline.isLoading || chaptersQuery.isLoading || work.isLoading,
      isError: outline.isError || chaptersQuery.isError || work.isError,
      error: outline.error ?? chaptersQuery.error ?? work.error,
      refetch: () => {
        void outline.refetch();
        void chaptersQuery.refetch();
        void work.refetch();
      },
    };
  }, [
    outline.data, outline.isLoading, outline.isError, outline.error, outline.refetch,
    chaptersQuery.isLoading, chaptersQuery.isError, chaptersQuery.error, chaptersQuery.refetch,
    work.data, work.isLoading, work.isError, work.error, work.refetch,
    chapters, fixture,
  ]);

  // Mutable context ref — the adapter object is stable (created once); it reads
  // fresh values from this ref at projection/render time so the orchestrator
  // can update state without invalidating useCanvasSurface's memoized graph.
  const ctxRef = useRef<OutlineCanvasAdapterContext>({
    translateFallback,
    t,
    workId,
    outline: outline.data,
    chapters,
    chapterById,
    fixture,
    altViewSceneBeatFixture,
    onPatchChapter: () => {},
    onMove: () => {},
    patchChapterIsPending: false,
    isConflicting: false,
    contentVersion: 0,
  });
  const adapter = useMemo(() => createOutlineCanvasAdapter(ctxRef), []);
  const surface = useCanvasSurface(adapter, surfaceQuery);

  // Selection state — previously owned by `useOutlineCanvasGraph`; now owned by
  // the orchestrator. `useCanvasSurface` exposes `selectedNodeId` (derived from
  // the RF node `selected` flag); the orchestrator resolves it to chapter /
  // scene / beat ids via the existing helpers so the StructurePanel,
  // TimelinePanel, and inline inspector routing stay coordinated.
  const [selectedChapterId, setSelectedChapterId] = useState<number | null>(
    initialSelectedChapterId ?? null,
  );
  const [selectedSceneId, setSelectedSceneId] = useState<string | null>(null);
  const [selectedBeatId, setSelectedBeatId] = useState<string | null>(null);

  // Thin selection resolver — replaces the hook's selection-sync effect
  // (FB-C1-003 + FB-C2-002). Reads `surface.selectedNode` (which changes only
  // when the selected RF node changes) and resolves chapter / scene / beat ids
  // via the same helpers the hook used. Passing a one-node array to the helpers
  // works because `surface.selectedNode` carries `selected: true`.
  //
  // `hasSelectedRef` distinguishes initial-mount-with-no-selection from a real
  // deselection (user clicks the canvas background after selecting a node). On
  // mount we must NOT clear — `initialSelectedChapterId` (deep-link via
  // ?chapter=N) is the seed and no RF node is selected yet. Once a node has
  // been selected, a subsequent null clears all derived selections so the
  // inspector does not stay active for a stale/deselected node.
  const hasSelectedRef = useRef(false);
  useEffect(() => {
    const selected = surface.selectedNode;
    if (!selected) {
      if (hasSelectedRef.current) {
        setSelectedChapterId(null);
        setSelectedSceneId(null);
        setSelectedBeatId(null);
      }
      return;
    }
    hasSelectedRef.current = true;

    const chapterId = selectedChapterIdFromNodes([selected as Node]);
    if (chapterId !== null) setSelectedChapterId(chapterId);
    else setSelectedChapterId(null);

    const sceneId = selectedSceneIdFromNodes([selected as Node]);
    if (sceneId !== null) setSelectedSceneId(sceneId);
    else setSelectedSceneId(null);

    const beatId = selectedBeatIdFromNodes([selected as Node]);
    if (beatId !== null) setSelectedBeatId(beatId);
    else setSelectedBeatId(null);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [surface.selectedNodeId]);

  const selectedChapter = selectedChapterId ? chapterById.get(selectedChapterId) ?? null : null;

  // V1.109 C2 T4 — resolve the selected Scene/Beat from the fixture payload +
  // selection state (FB-C2-002). The selection resolver above drives
  // `selectedSceneId` / `selectedBeatId` from RF graph-click; the orchestrator
  // resolves them against the fixture to get the entity data + parent title for
  // the inspector. On real Works (empty fixture) both are always null — the
  // Chapter inspector remains the default.
  const selectedScene = selectedSceneId
    ? fixture.scenes.find((s) => s.sceneId === selectedSceneId) ?? null
    : null;
  const selectedBeat = selectedBeatId
    ? fixture.beats.find((b) => b.beatId === selectedBeatId) ?? null
    : null;

  // Parent titles for the *Part of* helper (Voice & Content lock).
  const sceneParentChapterTitle = selectedScene
    ? (() => {
        const ch = chapterById.get(selectedScene.chapterId);
        return ch ? chapterDisplayTitle(ch, outline.data?.chapter_titles as Record<string, string> | undefined, t('chapter.fallback')) : null;
      })()
    : null;
  const beatParentSceneTitle = selectedBeat
    ? sceneTitleById.get(selectedBeat.sceneId) ?? null
    : null;

  const summary = outlineGraphSummary(outline.data, chapters.length, t);

  function captureConflictState(
    error: unknown,
    base: Omit<ConflictState, 'currentRevision' | 'conflictingPath'>,
  ) {
    if (!isOutlineConflictError(error)) return;
    // Outline OCC 409 details carry `current_revision` (core_error.rs
    // OutlineConflict arm); `current_version` is the World-KB field name and
    // never appears on this envelope.
    const details = error.details as
      | { current_revision?: number; conflicting_path?: string }
      | undefined;
    setConflict({
      ...base,
      currentRevision: details?.current_revision ?? outline.data?.outline_revision ?? 0,
      conflictingPath: details?.conflicting_path ?? base.pendingRequest.kind,
    });
  }

  // `onSuccess` lets the authoring panel clear a draft title only once its
  // write actually lands — a 409/422 refusal keeps the draft intact for retry.
  function handleStructure(request: OutlinePatchStructureRequest, onSuccess?: () => void) {
    const state: Omit<ConflictState, 'currentRevision' | 'conflictingPath'> = {
      pendingRequest: { kind: 'structure', request },
    };
    patchStructure.mutate(request, {
      onError: (error) => captureConflictState(error, state),
      onSuccess,
    });
  }

  function handleChapter(chapter: number, request: OutlinePatchChapterRequest) {
    const state: Omit<ConflictState, 'currentRevision' | 'conflictingPath'> = {
      pendingRequest: { kind: 'chapter', chapter, request },
    };
    patchChapter.mutate(
      { chapter, request },
      {
        onError: (error) => captureConflictState(error, state),
      },
    );
  }

  // Mirrors `handleStructure`: `onSuccess` lets the Timeline inspector clear
  // its bind drafts only once the write actually lands (V1.200 DR-26 round 2) —
  // a 409/422 refusal keeps the typed World-event draft for retry.
  function handleTimeline(request: TimelinePatchEventRequest, onSuccess?: () => void) {
    const state: Omit<ConflictState, 'currentRevision' | 'conflictingPath'> = {
      pendingRequest: { kind: 'timeline', request },
    };
    patchTimeline.mutate(request, {
      onError: (error) => captureConflictState(error, state),
      onSuccess,
    });
  }

  async function onUseCurrent() {
    setConflict(null);
    await outline.refetch();
    // Also invalidate the per-chapter outline cache (useChapterOutline, read by
    // the content editor). The work-level outline.refetch() above does NOT touch
    // the chapter outline query; without this invalidation the forced content
    // reset below would reload stale chapter prose — silently showing outdated
    // content when another writer concurrently edited the same chapter. The
    // content editor's content-sync effect guards on outline.isFetching, so it
    // waits for this refetch before applying the forced reset.
    void qc.invalidateQueries({
      queryKey: [...queryKeys.chapters.outlines(), workId],
    });
    // Force the content editor to discard its draft and reload the canonical
    // content. contentVersion is no longer bumped on ordinary patches, so this
    // bump is a reliable forced-reset signal that overrides the editor's
    // dirty/saving guard.
    setContentVersion((v) => v + 1);
  }

  function onDismiss() {
    setConflict(null);
  }

  async function onReapply() {
    if (!conflict) return;
    setConflict(null);
    const fresh = await outline.refetch();
    const baseRevision = fresh.data?.outline_revision;
    if (baseRevision === undefined) return;
    const { pendingRequest } = conflict;
    if (pendingRequest.kind === 'structure') {
      handleStructure({ ...pendingRequest.request, base_revision: baseRevision });
    } else if (pendingRequest.kind === 'chapter') {
      handleChapter(pendingRequest.chapter, {
        ...pendingRequest.request,
        base_revision: baseRevision,
      });
    } else {
      handleTimeline({ ...pendingRequest.request, base_revision: baseRevision });
    }
  }

  if (outline.isError || chaptersQuery.isError || work.isError) {
    return (
      <ErrorState
        title={t('outline.loadError.title')}
        description={t('outline.loadError.description')}
        onRetry={() => {
          void outline.refetch();
          void chaptersQuery.refetch();
          void work.refetch();
        }}
      />
    );
  }

  if (outline.isLoading || chaptersQuery.isLoading || work.isLoading) {
    return <LoadingState label={t('outline.loading')} />;
  }

  if (!outline.data) {
    return (
      <EmptyState
        title={t('outline.empty.title')}
        description={t('outline.empty.description')}
      />
    );
  }

  // Update the mutable adapter context every render (after early returns, before
  // JSX). The adapter object is stable, so useCanvasSurface's memoized graph
  // projection survives state changes; inspectors/alt-view rendered via the
  // adapter read fresh values from this ref at their render time.
  ctxRef.current = {
    translateFallback,
    t,
    workId,
    outline: outline.data,
    chapters,
    chapterById,
    fixture,
    altViewSceneBeatFixture,
    onPatchChapter: handleChapter,
    onMove: (chapterId: number, volumeId: number) =>
      handleStructure({
        work_id: workId,
        base_revision: outline.data.outline_revision,
        operation: 'move_chapter',
        chapter_id: chapterId,
        volume_id: volumeId,
      }),
    patchChapterIsPending: patchChapter.isPending,
    isConflicting: conflict !== null,
    contentVersion,
  };

  return (
    <div className="flex flex-col gap-4">
      <CanvasHeader
        title={work.data?.title ?? t('outline.untitledWork')}
        subtitle={t('outline.subtitle')}
        revision={outline.data.outline_revision}
        status={patchStructure.isPending ? 'dirty' : 'clean'}
        showAlt={showAlt}
        setShowAlt={setShowAlt}
      />

      {/* V1.200 DR-26 (Greptile wave A) — the alt view reads the same canonical
          carrier as the graph and inspectors (an explicit Studio fixture still
          wins), so authored scenes/beats appear in list view too. With no
          explicit fixture and genuinely empty canonical arrays it stays
          undefined → honest empty chrome (no invented scene structure). */}
      {showAlt ? (
        <OutlineAltView
          outline={outline.data}
          chapters={chapters}
          sceneBeatFixture={altViewSceneBeatFixture}
        />
      ) : (
        <CanvasShell
          nodes={surface.nodes}
          edges={surface.edges}
          nodeTypes={surface.nodeTypes}
          onNodesChange={surface.onNodesChange}
          summaryText={summary}
          ariaLabel={t('outline.graphAriaLabel')}
          surfaceKey={`outline:${workId}`}
          // v1.183 P0 QC fix (qc2 F-003): the Outline surface reads the
          // minimap with its own accent token, not the shell's strategy
          // default.
          minimapAccent="var(--color-canvas-outline-accent)"
        >
          {/* I-QC1-001 — when the projection has zero nodes, render the
              EmptyState as an in-shell overlay so CanvasShell is always
              mounted for the graph view (FB-C1-000 shared-shell parity). */}
          {surface.nodes.length === 0 ? (
            <div className="pointer-events-none absolute inset-0 flex items-center justify-center">
            <EmptyState
              title={t('outline.noGraph.title')}
              description={t('outline.noGraph.description')}
            />
            </div>
          ) : null}
        </CanvasShell>
      )}

      <div className="grid gap-4 lg:grid-cols-[1fr_360px]">
        <OutlineStructurePanel
          outline={outline.data}
          chapters={chapters}
          selectedChapterId={selectedChapterId}
          onSelectChapter={setSelectedChapterId}
          onMoveChapter={(chapterId, volumeId) =>
            handleStructure({
              work_id: workId,
              base_revision: outline.data.outline_revision,
              operation: 'move_chapter',
              chapter_id: chapterId,
              volume_id: volumeId,
            })
          }
        />

        <div className="flex flex-col gap-4">
          {/* V1.109 C2 T4 — Scene/Beat inspector mounting (FB-C2-002). The
              hook's selection coordination ensures only one of Beat/Scene/
              Chapter is selected at a time. When a Beat is selected, show the
              Beat inspector; when a Scene is selected, show the Scene
              inspector; otherwise fall through to the Chapter inspector
              (default — includes its empty state when no chapter is selected).
              On real Works (empty fixture), selectedBeat/selectedScene are
              always null → Chapter inspector is always shown (no regression). */}
          {selectedBeat ? (
            <BeatInspector beat={selectedBeat} parentSceneTitle={beatParentSceneTitle} />
          ) : selectedScene ? (
            <SceneInspector scene={selectedScene} parentChapterTitle={sceneParentChapterTitle} />
          ) : (
            <ChapterInspector
              workId={workId}
              outline={outline.data}
              chapter={selectedChapter}
              baseRevision={outline.data.outline_revision}
              onPatchChapter={handleChapter}
              onMove={(chapterId, volumeId) =>
                handleStructure({
                  work_id: workId,
                  base_revision: outline.data.outline_revision,
                  operation: 'move_chapter',
                  chapter_id: chapterId,
                  volume_id: volumeId,
                })
              }
              patchIsPending={patchChapter.isPending}
              isConflicting={conflict !== null}
              contentVersion={contentVersion}
            />
          )}

          <TimelinePanel
            outline={outline.data}
            selectedChapterId={selectedChapterId}
            baseRevision={outline.data.outline_revision}
            onPatchTimeline={handleTimeline}
            boundWorldId={work.data?.world_id ?? undefined}
          />
        </div>
      </div>

      <OutlineConflictDialog
        conflict={conflict}
        onUseCurrent={onUseCurrent}
        onReapply={onReapply}
        onDismiss={onDismiss}
      />

      {/* V1.200 DR-26 Task 3 — scene/beat authoring. Authoring lives on the
          Outline surface (both Timeline Moment layers are read-only
          projections); every control routes through `handleStructure`, so a
          stale-revision 409 opens the conflict modal above rather than a new
          error path. */}
      <SceneBeatAuthoringPanel
        outline={outline.data}
        chapters={chapters}
        onPatchStructure={handleStructure}
        isPending={patchStructure.isPending}
      />
    </div>
  );
}

/**
 * V1.200 DR-26 Task 3 — Scene/Beat authoring panel.
 *
 * Surfaces the four `outline.patch_structure` scene/beat operations (Task 2)
 * on the Outline canvas — the single authoring surface (the Work and World
 * Timeline Moment layers remain read-only projections).
 *
 * Write path: every control calls the orchestrator's `onPatchStructure` (the
 * same `usePatchOutlineStructure` mutation + `captureConflictState` wiring the
 * structure/timeline patches use), so a stale-revision 409 opens the existing
 * conflict modal — no new error path. The read-only Scene/Beat inspectors are
 * untouched.
 *
 * Guards (V1.200 Greptile wave A):
 *   • Draft titles are cleared only after their write SUCCEEDS, so a 409 /
 *     published refusal keeps the draft for retry.
 *   • `remove_scene` on a scene that owns beats asks for confirmation first and
 *     names the beats the cascade will take with it (`window.confirm`, the
 *     outline/inspector confirm pattern) — the cascade scope is visible before
 *     the click commits.
 *
 * Reads: the canonical `outline.scenes` / `outline.beats` arrays. Empty arrays
 * are real emptiness (no fixture substitution); server-minted `scn_`/`bet_`
 * ids are observed through the post-write canonical refetch the mutation
 * invalidation triggers.
 *
 * Server rules mirrored (not re-implemented): `add_scene` needs an existing
 * chapter, `add_beat` an existing scene of this Work, both need a non-blank
 * title; the daemon is the authority (its 422 validation envelope surfaces
 * through the existing error toast, its 409 through the conflict modal).
 */
function SceneBeatAuthoringPanel({
  outline,
  chapters,
  onPatchStructure,
  isPending,
}: {
  outline: WorkOutline;
  chapters: ChapterSummary[];
  onPatchStructure: (request: OutlinePatchStructureRequest, onSuccess?: () => void) => void;
  isPending: boolean;
}) {
  const { t } = useTranslation('canvas');
  const [sceneTitle, setSceneTitle] = useState('');
  const [chapterValue, setChapterValue] = useState('');
  const [beatTitles, setBeatTitles] = useState<Record<string, string>>({});

  const chapterId = chapterValue === '' ? chapters[0]?.chapter : Number(chapterValue);
  const canAddScene = !isPending && sceneTitle.trim().length > 0 && chapterId !== undefined;

  const base = { work_id: outline.work_id, base_revision: outline.outline_revision };

  function submitScene() {
    if (!canAddScene || chapterId === undefined) return;
    onPatchStructure(
      {
        ...base,
        operation: 'add_scene',
        chapter_id: chapterId,
        title: sceneTitle.trim(),
      },
      // Clear the draft only once the write lands: a 409 / published refusal
      // (which opens the conflict modal) keeps the title for retry.
      () => setSceneTitle(''),
    );
  }

  function submitBeat(sceneId: string) {
    const title = (beatTitles[sceneId] ?? '').trim();
    if (isPending || title.length === 0) return;
    onPatchStructure(
      { ...base, operation: 'add_beat', scene_id: sceneId, title },
      // Same success-gated clear as the scene draft above.
      () => setBeatTitles((prev) => ({ ...prev, [sceneId]: '' })),
    );
  }

  return (
    <Card data-testid="outline-scene-beat-authoring">
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          <Clapperboard className="h-5 w-5 text-canvas-outline-accent" aria-hidden />
          {t('sceneBeatAuthoring.title')}
        </CardTitle>
        <CardDescription>{t('sceneBeatAuthoring.description')}</CardDescription>
      </CardHeader>
      <CardContent className="space-y-4">
        <div className="flex flex-wrap items-end gap-3">
          <MetaField label={t('sceneBeatAuthoring.chapter')}>
            <select
              data-testid="outline-scene-chapter"
              value={chapterValue}
              onChange={(event) => setChapterValue(event.target.value)}
              disabled={isPending || chapters.length === 0}
              className={INPUT_CLASS}
            >
              {chapters.map((chapter) => (
                <option key={chapter.chapter} value={String(chapter.chapter)}>
                  {t('chapter.fallback', { chapter: chapter.chapter })}
                </option>
              ))}
            </select>
          </MetaField>
          <MetaField label={t('sceneBeatAuthoring.sceneTitle')}>
            <input
              type="text"
              data-testid="outline-scene-title"
              value={sceneTitle}
              onChange={(event) => setSceneTitle(event.target.value)}
              disabled={isPending}
              className={INPUT_CLASS}
            />
          </MetaField>
          <Button
            type="button"
            variant="secondary"
            size="small"
            data-testid="outline-add-scene"
            disabled={!canAddScene}
            onClick={submitScene}
          >
            <Plus className="h-4 w-4" aria-hidden />
            {t('sceneBeatAuthoring.addScene')}
          </Button>
        </div>

        {chapters.length === 0 ? (
          <p className="text-copy-13 text-gray-700">
            {t('sceneBeatAuthoring.noChapters')}
          </p>
        ) : null}

        {outline.scenes.length === 0 ? (
          <EmptyState
            title={t('sceneBeatAuthoring.empty.title')}
            description={t('sceneBeatAuthoring.empty.description')}
          />
        ) : (
          <ul className="space-y-3" data-testid="outline-scene-list">
            {outline.scenes.map((scene) => {
              const beats = outline.beats.filter((beat) => beat.scene_id === scene.scene_id);
              const beatTitle = beatTitles[scene.scene_id] ?? '';

              // Cascade guard (V1.200 Greptile wave A) — removing a scene also
              // removes every beat it owns, so confirm first and name the
              // affected beats (`window.confirm`, the outline/inspector confirm
              // pattern). A scene with no beats removes directly.
              function removeScene() {
                if (beats.length > 0) {
                  const affected = beats
                    .map((beat) => beat.title?.trim() || t('sceneBeatAuthoring.untitledBeat'))
                    .join(', ');
                  if (
                    !window.confirm(t('sceneBeatAuthoring.removeSceneConfirm', { beats: affected }))
                  ) {
                    return;
                  }
                }
                onPatchStructure({
                  ...base,
                  operation: 'remove_scene',
                  scene_id: scene.scene_id,
                });
              }

              return (
                <li
                  key={scene.scene_id}
                  className="rounded-card border border-gray-alpha-300 bg-background-100 p-3"
                >
                  <div className="flex flex-wrap items-center justify-between gap-2">
                    <div className="flex items-center gap-2">
                      <span className="text-label-14 font-semibold text-gray-900">
                        {scene.title || t('sceneBeatAuthoring.untitledScene')}
                      </span>
                      <Badge variant="neutral">
                        {t('sceneBeatAuthoring.chapterBadge', { chapter: scene.chapter_id })}
                      </Badge>
                    </div>
                    <Button
                      type="button"
                      variant="tertiary"
                      size="small"
                      data-testid={`outline-remove-scene-${scene.scene_id}`}
                      disabled={isPending}
                      onClick={removeScene}
                    >
                      <Trash2 className="h-4 w-4" aria-hidden />
                      {t('sceneBeatAuthoring.remove')}
                    </Button>
                  </div>

                  {beats.length > 0 ? (
                    <ul className="mt-2 space-y-1">
                      {beats.map((beat) => (
                        <li
                          key={beat.beat_id}
                          className="flex items-center justify-between gap-2 text-copy-13 text-gray-900"
                        >
                          <span>
                            {beat.title || t('sceneBeatAuthoring.untitledBeat')}
                          </span>
                          <Button
                            type="button"
                            variant="tertiary"
                            size="small"
                            data-testid={`outline-remove-beat-${beat.beat_id}`}
                            disabled={isPending}
                            onClick={() =>
                              onPatchStructure({
                                ...base,
                                operation: 'remove_beat',
                                beat_id: beat.beat_id,
                              })
                            }
                          >
                            <Trash2 className="h-4 w-4" aria-hidden />
                            {t('sceneBeatAuthoring.remove')}
                          </Button>
                        </li>
                      ))}
                    </ul>
                  ) : null}

                  <div className="mt-2 flex flex-wrap items-center gap-2">
                    <input
                      type="text"
                      data-testid={`outline-beat-title-${scene.scene_id}`}
                      aria-label={t('sceneBeatAuthoring.beatTitle')}
                      value={beatTitle}
                      onChange={(event) =>
                        setBeatTitles((prev) => ({ ...prev, [scene.scene_id]: event.target.value }))
                      }
                      disabled={isPending}
                      className={INPUT_CLASS}
                    />
                    <Button
                      type="button"
                      variant="secondary"
                      size="small"
                      data-testid={`outline-add-beat-${scene.scene_id}`}
                      disabled={isPending || beatTitle.trim().length === 0}
                      onClick={() => submitBeat(scene.scene_id)}
                    >
                      <Plus className="h-4 w-4" aria-hidden />
                      {t('sceneBeatAuthoring.addBeat')}
                    </Button>
                  </div>
                </li>
              );
            })}
          </ul>
        )}
      </CardContent>
    </Card>
  );
}
