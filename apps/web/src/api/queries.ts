/**
 * TanStack Query hooks for the Control Room + Setup screens.
 *
 * Each hook reads via the `NexusClient` interface (transport-agnostic). List
 * endpoints now return the canonical `{ items, pagination }` shape (F-P3) and
 * accept a single `sort` query parameter (F-F1). Cursor-paginated lists (Works,
 * Findings, Sessions, Schedules, Capabilities) use server order.
 *
 * Findings + Works use cursor pagination; the hook exposes TanStack's
 * `fetchNextPage`/`hasNextPage` for "Load more".
 */
import { useCallback, useEffect, useRef } from 'react';
import { useTranslation } from 'react-i18next';
import {
  useInfiniteQuery,
  useMutation,
  useQuery,
  useQueryClient,
} from '@tanstack/react-query';
import type {
  BatchUpdateFindingsRequest,
  ChapterContentQuery,
  ChapterSummary,
  CountPendingReviewsResponse,
  CreateWorkRequest,
  CreateWorldRequest,
  FindingDetailResponse,
  ListCapabilitiesQuery,
  ListChaptersQuery,
  ListCreatorsQuery,
  ListCreatorsResponse,
  ListFindingsQuery,
  ListMemoryFragmentsQuery,
  ListPendingReviewsQuery,
  ListSchedulesQuery,
  ListSessionsQuery,
  ListWorksQuery,
  ModuleSummary,
  PaginationInfo,
  PatchChapterRequest,
  PatchWorkRequest,
  PendingReviewInfo,
  PresetSummary,
  ReadingAnnotation,
  ReadingAnnotationCreateRequest,
  ReadingAnnotationListResponse,
  ReadingAnnotationPatchRequest,
  ReadingProgressResponse,
  ReviewResponse,
  RunAcceptRequest,
  RunRequest,
  RunSummary,
  ScaffoldPresetRequest,
  ScanRequest,
  ScanResponse,
  SoulNarrativeResponse,
  TimelineOverviewResponse,
  TimelineEventInfo,
  UpdateFindingRequest,
  ValidatePresetRequest,
  WorkSummary,
  World,
} from '@42ch/nexus-contracts';

import { useToast } from '@/lib/use-toast';
import { useDesktopCapabilities, useNexusClient } from '@/lib/client-context';
import { NexusClientError, type ListRunsQuery } from '@/lib/nexus';
import { shortId } from '@/lib/format';
import { queryKeys } from '@/lib/nexus/query-keys';
import { useActiveCreatorId as useActiveCreatorIdFromContext } from '@/lib/active-creator-context';

/** Default page size for cursor-paginated lists. */
export const DEFAULT_PAGE_SIZE = 20;

interface CursorPage<T> {
  items: T[];
  pagination: PaginationInfo;
}

/** Cursor token type for infinite queries (undefined on the first page). */
type Cursor = string | undefined;

const FIRST_PAGE: Cursor = undefined;

// ── Works (cursor-paginated dashboard) ───────────────────────────────────────

/** Cursor-paginated Works list (F-P1/F-P3/F-F1). */
export function useWorks(query?: ListWorksQuery) {
  const client = useNexusClient();
  const limit = query?.limit ?? DEFAULT_PAGE_SIZE;
  return useInfiniteQuery({
    queryKey: queryKeys.works.list({ ...query, limit }),
    initialPageParam: FIRST_PAGE,
    queryFn: async ({ pageParam }): Promise<CursorPage<WorkSummary>> => {
      const res = await client.listWorks({ ...query, limit, cursor: pageParam });
      return {
        items: res.items,
        pagination: res.pagination,
      };
    },
    getNextPageParam: (lastPage: CursorPage<WorkSummary>): Cursor =>
      lastPage.pagination.has_more ? lastPage.pagination.next_cursor : undefined,
  });
}

export function useWork(workId: string | undefined) {
  const client = useNexusClient();
  return useQuery({
    queryKey: queryKeys.works.detail(workId ?? ''),
    queryFn: () => client.getWork(workId!),
    enabled: Boolean(workId),
  });
}

// ── Sessions (cursor-paginated) ──────────────────────────────────────────────

/**
 * AD-P0-2c (V1.120 P2 / F3): Sessions is an active-work monitor. The daemon
 * list handler excludes `_system.*` boot sessions (AD-P0-2b); this defensive
 * filter is the belt for older daemons that still return them. Pure and
 * exported for unit tests.
 */
export function filterVisibleSessions<T extends { preset_id: string }>(items: T[]): T[] {
  return items.filter((s) => !s.preset_id.startsWith('_system.'));
}

export function useSessions(query?: ListSessionsQuery) {
  const client = useNexusClient();
  return useQuery({
    queryKey: queryKeys.sessions.list(query),
    queryFn: async () => {
      const res = await client.listSessions(query);
      return filterVisibleSessions(res.items);
    },
  });
}

// ── Schedules (cursor-paginated) ─────────────────────────────────────────────

export function useSchedules(query?: ListSchedulesQuery) {
  const client = useNexusClient();
  return useQuery({
    queryKey: queryKeys.schedules.list(query),
    queryFn: async () => {
      const res = await client.listSchedules(query);
      return res.items;
    },
  });
}

// ── Capabilities (cursor-paginated; default server order is by name) ─────────

export function useCapabilities(query?: ListCapabilitiesQuery) {
  const client = useNexusClient();
  return useQuery({
    queryKey: queryKeys.capabilities.list(query),
    queryFn: async () => {
      const res = await client.listCapabilities(query);
      return res.items;
    },
  });
}

// ── Findings (cursor-paginated per Work) ─────────────────────────────────────

export function useFindings(workId: string | undefined, query?: ListFindingsQuery) {
  const client = useNexusClient();
  const limit = query?.limit ?? DEFAULT_PAGE_SIZE;
  return useInfiniteQuery({
    queryKey: queryKeys.findings.list(workId ?? '', { ...query, limit }),
    initialPageParam: FIRST_PAGE,
    queryFn: async ({ pageParam }): Promise<CursorPage<FindingDetailResponse>> => {
      const res = await client.listFindings(workId!, { ...query, limit, cursor: pageParam });
      return {
        items: res.items,
        pagination: res.pagination,
      };
    },
    enabled: Boolean(workId),
    getNextPageParam: (lastPage: CursorPage<FindingDetailResponse>): Cursor =>
      lastPage.pagination.has_more ? lastPage.pagination.next_cursor : undefined,
  });
}

/** Flatten an infinite-query data structure into one items array. */
export function flattenPages<T>(data: { pages: CursorPage<T>[] } | undefined): T[] {
  if (!data) return [];
  return data.pages.flatMap((p) => p.items);
}

// Forward-staging closure: a `useFinding(workId, findingId)` detail hook was
// considered for the V1.77 remediation surface but is intentionally absent
// here. The FindingDetailPanel reads the selected row from the work-scoped
// list cache, which is sufficient while the UI is a list+inspector hybrid. A
// dedicated detail-endpoint hook can be re-introduced when (and only when) a
// standalone finding-detail route or inspector needs it as the source of truth
// (qc1 S-001).

// ── Presets (grouped by source) ──────────────────────────────────────────────

export interface PresetGroups {
  embedded: PresetSummary[];
  system: PresetSummary[];
  user: PresetSummary[];
}

export function usePresets() {
  const client = useNexusClient();
  return useQuery({
    queryKey: queryKeys.presets.list(),
    queryFn: async (): Promise<PresetGroups> => client.listPresets(),
  });
}

// ── Timeline (V1.126 P2; V1.127 P0 T2 infinite pagination) ────────────────────

