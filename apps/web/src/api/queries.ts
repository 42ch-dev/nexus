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
import {
  useInfiniteQuery,
  useMutation,
  useQuery,
  useQueryClient,
} from '@tanstack/react-query';
import type {
  ChapterContentQuery,
  ChapterSummary,
  CountPendingReviewsResponse,
  CreateWorkRequest,
  FindingDetailResponse,
  ListCapabilitiesQuery,
  ListChaptersQuery,
  ListFindingsQuery,
  ListMemoryFragmentsQuery,
  ListPendingReviewsQuery,
  ListSchedulesQuery,
  ListSessionsQuery,
  ListWorksQuery,
  PaginationInfo,
  PatchChapterRequest,
  PatchWorkRequest,
  PendingReviewInfo,
  PresetSummary,
  ReviewResponse,
  ScaffoldPresetRequest,
  UpdateFindingRequest,
  ValidatePresetRequest,
  WorkSummary,
} from '@42ch/nexus-contracts';

import { useToast } from '@/lib/use-toast';
import { useNexusClient } from '@/lib/client-context';
import { NexusClientError } from '@/lib/nexus';
import { shortId } from '@/lib/format';
import { queryKeys } from '@/lib/nexus/query-keys';

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

export function useSessions(query?: ListSessionsQuery) {
  const client = useNexusClient();
  return useQuery({
    queryKey: queryKeys.sessions.list(query),
    queryFn: async () => {
      const res = await client.listSessions(query);
      return res.items;
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

// ── Mutations (Setup writes) ─────────────────────────────────────────────────

/** Surface a NexusClientError as a toast; callers may still read the result. */
function useErrorToast() {
  const { toast } = useToast();
  return (error: unknown, fallbackTitle: string) => {
    const description =
      error instanceof NexusClientError
        ? error.message
        : error instanceof Error
          ? error.message
          : 'Unexpected error.';
    toast({ variant: 'error', title: fallbackTitle, description });
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
    onError: (error) => errorToast(error, 'Could not create Work'),
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
    onError: (error) => errorToast(error, 'Could not update Work'),
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
      errorToast(error, 'Could not update finding');
    },
    onSuccess: (_data, vars) => {
      toast({ variant: 'success', title: 'Finding updated', description: shortId(vars.findingId) });
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

export function useScaffoldPreset() {
  const client = useNexusClient();
  const qc = useQueryClient();
  const errorToast = useErrorToast();
  return useMutation({
    mutationFn: (request: ScaffoldPresetRequest) => client.scaffoldPreset(request),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: queryKeys.presets.list() });
    },
    onError: (error) => errorToast(error, 'Could not scaffold preset'),
  });
}

export function useValidatePreset() {
  const client = useNexusClient();
  const errorToast = useErrorToast();
  return useMutation({
    mutationFn: (request: ValidatePresetRequest) => client.validatePreset(request),
    onError: (error) => errorToast(error, 'Could not validate preset'),
    // On success the caller surfaces structured errors/warnings inline; a toast
    // is not added here so the validate dialog stays the single source of truth.
  });
}

export function useReloadPreset() {
  const client = useNexusClient();
  const qc = useQueryClient();
  const errorToast = useErrorToast();
  const { toast } = useToast();
  return useMutation({
    mutationFn: (presetId: string) => client.reloadPreset(presetId),
    onSuccess: (_data, presetId) => {
      toast({ variant: 'success', title: 'Preset reloaded', description: presetId });
      void qc.invalidateQueries({ queryKey: queryKeys.presets.list() });
    },
    onError: (error) => errorToast(error, 'Could not reload preset'),
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
    onError: (error) => errorToast(error, 'Could not update chapter'),
  });
}

// ── Creator Memory review-loop (V1.78) ───────────────────────────────────────

/**
 * Resolve the active creator id from the most recent session/schedule. The
 * daemon model is single-active-creator (config.toml); every memory endpoint
 * rejects a `creator_id` that does not match the active creator with 403. There
 * is no dedicated active-creator accessor in the client surface today, so the
 * Memory page mirrors the canvas's `useDerivedCreatorId` derivation
 * (`apps/web/src/lib/canvas/use-strategy-data.ts:76`) — it reads the creator_id
 * off existing sessions/schedules, which are themselves creator-scoped. Returns
 * `undefined` until sessions load (the page gates memory calls on a defined id).
 *
 * Compass Phase 2b open item #1 (`creator_id` UI source): this is the chosen
 * wiring. A first-class active-creator endpoint/context is a future surface.
 */
export function useActiveCreatorId(): string | undefined {
  const client = useNexusClient();
  const sessions = useQuery({
    // Borrow the sessions list key (single page) — do not introduce a parallel
    // creator query; the derivation is a projection over existing data.
    queryKey: [...queryKeys.sessions.all, 'for-creator-derivation'],
    queryFn: async () => {
      const res = await client.listSessions({ limit: 1 });
      return res.items;
    },
  });
  return sessions.data?.[0]?.creator_id;
}

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
) {
  const client = useNexusClient();
  return useQuery({
    queryKey: queryKeys.memory.fragments(creatorId ?? '', query),
    queryFn: () => client.listMemoryFragments(creatorId!, query),
    enabled: Boolean(creatorId),
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
      errorToast(error, 'Could not delete pending review');
    },
    onSuccess: (_data, vars) => {
      toast({ variant: 'success', title: 'Pending review deleted', description: shortId(vars.pendingId) });
    },
    onSettled: (_data, _error, vars) => {
      void qc.invalidateQueries({ queryKey: queryKeys.memory.pendingList(vars.creatorId) });
      void qc.invalidateQueries({ queryKey: queryKeys.memory.count(vars.creatorId) });
      void qc.invalidateQueries({ queryKey: queryKeys.memory.fragments(vars.creatorId) });
    },
  });
}

/**
 * Trigger the server-side review/summarization pipeline. Surfaces the result
 * counters (`promoted`/`fragmented`/`dropped`) in a confirmation toast, then
 * invalidates pending-list + count + fragments so the post-review state
 * refetches. Processing state is exposed via `isPending` (the caller disables
 * the CTA while in-flight — there is no optimistic body to render because the
 * server classifies the whole queue atomically).
 */
export function useReviewMemory() {
  const client = useNexusClient();
  const qc = useQueryClient();
  const errorToast = useErrorToast();
  const { toast } = useToast();
  return useMutation({
    mutationFn: (creatorId: string) => client.reviewMemory({ creator_id: creatorId }),
    onSuccess: (data: ReviewResponse, creatorId) => {
      toast({
        variant: 'success',
        title: 'Review complete',
        description: `${data.promoted} promoted to long-term memory, ${data.fragmented} saved as fragments, ${data.dropped} dropped.`,
      });
      void qc.invalidateQueries({ queryKey: queryKeys.memory.pendingList(creatorId) });
      void qc.invalidateQueries({ queryKey: queryKeys.memory.count(creatorId) });
      void qc.invalidateQueries({ queryKey: queryKeys.memory.fragments(creatorId) });
    },
    onError: (error) => errorToast(error, 'Could not complete review'),
  });
}
