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
  CreateWorkRequest,
  FindingDetailResponse,
  ListCapabilitiesQuery,
  ListChaptersQuery,
  ListFindingsQuery,
  ListSchedulesQuery,
  ListSessionsQuery,
  ListWorksQuery,
  PaginationInfo,
  PatchChapterRequest,
  PatchWorkRequest,
  PresetSummary,
  PutChapterOutlineRequest,
  ScaffoldPresetRequest,
  ValidatePresetRequest,
  WorkSummary,
  WorldKbPatchEntityRequest,
  WorldKbPatchRelationshipRequest,
} from '@42ch/nexus-contracts';

import { useToast } from '@/lib/use-toast';
import { useNexusClient } from '@/lib/client-context';
import { NexusClientError } from '@/lib/nexus';
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

// ── World KB canvas (V1.73/V1.74 Track A) ───────────────────────────────────

export function useWorldKbGraph(worldId: string | undefined) {
  const client = useNexusClient();
  return useQuery({
    queryKey: queryKeys.worldKb.graph(worldId ?? ''),
    queryFn: () => client.getWorldKbGraph(worldId!),
    enabled: Boolean(worldId),
  });
}

export function useWorldKbPatchEntity(worldId: string | undefined) {
  const client = useNexusClient();
  const qc = useQueryClient();
  const errorToast = useErrorToast();
  return useMutation({
    mutationFn: (request: WorldKbPatchEntityRequest) =>
      client.worldKbPatchEntity(worldId!, request),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: queryKeys.worldKb.graph(worldId!) });
      void qc.invalidateQueries({ queryKey: queryKeys.worldKb.candidates(worldId!) });
    },
    onError: (error) => errorToast(error, 'Could not update entity'),
  });
}

export function useWorldKbPatchRelationship(worldId: string | undefined) {
  const client = useNexusClient();
  const qc = useQueryClient();
  const errorToast = useErrorToast();
  return useMutation({
    mutationFn: (request: WorldKbPatchRelationshipRequest) =>
      client.worldKbPatchRelationship(worldId!, request),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: queryKeys.worldKb.graph(worldId!) });
    },
    onError: (error) => errorToast(error, 'Could not update relationship'),
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

export function usePutChapterOutline(
  workId: string | undefined,
  chapter: number | undefined,
  query?: ChapterContentQuery,
) {
  const client = useNexusClient();
  const qc = useQueryClient();
  const errorToast = useErrorToast();
  return useMutation({
    mutationFn: (request: PutChapterOutlineRequest) =>
      client.putChapterOutline(workId!, chapter!, request, query),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: queryKeys.chapters.outlines() });
      void qc.invalidateQueries({ queryKey: queryKeys.chapters.lists() });
    },
    onError: (error) => errorToast(error, 'Could not save outline'),
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