/**
 * Cursor-paginated global Timeline overview across all Worlds.
 *
 * V1.127 P0 T2: upgraded from a single-page `useQuery` to `useInfiniteQuery`
 * so the Global Timeline + Worlds page can page past the daemon's 20-World
 * page cap (`TimelineOverviewResponse.worlds.maxItems: 20`). The composite
 * response carries the NEXT cursor directly as `TimelineOverviewResponse.cursor`
 * — there is no separate `has_more` flag, so `getNextPageParam` treats a
 * non-null cursor as "has next page" and the cursor string itself as the next
 * page param. Consumers drive their "Load more" controls off `hasNextPage` /
 * `fetchNextPage` and flatten pages via {@link flattenOverviewWorlds}.
 */
export function useTimelineOverview() {
  const client = useNexusClient();
  return useInfiniteQuery({
    queryKey: queryKeys.timeline.overview(),
    initialPageParam: FIRST_PAGE,
    queryFn: async ({ pageParam }): Promise<TimelineOverviewResponse> => {
      return client.getTimelineOverview(pageParam);
    },
    getNextPageParam: (lastPage: TimelineOverviewResponse): Cursor =>
      lastPage.cursor ?? undefined,
    staleTime: 10_000,
  });
}

/** Flatten the infinite overview pages into one worlds array. */
export function flattenOverviewWorlds(
  data: { pages: TimelineOverviewResponse[] } | undefined,
): TimelineOverviewResponse['worlds'] {
  if (!data) return [];
  return data.pages.flatMap((p) => p.worlds);
}

// ── Mutations (Setup writes) ─────────────────────────────────────────────────

/**
 * Surface a NexusClientError as a toast; callers may still read the result.
 *
 * V1.129 P1: when the error carries a `kind` (transport-classified, status=0),
 * the toast description becomes the shared single-source body copy from
 * `shell.profile.createError.<kind>.body` — same locale keys the
 * create-creator dialog and FingerprintGate use. The caller-supplied `key`
 * stays the toast title (the action context, e.g.
 * `error.couldNotCreateWork`). Non-transport errors (HTTP 4xx/5xx, generic
 * throws) keep the legacy `errorMessage(error)` description so actionable
 * text from the daemon still surfaces.
 *
 * Toast CTA limitation: the current `useToast` API (`@42ch/nexus-ui` Toast)
 * has no action slot — only `title` + `description` + variant/duration. The
 * CTAs available on the full-page `<TransportErrorBlock>` (Retry, Open
 * Connection Settings) therefore cannot surface in the toast. Per the V1.129
 * P1 architect lock, the toast gets headline + body only; CTAs stay on
 * full-page / inline error blocks. Adding an action slot to the toast
 * component is out of P1 scope.
 */
export function useErrorToast() {
  const { toast } = useToast();
  const { t: commonT } = useTranslation('common');
  const { t: shellT } = useTranslation('shell');
  return (error: unknown, key: string) => {
    const title = commonT(key, { defaultValue: key });
    const kind =
      error instanceof NexusClientError && error.kind ? error.kind : null;
    const description = kind
      ? shellT(`profile.createError.${kind}.body`)
      : error instanceof Error
        ? error.message
        : commonT('error.unexpected');
    toast({ variant: 'error', title, description });
  };
}

export function useCreateWork() {
  const client = useNexusClient();
  const qc = useQueryClient();
  const errorToast = useErrorToast();
  return useMutation({
    mutationFn: (request: CreateWorkRequest) => client.createWork(request),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: queryKeys.works.lists() });
    },
    onError: (error) => errorToast(error, 'error.couldNotCreateWork'),
  });
}

export function useCreateWorld() {
  const client = useNexusClient();
  const qc = useQueryClient();
  const errorToast = useErrorToast();
  return useMutation({
    mutationFn: (request: CreateWorldRequest) => client.createWorld(request),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: queryKeys.memory.worlds() });
      // Mirror useDeleteWorld: Timeline overview is World-centric (era/event
      // counts per World). A newly created World must appear in overview
      // caches, not only the narrative worlds list.
      void qc.invalidateQueries({ queryKey: queryKeys.timeline.all });
    },
    onError: (error) => errorToast(error, 'error.couldNotCreateWorld'),
  });
}

export function usePatchWork() {
  const client = useNexusClient();
  const qc = useQueryClient();
  const errorToast = useErrorToast();
  return useMutation({
    mutationFn: (vars: { workId: string; request: PatchWorkRequest }) =>
      client.patchWork(vars.workId, vars.request),
    onSuccess: (_data, vars) => {
      void qc.invalidateQueries({ queryKey: queryKeys.works.lists() });
      void qc.invalidateQueries({ queryKey: queryKeys.works.detail(vars.workId) });
    },
    onError: (error) => errorToast(error, 'error.couldNotUpdateWork'),
  });
}

/**
 * Hard-delete a Work (V1.129 P2 — R-V1126P0-T2-001).
 *
 * Cascade is server-side (SQLite FK `ON DELETE CASCADE` on chapters, findings,
 * pool entries, reading progress/annotations; SET NULL on inspiration items
 * and lineage_from_work_id). The mutation invalidates the works list on
 * success; transport errors route through the shared error-toast classifier
 * (which surfaces `<TransportErrorBlock>` copy for known kinds).
 *
 * No timeline.invalidation here: `TimelineOverviewResponse.worlds` is World-
 * centric (per-World era/event counts) and carries no Work rows, so deleting a
 * Work cannot stale the timeline overview cache. (Checked V1.129 P5.)
 */
export function useDeleteWork() {
  const client = useNexusClient();
  const qc = useQueryClient();
  const errorToast = useErrorToast();
  return useMutation({
    mutationFn: (workId: string) => client.deleteWork(workId),
    onSuccess: (_data, workId) => {
      void qc.invalidateQueries({ queryKey: queryKeys.works.lists() });
       qc.removeQueries({ queryKey: queryKeys.works.detail(workId) });
    },
    onError: (error) => errorToast(error, 'error.couldNotDeleteWork'),
  });
}

/**
 * Hard-delete a World (V1.129 P2 — R-V1126P0-T2-001).
 *
 * Cascade is server-side: KB + timelines drop via FK CASCADE; Works are
 * preserved with `world_id = NULL` (architect lock). The mutation invalidates
 * the narrative world list on success.
 */
export function useDeleteWorld() {
  const client = useNexusClient();
  const qc = useQueryClient();
  const errorToast = useErrorToast();
  return useMutation({
    mutationFn: (worldId: string) => client.deleteWorld(worldId),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: queryKeys.memory.worlds() });
      // V1.129 P5 (Greptile P1): the Global Timeline overview is World-centric
      // — `TimelineOverviewResponse.worlds` lists each World with its era/event
      // counts, so a deleted World must be evicted from the overview cache or
      // it keeps rendering until the next manual refetch. Invalidating
      // `timeline.all` covers every cached cursor page (overview is the only
      // timeline sub-key today, and the `overview(cursor)` key is parameterized
      // by cursor, so a single `overview()` invalidation would miss non-first
      // pages).
      void qc.invalidateQueries({ queryKey: queryKeys.timeline.all });
    },
    onError: (error) => errorToast(error, 'error.couldNotDeleteWorld'),
  });
}

/**
 * Update a finding (V1.77 findings-remediation). Optimistically patches the
 * finding in the cached findings list before the server responds, rolls back on
 * error, and refetches the list + detail on settle. Last-writer-wins (D1b — no
 * OCC, no conflict modal); the quality loop is single-author-triage, so an
 * optimistic update cannot collide with a concurrent author.
 *
 * The server enforces the 6-state lifecycle adjacency (HTTP 422
 * `INVALID_TRANSITION`); the UI disables illegal transitions as defense-in-
 * depth, but a bypass reaches the server and rolls back here.
 */
export function useUpdateFinding() {
  const client = useNexusClient();
  const qc = useQueryClient();
  const errorToast = useErrorToast();
  const { t } = useTranslation('common');
  const { toast } = useToast();
  type FindingsListData = { pages: CursorPage<FindingDetailResponse>[] };
  return useMutation({
    mutationFn: (vars: { workId: string; findingId: string; patch: UpdateFindingRequest }) =>
      client.updateFinding(vars.workId, vars.findingId, vars.patch),
    onMutate: async (vars) => {
      // Cancel outgoing refetches for this Work so they don't overwrite the
      // optimistic update. Scope to vars.workId — cancelling other Works'
      // lists is unnecessary and contradicts the work-scoped invalidation
      // (qc3 W-QC3-P0-001).
      await qc.cancelQueries({ queryKey: queryKeys.findings.list(vars.workId) });
      // Snapshot every matched list cache for this work (across query filters)
      // so onError can restore the pre-mutation state.
      const previousLists = qc.getQueriesData<FindingsListData>({
        queryKey: queryKeys.findings.list(vars.workId),
      });
      // Only apply defined patch fields — undefined means "no-op" on the wire
      // and must not clobber the cached value during the optimistic merge.
      const optimistic = Object.fromEntries(
        Object.entries(vars.patch).filter(([, v]) => v !== undefined),
      );
      qc.setQueriesData<FindingsListData>(
        { queryKey: queryKeys.findings.list(vars.workId) },
        (old) => {
          if (!old) return old;
          return {
            ...old,
            pages: old.pages.map((page) => ({
              ...page,
              items: page.items.map((f) =>
                f.finding_id === vars.findingId ? { ...f, ...optimistic } : f,
              ),
            })),
          };
        },
      );
      // Asymmetry note: `getQueriesData` snapshots every matched list under the
      // work-scoped prefix (all filter views), while `setQueryData` restores
      // each snapshot by its exact query key. This is correct because TanStack's
      // `setQueryData` ignores filters that are not part of the exact key tuple;
      // a rollback must target the same key that was snapshotted. If the
      // snapshot/apply scope ever changes (e.g., page-cursor filters become part
      // of the key), the rollback loop must be widened to match (qc1 S-004).
      return { previousLists };
    },
    onError: (error, _vars, context) => {
      if (context?.previousLists) {
        for (const [queryKey, data] of context.previousLists) {
          qc.setQueryData(queryKey, data);
        }
      }
      errorToast(error, 'error.couldNotUpdateFinding');
    },
    onSuccess: (_data, vars) => {
      toast({ variant: 'success', title: t('toast.findingUpdated'), description: shortId(vars.findingId) });
    },
    onSettled: (_data, _error, vars) => {
      // Narrow to the mutated Work's list scope (all filter views of that Work
      // only), not the global findings-list prefix — so a status change /
      // assignment / inline edit in one Work doesn't mark every other Work's
      // findings lists stale and refetch them (qc3 W-QC3-P0-001). The scoped
      // refetch is still needed: a status transition can move a finding between
      // filter views of this Work.
      void qc.invalidateQueries({ queryKey: queryKeys.findings.list(vars.workId) });
    },
  });
}

/**
 * Bulk update findings (V1.91 P1). Calls `PATCH /v1/daemon/findings/batch` for
 * up to 100 IDs. Partial success: the server returns counts and `not_found` /
 * `conflict` arrays; this hook surfaces those in a toast and invalidates the
 * work-scoped findings list so the table reflects the applied changes.
 */
export function useBatchUpdateFindings() {
  const client = useNexusClient();
  const qc = useQueryClient();
  const errorToast = useErrorToast();
  const { t } = useTranslation('common');
  const { toast } = useToast();
  return useMutation({
    mutationFn: (vars: { workId: string; request: BatchUpdateFindingsRequest }) =>
      client.batchUpdateFindings(vars.request),
    onSuccess: (data, vars) => {
      if (data.updated && !data.not_found?.length && !data.conflict?.length) {
        toast({
          variant: 'success',
          title: t('toast.batchUpdateComplete'),
          description: t('toast.batchUpdateCompleteDescription', { count: data.updated }),
        });
      } else {
        if (data.updated) {
          toast({ variant: 'success', title: t('toast.batchUpdate'), description: t('toast.batchUpdateDescription', { count: data.updated }) });
        }
        if (data.not_found?.length) {
          toast({
            variant: 'warning',
            title: t('toast.findingsNotFound'),
            description: t('toast.findingsNotFoundDescription', { count: data.not_found.length }),
            duration: 0,
          });
        }
        if (data.conflict?.length) {
          toast({
            variant: 'warning',
            title: t('toast.findingsConflict'),
            description: t('toast.findingsConflictDescription', { count: data.conflict.length }),
            duration: 0,
          });
        }
      }
      void qc.invalidateQueries({ queryKey: queryKeys.findings.list(vars.workId) });
    },
    onError: (error) => errorToast(error, 'error.couldNotUpdateFindings'),
  });
}

export function useScaffoldPreset() {
  const client = useNexusClient();
  const qc = useQueryClient();
  const errorToast = useErrorToast();
  return useMutation({
    mutationFn: (request: ScaffoldPresetRequest) => client.scaffoldPreset(request),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: queryKeys.presets.list() });
    },
    onError: (error) => errorToast(error, 'error.couldNotScaffoldPreset'),
  });
}

export function useValidatePreset() {
  const client = useNexusClient();
  const errorToast = useErrorToast();
  return useMutation({
    mutationFn: (request: ValidatePresetRequest) => client.validatePreset(request),
    onError: (error) => errorToast(error, 'error.couldNotValidatePreset'),
    // On success the caller surfaces structured errors/warnings inline; a toast
    // is not added here so the validate dialog stays the single source of truth.
  });
}

export function useReloadPreset() {
  const client = useNexusClient();
  const qc = useQueryClient();
  const errorToast = useErrorToast();
  const { t } = useTranslation('common');
  const { toast } = useToast();
  return useMutation({
    mutationFn: (presetId: string) => client.reloadPreset(presetId),
    onSuccess: (_data, presetId) => {
      toast({ variant: 'success', title: t('toast.presetReloaded'), description: presetId });
      void qc.invalidateQueries({ queryKey: queryKeys.presets.list() });
    },
    onError: (error) => errorToast(error, 'error.couldNotReloadPreset'),
  });
}

/**
 * Delete a user preset (V1.120 strategies-repair). Mirrors the
 * `useScaffoldPreset` shape: `DELETE /v1/daemon/presets/{id}` then invalidate
 * the grouped presets list so the row disappears. Only user presets are
 * deletable — the row UI gates this (system/embedded rows render no Delete).
 */
export function useDeletePreset() {
  const client = useNexusClient();
  const qc = useQueryClient();
  const errorToast = useErrorToast();
  return useMutation({
    mutationFn: (presetId: string) => client.deletePreset(presetId),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: queryKeys.presets.list() });
    },
    onError: (error) => errorToast(error, 'error.couldNotDeletePreset'),
  });
}

// ── Chapters (V1.65 Content-Authoring) ───────────────────────────────────────

/** Cursor-paginated chapter list for a Work (F-P3 `items` key). */
export function useChapters(workId: string | undefined, query?: ListChaptersQuery) {
  const client = useNexusClient();
  const limit = query?.limit ?? DEFAULT_PAGE_SIZE;
  return useInfiniteQuery({
    queryKey: queryKeys.chapters.list(workId ?? '', { ...query, limit }),
    initialPageParam: FIRST_PAGE,
    queryFn: async ({ pageParam }): Promise<CursorPage<ChapterSummary>> => {
      const res = await client.listChapters(workId!, { ...query, limit, cursor: pageParam });
      return {
        items: res.items,
        pagination: res.pagination,
      };
    },
    enabled: Boolean(workId),
    getNextPageParam: (lastPage: CursorPage<ChapterSummary>): Cursor =>
      lastPage.pagination.has_more ? lastPage.pagination.next_cursor : undefined,
  });
}

export function useChapter(workId: string | undefined, chapter: number | undefined, query?: ChapterContentQuery) {
  const client = useNexusClient();
  return useQuery({
    queryKey: queryKeys.chapters.detail(workId ?? '', chapter ?? 0, query),
    queryFn: () => client.getChapter(workId!, chapter!, query),
    enabled: Boolean(workId) && typeof chapter === 'number' && chapter > 0,
  });
}

export function useChapterOutline(
  workId: string | undefined,
  chapter: number | undefined,
  query?: ChapterContentQuery,
) {
  const client = useNexusClient();
  return useQuery({
    queryKey: queryKeys.chapters.outline(workId ?? '', chapter ?? 0, query),
    queryFn: () => client.getChapterOutline(workId!, chapter!, query),
    enabled: Boolean(workId) && typeof chapter === 'number' && chapter > 0,
  });
}

export function useChapterBody(
  workId: string | undefined,
  chapter: number | undefined,
  query?: ChapterContentQuery,
) {
  const client = useNexusClient();
  return useQuery({
    queryKey: queryKeys.chapters.body(workId ?? '', chapter ?? 0, query),
    queryFn: () => client.getChapterBody(workId!, chapter!, query),
    enabled: Boolean(workId) && typeof chapter === 'number' && chapter > 0,
  });
}

export function usePatchChapter(workId: string | undefined) {
  const client = useNexusClient();
  const qc = useQueryClient();
  const errorToast = useErrorToast();
  return useMutation({
    mutationFn: (vars: { chapter: number; request: PatchChapterRequest; query?: ChapterContentQuery }) =>
      client.patchChapter(workId!, vars.chapter, vars.request, vars.query),
    onSuccess: (_data, vars) => {
      void qc.invalidateQueries({ queryKey: queryKeys.chapters.lists() });
      void qc.invalidateQueries({ queryKey: queryKeys.chapters.detail(workId!, vars.chapter) });
    },
    onError: (error) => errorToast(error, 'error.couldNotUpdateChapter'),
  });
}

/**
 * Resolve the active creator id from client context.
 *
 * V1.94: the footer profile switcher stores the selected creator id in
 * {@link ActiveCreatorProvider} (backed by localStorage / Tauri store). Memory
 * and other creator-scoped queries read that value. When no explicit selection
 * exists yet, callers can fall back to deriving one from sessions or creators.
 */
export function useActiveCreatorId(): string | undefined {
  return useActiveCreatorIdFromContext() ?? undefined;
}

// ── Creators (V1.94 P1) ──────────────────────────────────────────────────────

export function useCreators(query?: ListCreatorsQuery) {
  const client = useNexusClient();
  return useQuery({
    queryKey: queryKeys.creators.list(query),
    queryFn: async (): Promise<ListCreatorsResponse> => client.listCreators(query),
  });
}

export function useCreateCreator() {
  const client = useNexusClient();
  const qc = useQueryClient();
  const errorToast = useErrorToast();
  return useMutation({
    mutationFn: (request: { display_name: string }) => client.createCreator(request),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: queryKeys.creators.all });
    },
    onError: (error) => errorToast(error, 'error.couldNotCreateCreator'),
  });
}

export function useSetActiveCreator() {
  const client = useNexusClient();
  const qc = useQueryClient();
  const errorToast = useErrorToast();
  return useMutation({
    mutationFn: (creatorId: string) => client.setActiveCreator({ creator_id: creatorId }),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: queryKeys.creators.all });
    },
    onError: (error) => errorToast(error, 'error.couldNotSwitchCreator'),
  });
}

// ── Agent host (V1.94 P1) ────────────────────────────────────────────────────

export function useScanAgents(request?: ScanRequest) {
  const client = useNexusClient();
  return useQuery({
    queryKey: queryKeys.agentHost.scan(request),
    queryFn: async (): Promise<ScanResponse> => client.scanAgents(request),
  });
}

/** Saved agent profile snapshot shape (desktop `getAgentProfile` payload). */
export type AgentProfileSnapshot = { name: string; launchCommand?: string };

/**
 * Desktop-only saved agent profile (V1.120 P1 T1).
 *
 * Backed by React Query so the Settings Agent Save handler can invalidate
 * `queryKeys.agentProfile` and the DaemonStatusBar agent badge refreshes
 * immediately after a save — no 10s poll wait (AD-P1-1). Browser build: the
 * hook is disabled (`desktop === null`) and `data` stays `undefined`.
 */
export function useAgentProfile() {
  const desktop = useDesktopCapabilities();
  return useQuery({
    queryKey: queryKeys.agentProfile.detail(),
    queryFn: (): Promise<AgentProfileSnapshot | null> => desktop!.getAgentProfile(),
    enabled: Boolean(desktop),
  });
}

/**
 * V1.108 FB-UI-008 — Verify Agent probe for the custom launch field.
 *
 * Reuses the existing scan endpoint (`POST /v1/daemon/agent-host/scan`) with
 * `filter: 'installed'` and matches the trimmed custom command against
 * installed agents' `launch_command` (locked FB-UI-008 design: match scan
 * result by command string). No wire change. The probe validates commands
 * that resolve to an installed ACP-registry agent; it does not verify
 * arbitrary binaries outside the registry.
 *
 * Matching is resilient to path/arg normalization (PR-review fix): a custom
 * command typed as a full path (or with extra args) still matches an installed
 * agent whose `launch_command` is the short form, and vice versa. See
 * {@link launchCommandMatches} for the exact rules.
 *
 * @returns `true` when an installed agent's `launch_command` matches.
 */
export function useVerifyAgent() {
  const client = useNexusClient();
  // R-V1108P1QC3-W002: cancel the in-flight probe on unmount so a stale scan
  // result does not settle a mutation whose component tree is already gone.
  const abortRef = useRef<AbortController | null>(null);

  useEffect(() => {
    return () => {
      abortRef.current?.abort();
    };
  }, []);

  return useMutation({
    mutationFn: async (command: string): Promise<boolean> => {
      const controller = new AbortController();
      abortRef.current = controller;

      const trimmed = command.trim();
      if (!trimmed) return false;
      const res = await client.scanAgents({ filter: 'installed' });
      // If the component unmounted while the scan was in flight, discard the
      // result rather than settling a mutation on a gone tree.
      if (controller.signal.aborted) {
        throw new DOMException('useVerifyAgent aborted on unmount', 'AbortError');
      }
      return res.agents.some(
        (a) => a.installed && launchCommandMatches(trimmed, a.launch_command),
      );
    },
  });
}

/**
 * PR-review fix — lenient command matching for the Verify Agent probe.
 *
 * Exact string equality produced false negatives when the same binary was
 * expressed differently on the two sides (e.g. the user types a full path
 * `/usr/local/bin/my-agent` while the registry reports the short form
 * `my-agent`, or one side carries trailing args). The rules, in order:
 *
 *   1. Trim both sides.
 *   2. Exact (case-sensitive) equality → match. Preserves the original
 *      FB-UI-008 behavior for the common case.
 *   3. Otherwise, compare the **binary basename** case-insensitively. The
 *      binary is the first whitespace-delimited token (so trailing args like
 *      `my-agent --foo` are ignored), and the basename is its last `/`-separated
 *      segment (so `/usr/local/bin/my-agent` and `my-agent` align). This is the
 *      narrowest normalization that still survives path/arg drift without
 *      admitting substring false positives (e.g. `code` must NOT match `codex`).
 *
 * Pure and side-effect-free so it can be unit-tested directly.
 */
export function launchCommandMatches(
  customCommand: string,
  scanLaunchCommand: string | null | undefined,
): boolean {
  const custom = customCommand.trim();
  const scan = scanLaunchCommand?.trim();
  if (!custom || !scan) return false;
  if (custom === scan) return true;

  const basename = (cmd: string): string => {
    const binary = cmd.split(/\s+/)[0] ?? '';
    const segs = binary.split('/');
    return (segs[segs.length - 1] ?? '').toLowerCase();
  };
  return basename(custom) !== '' && basename(custom) === basename(scan);
}

// ── Creator Memory review-loop (V1.78) ───────────────────────────────────────

/** Pending-review count badge refresh cadence (live count indicator). */
const MEMORY_COUNT_POLL_MS = 10_000;

/** Cursor-paginated pending-review list for the active creator. */
export function usePendingReviews(
  creatorId: string | undefined,
  query?: Omit<ListPendingReviewsQuery, 'creator_id'>,
) {
  const client = useNexusClient();
  const limit = query?.limit ?? DEFAULT_PAGE_SIZE;
  return useInfiniteQuery({
    queryKey: queryKeys.memory.pendingList(creatorId ?? '', { ...query, limit }),
    initialPageParam: FIRST_PAGE,
    queryFn: async ({ pageParam }): Promise<CursorPage<PendingReviewInfo>> => {
      const res = await client.listPendingReviews(creatorId!, { ...query, limit, cursor: pageParam });
      return { items: res.items, pagination: res.pagination };
    },
    enabled: Boolean(creatorId),
    getNextPageParam: (lastPage: CursorPage<PendingReviewInfo>): Cursor =>
      lastPage.pagination.has_more ? lastPage.pagination.next_cursor : undefined,
  });
}

/** Live pending-review count for the header badge (polled). */
export function usePendingReviewCount(creatorId: string | undefined) {
  const client = useNexusClient();
  return useQuery({
    queryKey: queryKeys.memory.count(creatorId ?? ''),
    queryFn: (): Promise<CountPendingReviewsResponse> => client.countPendingReviews(creatorId!),
    enabled: Boolean(creatorId),
    refetchInterval: MEMORY_COUNT_POLL_MS,
    // Intentionally NO `refetchIntervalInBackground: true`: TanStack pauses
    // `refetchInterval` when the tab/window is hidden by default, which keeps
    // this 10s poll from draining battery/CPU on the Tauri desktop shell and
    // backgrounded browser tabs. Do not add `refetchIntervalInBackground` here
    // without a deliberate reason — this is a battery-sensitive surface.
  });
}

/** Read-only fragments list for the active creator (NOT paginated; bounded by `limit`). */
export function useMemoryFragments(
  creatorId: string | undefined,
  query?: Omit<ListMemoryFragmentsQuery, 'creator_id'>,
  options?: { refetchInterval?: number },
) {
  const client = useNexusClient();
  return useQuery({
    queryKey: queryKeys.memory.fragments(creatorId ?? '', query),
    queryFn: () => client.listMemoryFragments(creatorId!, query),
    enabled: Boolean(creatorId),
    refetchInterval: options?.refetchInterval,
    // Intentionally NO `refetchIntervalInBackground: true`: TanStack pauses
    // `refetchInterval` when the tab/window is hidden by default, which keeps a
    // SOUL auto-refresh poll from draining battery/CPU on the Tauri desktop
    // shell and backgrounded browser tabs (matches usePendingReviewCount).
  });
}

/**
 * Delete a pending-review row. Optimistically removes the row from every cached
 * pending-review list for this creator and decrements the count badge before
 * the server responds, rolls back on error, and invalidates pending-list +
 * count + fragments queries on settle.
 */
export function useDeletePendingReview() {
  const client = useNexusClient();
  const qc = useQueryClient();
  const errorToast = useErrorToast();
  const { t } = useTranslation('common');
  const { toast } = useToast();
  type PendingListData = { pages: CursorPage<PendingReviewInfo>[] };
  return useMutation({
    mutationFn: (vars: { pendingId: string; creatorId: string }) =>
      client.deletePendingReview(vars.pendingId, vars.creatorId),
    onMutate: async (vars) => {
      await qc.cancelQueries({ queryKey: queryKeys.memory.pendingList(vars.creatorId) });
      const previousLists = qc.getQueriesData<PendingListData>({
        queryKey: queryKeys.memory.pendingList(vars.creatorId),
      });
      const previousCount = qc.getQueryData<CountPendingReviewsResponse>(
        queryKeys.memory.count(vars.creatorId),
      );
      // Drop the row from every cached list view for this creator.
      qc.setQueriesData<PendingListData>(
        { queryKey: queryKeys.memory.pendingList(vars.creatorId) },
        (old) => {
          if (!old) return old;
          return {
            ...old,
            pages: old.pages.map((page) => ({
              ...page,
              items: page.items.filter((r) => r.pending_id !== vars.pendingId),
            })),
          };
        },
      );
      // Optimistically decrement the count badge (floor at 0).
      if (previousCount && previousCount.count > 0) {
        qc.setQueryData<CountPendingReviewsResponse>(queryKeys.memory.count(vars.creatorId), {
          count: previousCount.count - 1,
        });
      }
      return { previousLists, previousCount };
    },
    onError: (error, vars, context) => {
      if (context?.previousLists) {
        for (const [queryKey, data] of context.previousLists) {
          qc.setQueryData(queryKey, data);
        }
      }
      if (context?.previousCount) {
        qc.setQueryData(queryKeys.memory.count(vars.creatorId), context.previousCount);
      }
      errorToast(error, 'error.couldNotDeletePendingReview');
    },
    onSuccess: (_data, vars) => {
      toast({ variant: 'success', title: t('toast.pendingReviewDeleted'), description: shortId(vars.pendingId) });
    },
    onSettled: (_data, _error, vars) => {
      void qc.invalidateQueries({ queryKey: queryKeys.memory.pendingList(vars.creatorId) });
      void qc.invalidateQueries({ queryKey: queryKeys.memory.count(vars.creatorId) });
      void qc.invalidateQueries({ queryKey: queryKeys.memory.fragments(vars.creatorId) });
    },
  });
}

/**
 * Maximum client-side drain calls per user action (V1.80 REL-01). The daemon
 * processes up to REVIEW_BATCH_LIMIT (50) rows per call; 20 calls × 50 rows =
 * 1,000 rows per user action before the drain stops with a still-draining
 * toast. Guards against pathological queues hanging the UI.
 */
const REVIEW_DRAIN_MAX_CALLS = 20;

/**
 * Trigger the server-side review/summarization pipeline. Surfaces the result
 * counters (`promoted`/`fragmented`/`dropped`) in a confirmation toast, then
 * invalidates pending-list + count + fragments so the post-review state
 * refetches.
 *
 * V1.80 REL-01: the daemon now processes a bounded batch per call and signals
 * `has_more` when the queue was not fully drained. This mutation drains the
 * queue by re-issuing the POST while `has_more === true`, up to
 * `REVIEW_DRAIN_MAX_CALLS` calls, aggregating counters across calls. It also
 * breaks early if a call returns zero progress (`processed === 0`) with
 * `has_more === true` to avoid an infinite tight loop on an unprocessable head
 * row. If the cap or zero-progress guard trips, a non-error "still draining"
 * info toast tells the author to run review again.
 *
 * Processing state is exposed via `isPending` (the caller disables the CTA
 * while in-flight — there is no optimistic body to render because the server
 * classifies the queue).
 */
export function useReviewMemory() {
  const client = useNexusClient();
  const qc = useQueryClient();
  const errorToast = useErrorToast();
  const { t } = useTranslation('common');
  const { toast } = useToast();
  return useMutation({
    mutationFn: async (creatorId: string): Promise<ReviewResponse> => {
      let promoted = 0;
      let fragmented = 0;
      let dropped = 0;
      let processed = 0;
      let hasMore = false;
      for (let call = 0; call < REVIEW_DRAIN_MAX_CALLS; call += 1) {
        const res = await client.reviewMemory({ creator_id: creatorId });
        promoted += res.promoted;
        fragmented += res.fragmented;
        dropped += res.dropped;
        processed += res.processed ?? 0;
        hasMore = res.has_more ?? false;
        // Queue fully drained by the server.
        if (!hasMore) break;
        // Zero-progress guard: a call that inspected no rows but still reports
        // has_more would loop forever on an unprocessable head row.
        if ((res.processed ?? 0) === 0) break;
      }
      return { promoted, fragmented, dropped, has_more: hasMore, processed };
    },
    onSuccess: (data: ReviewResponse) => {
      if (data.has_more) {
        // Cap or zero-progress guard tripped: the queue is still draining but
        // the client stopped re-requesting. Non-error informational message.
        toast({
          variant: 'info',
          title: t('toast.reviewStillDraining'),
          description: t('toast.reviewStillDrainingDescription', {
            processed: data.processed,
            promoted: data.promoted,
            fragmented: data.fragmented,
            dropped: data.dropped,
          }),
        });
      } else {
        toast({
          variant: 'success',
          title: t('toast.reviewComplete'),
          description: t('toast.reviewCompleteDescription', {
            promoted: data.promoted,
            fragmented: data.fragmented,
            dropped: data.dropped,
          }),
        });
      }
    },
    onError: (error) => errorToast(error, 'error.couldNotCompleteReview'),
    // Invalidate on settle (not just success): if the network fails AFTER the
    // server already processed/deleted the pending queue, the client would
    // otherwise keep showing rows that no longer exist until a manual refresh.
    // Matches the useDeletePendingReview onSettled pattern.
    onSettled: (_data, _error, creatorId) => {
      void qc.invalidateQueries({ queryKey: queryKeys.memory.pendingList(creatorId) });
      void qc.invalidateQueries({ queryKey: queryKeys.memory.count(creatorId) });
      void qc.invalidateQueries({ queryKey: queryKeys.memory.fragments(creatorId) });
      // V1.82 auto-refresh: a review produces new fragments, which may flip
      // any narrative scope (Creator-level or per-World) from `current` →
      // `stale` (or lift it over the insufficient-data threshold). Invalidate
      // the whole narrative cache prefix for this creator so every scope
      // re-reads post-review state without a manual reload.
      void qc.invalidateQueries({ queryKey: [...queryKeys.memory.all, 'soul-narrative', creatorId] });
    },
  });
}

// ── Creator SOUL Narrative (V1.81 SP-1 → V1.82 per-World) ────────────────────

/**
 * Auto-refresh cadence for the SOUL surface (V1.81 SP-4). Polled so new
 * fragments captured by a background review surface in the viz + narrative
 * stale-detection without a manual reload. Conservative vs. the 10s
 * pending-review badge: the SOUL surface is a reflection view, not a live
 * action queue, so a slower cadence keeps the Tauri desktop shell light.
 */
export const SOUL_REFETCH_MS = 30_000;

/**
 * Read the workspace-scoped world list for the SOUL selector.
 *
 * `GET /v1/daemon/narrative/worlds` returns every Work-backed world (including
 * zero-fragment worlds) so the selector can surface honest subset-empty states.
 * The list is workspace-scoped in the single-creator local model; P1 does not
 * client-filter by owner. V1.82 mocks the response shape against the generated
 * `World` domain contract until P0 lands the generated list-response type.
 */
export function useNarrativeWorlds(options?: { limit?: number }) {
  const client = useNexusClient();
  const limit = options?.limit;
  return useQuery({
    queryKey: queryKeys.memory.worlds(),
    queryFn: (): Promise<World[]> => client.listNarrativeWorlds(),
    select: (data) => (limit != null ? data.slice(0, limit) : data),
  });
}

/**
 * Read the cached SOUL narrative for the active scope (V1.82).
 *
 * The `/soul/reflect` endpoint is a POST that returns the current cache state
 * when `force_regenerate` is absent — so this is a *read* query shaped as a POST
 * (the contract is one endpoint for read + regenerate; there is no separate
 * GET). It returns one of `ungenerated` / `current` / `stale` /
 * `insufficient_data`, plus the cached narrative text + metadata when present.
 *
 * `worldId` absent/null reads the whole-Creator narrative; a present `worldId`
 * reads that world's per-World narrative. The query key includes `worldId` so
 * the narrative re-fetches when the selector changes and TanStack maintains
 * exactly one active observer per scope (no duplicate poll timers).
 * Auto-refreshes on the SOUL poll cadence + after a review mutation.
 */
export function useSoulNarrative(creatorId: string | undefined, worldId?: string | null) {
  const client = useNexusClient();
  return useQuery({
    queryKey: queryKeys.memory.soulNarrative(creatorId ?? '', worldId),
    queryFn: (): Promise<SoulNarrativeResponse> =>
      client.reflectSoulNarrative({ creator_id: creatorId!, world_id: worldId ?? undefined }),
    enabled: Boolean(creatorId),
    refetchInterval: SOUL_REFETCH_MS,
  });
}

/**
 * Force-regenerate the SOUL narrative for the active scope (V1.82).
 *
 * Fires `force_regenerate: true` on the CTA ("Reflect on my SOUL" /
 * "Re-reflect"). `worldId` absent/null regenerates the whole-Creator narrative;
 * a present `worldId` regenerates that world's per-World narrative. The caller
 * drives the `generating` UX from `isPending`; on settle the matching narrative
 * read query is invalidated so the fresh synthesis replaces the cached text.
 * Errors surface as a toast; the read query is still invalidated on error so a
 * partial/failed regeneration does not leave a frozen stale card.
 */
export function useReflectSoulNarrative() {
  const client = useNexusClient();
  const qc = useQueryClient();
  const errorToast = useErrorToast();
  const { t } = useTranslation('common');
  const { toast } = useToast();
  return useMutation({
    mutationFn: (vars: { creatorId: string; worldId?: string | null }): Promise<SoulNarrativeResponse> =>
      client.reflectSoulNarrative({
        creator_id: vars.creatorId,
        world_id: vars.worldId ?? undefined,
        force_regenerate: true,
      }),
    onSuccess: () => {
      toast({ variant: 'success', title: t('toast.soulReflected'), description: t('toast.soulReflectedDescription') });
    },
    onError: (error) => errorToast(error, 'error.couldNotReflectSoul'),
    onSettled: (_data, _error, vars) => {
      void qc.invalidateQueries({
        queryKey: queryKeys.memory.soulNarrative(vars.creatorId, vars.worldId),
      });
    },
  });
}

// ── Reading progress + annotations (V1.89 Deeper Manuscript Reading) ─────────

const SCROLL_PROGRESS_UNIT = 10_000;

function ratioToScrollProgress(ratio: number): number {
  return Math.max(0, Math.min(SCROLL_PROGRESS_UNIT, Math.round(ratio * SCROLL_PROGRESS_UNIT)));
}

export function useReadingProgress(workId: string | undefined, chapter: number | undefined) {
  const client = useNexusClient();
  return useQuery({
    queryKey: queryKeys.reading.progress(workId ?? '', chapter ?? 0),
    queryFn: (): Promise<ReadingProgressResponse> => client.getReadingProgress(workId!, chapter!),
    enabled: Boolean(workId) && typeof chapter === 'number' && chapter > 0,
  });
}

export function useSaveReadingProgress(options?: { showToast?: boolean }) {
  const client = useNexusClient();
  const qc = useQueryClient();
  const errorToast = useErrorToast();
  const { t } = useTranslation('common');
  const { toast } = useToast();
  return useMutation({
    mutationFn: (vars: { workId: string; chapter: number; scrollProgress: number }) =>
      client.putReadingProgress({
        work_id: vars.workId,
        chapter: vars.chapter,
        scroll_progress: vars.scrollProgress,
      }),
    onSuccess: (_data, vars) => {
      qc.setQueryData(queryKeys.reading.progress(vars.workId, vars.chapter), _data);
      if (options?.showToast) {
        toast({ variant: 'success', title: t('toast.progressSaved') });
      }
    },
    onError: (error) => errorToast(error, 'error.couldNotSaveReadingProgress'),
  });
}

export function useAnnotations(workId: string | undefined, chapter: number | undefined) {
  const client = useNexusClient();
  return useQuery({
    queryKey: queryKeys.reading.annotations(workId ?? '', chapter ?? 0),
    queryFn: async (): Promise<ReadingAnnotation[]> => {
      const res: ReadingAnnotationListResponse = await client.listReadingAnnotations(workId!, chapter!);
      return res.items;
    },
    enabled: Boolean(workId) && typeof chapter === 'number' && chapter > 0,
  });
}

export function useCreateAnnotation() {
  const client = useNexusClient();
  const qc = useQueryClient();
  const errorToast = useErrorToast();
  return useMutation({
    mutationFn: (request: ReadingAnnotationCreateRequest) => client.createReadingAnnotation(request),
    onSuccess: (data: ReadingAnnotation) => {
      void qc.invalidateQueries({ queryKey: queryKeys.reading.annotations(data.work_id, data.chapter) });
    },
    onError: (error) => errorToast(error, 'error.couldNotCreateHighlight'),
  });
}

export function useUpdateAnnotation() {
  const client = useNexusClient();
  const qc = useQueryClient();
  const errorToast = useErrorToast();
  return useMutation({
    mutationFn: (vars: {
      annotationId: string;
      workId: string;
      chapter: number;
      patch: ReadingAnnotationPatchRequest;
    }) => client.patchReadingAnnotation(vars.annotationId, vars.patch),
    onSuccess: (_data, vars) => {
      void qc.invalidateQueries({ queryKey: queryKeys.reading.annotations(vars.workId, vars.chapter) });
    },
    onError: (error) => errorToast(error, 'error.couldNotUpdateHighlight'),
  });
}

export function useDeleteAnnotation() {
  const client = useNexusClient();
  const qc = useQueryClient();
  const errorToast = useErrorToast();
  const { t } = useTranslation('common');
  const { toast } = useToast();
  return useMutation({
    mutationFn: (vars: { annotationId: string; workId: string; chapter: number }) =>
      client.deleteReadingAnnotation(vars.annotationId),
    onSuccess: (_data, vars) => {
      toast({ variant: 'success', title: t('toast.highlightDeleted') });
      void qc.invalidateQueries({ queryKey: queryKeys.reading.annotations(vars.workId, vars.chapter) });
    },
    onError: (error) => errorToast(error, 'error.couldNotDeleteHighlight'),
  });
}

// ── Compute modules (V1.114 P2) ───────────────────────────────────────────────

/** List installed compute modules surfaced by the registry. */
export function useComputeModules() {
  const client = useNexusClient();
  return useQuery({
    queryKey: queryKeys.compute.modules.list(),
    queryFn: async (): Promise<ModuleSummary[]> => {
      const res = await client.getComputeModules();
      return res.items;
    },
  });
}

/** Full manifest detail for a single compute module. */
export function useComputeModule(moduleId: string) {
  const client = useNexusClient();
  return useQuery({
    queryKey: queryKeys.compute.modules.detail(moduleId),
    queryFn: () => client.getComputeModule(moduleId),
    enabled: Boolean(moduleId),
  });
}

// ── Compute runs (V1.147 P1 — Run Studio) ────────────────────────────────────

/** Filter for the runs history list (cursor + page size are hook-managed). */
export type ComputeRunsFilter = Omit<ListRunsQuery, 'cursor' | 'limit'>;

/**
 * Cursor-paginated compute run history (newest-first). Pages map to the shared
 * `CursorPage` shape so `flattenPages` works; the filter is part of the query
 * key so each filter view caches independently.
 */
export function useComputeRuns(filter?: ComputeRunsFilter) {
  const client = useNexusClient();
  const limit = DEFAULT_PAGE_SIZE;
  return useInfiniteQuery({
    queryKey: queryKeys.compute.runs.list({ ...filter, limit }),
    initialPageParam: FIRST_PAGE,
    queryFn: async ({ pageParam }): Promise<CursorPage<RunSummary>> => {
      const res = await client.listRuns({ ...filter, limit, cursor: pageParam });
      return {
        items: res.items,
        pagination: { limit, has_more: res.has_more, next_cursor: res.next_cursor },
      };
    },
    getNextPageParam: (lastPage: CursorPage<RunSummary>): Cursor =>
      lastPage.pagination.has_more ? lastPage.pagination.next_cursor : undefined,
  });
}

/** Full detail for a single run (proposals on succeeded, error on failed). */
export function useComputeRun(runId: string | undefined) {
  const client = useNexusClient();
  return useQuery({
    queryKey: queryKeys.compute.runs.detail(runId ?? ''),
    queryFn: () => client.getRun(runId!),
    enabled: Boolean(runId),
  });
}

/**
 * Invoke a module against an owned World. The World is not mutated by the run
 * itself — proposals are reviewed, then accepted or discarded. On success the
 * runs lists refetch so the new run appears in history without a reload.
 * `status: "failed"` arrives as data (not a thrown error); the caller renders
 * the failure copy from `RunResponse.error`.
 */
export function useRunCompute() {
  const client = useNexusClient();
  const qc = useQueryClient();
  const errorToast = useErrorToast();
  return useMutation({
    mutationFn: (request: RunRequest) => client.runCompute(request),
    onSuccess: (_data, request) => {
      void qc.invalidateQueries({ queryKey: queryKeys.compute.runs.lists() });
      // Keep the module screen fresh — it hosts the Run Studio entry for this
      // module (brief: runs-list + module-detail invalidation).
      void qc.invalidateQueries({ queryKey: queryKeys.compute.modules.detail(request.module_id) });
    },
    onError: (error, request) => {
      // The daemon persists a Failed run row even when the POST surfaces an
      // error envelope — refetch the Runs lists so the row shows immediately
      // (dogfood finding, V1.147 P1 T4). Unknown world / not-owner rejections
      // create no row; an extra refetch is harmless there.
      void qc.invalidateQueries({ queryKey: queryKeys.compute.runs.lists() });
      void qc.invalidateQueries({ queryKey: queryKeys.compute.modules.detail(request.module_id) });
      errorToast(error, 'error.couldNotRunCompute');
    },
  });
}

/**
 * Accept a succeeded run — atomically commits its proposals into the World.
 * Refetches the runs lists + the run detail so the status flip to Applied is
 * reflected everywhere it is cached, and the cross-screen fan-out (qc1 W-001 /
 * qc3 W-1): Accept mutates World + Timeline + KB together, so the Timeline
 * overview and World-KB graph caches are invalidated too (mirroring the
 * `useCreateWorld` / `useDeleteWorld` convention) — the app sets
 * `refetchOnWindowFocus: false`, so without this the post-Accept World state
 * would stay stale until a remount.
 */
export function useAcceptRun() {
  const client = useNexusClient();
  const qc = useQueryClient();
  const errorToast = useErrorToast();
  return useMutation({
    mutationFn: (vars: { runId: string; request?: RunAcceptRequest | null }) =>
      client.acceptRun(vars.runId, vars.request),
    onSuccess: (_data, vars) => {
      void qc.invalidateQueries({ queryKey: queryKeys.compute.runs.lists() });
      void qc.invalidateQueries({ queryKey: queryKeys.compute.runs.detail(vars.runId) });
      void qc.invalidateQueries({ queryKey: queryKeys.timeline.all });
      void qc.invalidateQueries({ queryKey: queryKeys.worldKb.all });
    },
    onError: (error) => errorToast(error, 'error.couldNotAcceptRun'),
  });
}

/**
 * Discard a succeeded run — drops its proposals (destructive; the UI confirms
 * first per the behavior spec). Same invalidation contract as accept,
 * including the Timeline/KB fan-out for symmetry (a discarded run never wrote
 * World state, but the stale-cache window after a previous Accept on the same
 * surface is closed the same way).
 */
export function useDiscardRun() {
  const client = useNexusClient();
  const qc = useQueryClient();
  const errorToast = useErrorToast();
  return useMutation({
    mutationFn: (runId: string) => client.discardRun(runId),
    onSuccess: (_data, runId) => {
      void qc.invalidateQueries({ queryKey: queryKeys.compute.runs.lists() });
      void qc.invalidateQueries({ queryKey: queryKeys.compute.runs.detail(runId) });
      void qc.invalidateQueries({ queryKey: queryKeys.timeline.all });
      void qc.invalidateQueries({ queryKey: queryKeys.worldKb.all });
    },
    onError: (error) => errorToast(error, 'error.couldNotDiscardRun'),
  });
}

// ── World timeline events (V1.147 P2 — compute_result merge) ────────────────

/** Optional filter for the canvas events query (branch scoping only). */
export interface WorldTimelineEventsFilter {
  branch_id?: string;
}

/**
 * Cursor-paginated per-World timeline log events for the Timeline canvas
 * Narrative merge. Hard-filtered to the machine-written `compute_result`
 * family in `canon` state (plan Global Constraints merge discipline; the T1
 * route defaults status to canon anyway). `branch_id` is intentionally NOT
 * sent — the daemon defaults to the World's current branch (root fallback),
 * which is the canvas's existing world-state source. Pages map to the shared
 * `CursorPage` shape so `flattenPages` works.
 *
 * Invalidation: `useAcceptRun` / `useDiscardRun` already invalidate
 * `queryKeys.timeline.all`, which prefix-matches `['timeline','events',…]` —
 * an accepted Run's new compute_result event appears on the canvas without a
 * manual refresh (the canvas stays mounted behind the Settings modal).
 */
export function useWorldTimelineEvents(
  worldId: string | undefined,
  filter?: WorldTimelineEventsFilter,
) {
  const client = useNexusClient();
  const limit = 100;
  return useInfiniteQuery({
    queryKey: queryKeys.timeline.events.list(worldId ?? '', {
      ...filter,
      event_type: 'compute_result',
      status: 'canon',
      limit,
    }),
    initialPageParam: FIRST_PAGE,
    queryFn: async ({ pageParam }): Promise<CursorPage<TimelineEventInfo>> => {
      const res = await client.getTimelineEvents(worldId!, {
        ...filter,
        event_type: 'compute_result',
        status: 'canon',
        limit,
        cursor: pageParam,
      });
      return {
        items: res.items,
        pagination: { limit, has_more: res.has_more, next_cursor: res.next_cursor },
      };
    },
    getNextPageParam: (lastPage: CursorPage<TimelineEventInfo>): Cursor =>
      lastPage.pagination.has_more ? lastPage.pagination.next_cursor : undefined,
    enabled: Boolean(worldId),
    staleTime: 10_000,
  });
}

/**
 * Sync the document scroll position with persisted reading progress.
 *
 * On mount / chapter change, restores the saved scroll ratio once the progress
 * query resolves. While the user reads, debounces scroll events (~500 ms) and
 * persists the ratio. Also flushes on `beforeunload` and `visibilitychange`
 * so the last position is not lost when the tab closes or navigates away.
 */
export function useReadingProgressSync(
  workId: string | undefined,
  chapter: number | undefined,
  options?: { enabled?: boolean; showSavedToast?: boolean },
) {
  const progress = useReadingProgress(workId, chapter);
  const save = useSaveReadingProgress({ showToast: options?.showSavedToast });
  const enabled = options?.enabled ?? true;
  const restoredRef = useRef(false);

  // Reset the restore guard whenever the chapter changes so navigation to a
  // different chapter restores its own position.
  useEffect(() => {
    restoredRef.current = false;
  }, [workId, chapter]);

  const saveMutate = save.mutate;
  const flushSave = useCallback(() => {
    if (!workId || chapter === undefined || chapter <= 0) return;
    const scrollable = document.documentElement.scrollHeight - window.innerHeight;
    const ratio = scrollable > 0 ? window.scrollY / scrollable : 0;
    saveMutate({ workId, chapter, scrollProgress: ratioToScrollProgress(ratio) });
  }, [workId, chapter, saveMutate]);

  // Restore once when the persisted value first resolves. The guard prevents
  // re-scrolling when the save mutation updates the cached scroll_progress.
  useEffect(() => {
    if (!enabled || !progress.isSuccess || restoredRef.current) return;
    if (!progress.data || progress.data.scroll_progress <= 0) {
      restoredRef.current = true;
      return;
    }
    const scrollable = document.documentElement.scrollHeight - window.innerHeight;
    if (scrollable <= 0) {
      restoredRef.current = true;
      return;
    }
    const ratio = progress.data.scroll_progress / SCROLL_PROGRESS_UNIT;
    window.scrollTo({ top: ratio * scrollable });
    restoredRef.current = true;
  }, [enabled, progress.isSuccess, progress.data, workId, chapter]);

  // Debounced scroll save.
  useEffect(() => {
    if (!enabled) return;
    let timer = 0;
    function onScroll() {
      window.clearTimeout(timer);
      timer = window.setTimeout(flushSave, 500);
    }
    window.addEventListener('scroll', onScroll, { passive: true });
    return () => {
      window.removeEventListener('scroll', onScroll);
      window.clearTimeout(timer);
    };
  }, [enabled, flushSave]);

  // Flush on page hide / beforeunload so the last position survives navigation.
  useEffect(() => {
    if (!enabled) return;
    function onBeforeUnload() {
      flushSave();
    }
    function onVisibilityChange() {
      if (document.visibilityState === 'hidden') flushSave();
    }
    window.addEventListener('beforeunload', onBeforeUnload);
    document.addEventListener('visibilitychange', onVisibilityChange);
    return () => {
      window.removeEventListener('beforeunload', onBeforeUnload);
      document.removeEventListener('visibilitychange', onVisibilityChange);
    };
  }, [enabled, flushSave]);

  return { progress, save };
}
