/**
 * Outline+Timeline canvas data hooks — TanStack Query bindings for V1.72 P0.
 *
 * Read the canonical `WorkOutline` via `getWorkOutline` and expose the three
 * patch mutations. Mutations invalidate the outline detail so the canvas stays
 * fresh after each successful write; callers handle 409 conflicts via the
 * returned error shape.
 */
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';

import { useNexusClient } from '@/lib/client-context';
import { NexusClientError } from '@/lib/nexus';
import { queryKeys } from '@/lib/nexus/query-keys';
import { useToast } from '@/lib/use-toast';
import type {
  OutlinePatchChapterRequest,
  OutlinePatchResponse,
  OutlinePatchStructureRequest,
  TimelinePatchEventRequest,
} from '@42ch/nexus-contracts';

function useErrorToast() {
  const { toast } = useToast();
  return (error: unknown, title: string) => {
    const description =
      error instanceof NexusClientError
        ? error.message
        : error instanceof Error
          ? error.message
          : 'Unexpected error.';
    toast({ variant: 'error', title, description });
  };
}

/** Read the work-level outline + timeline. */
export function useWorkOutline(workId: string | undefined) {
  const client = useNexusClient();
  return useQuery({
    queryKey: queryKeys.outline.detail(workId ?? ''),
    queryFn: async () => client.getWorkOutline(workId!),
    enabled: Boolean(workId),
    staleTime: 5_000,
  });
}

/** `POST /v1/local/works/{work_id}/outline/patch` — structure/volume edits. */
export function usePatchOutlineStructure(workId: string | undefined) {
  const client = useNexusClient();
  const qc = useQueryClient();
  const errorToast = useErrorToast();
  return useMutation({
    mutationFn: (request: OutlinePatchStructureRequest) =>
      client.patchOutlineStructure(workId!, request),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: queryKeys.outline.detail(workId ?? '') });
      void qc.invalidateQueries({ queryKey: queryKeys.chapters.lists() });
    },
    onError: (error) => errorToast(error, 'Could not update outline structure'),
  });
}

/** `POST /v1/local/works/{work_id}/chapters/{n}/patch` — chapter metadata. */
export function usePatchOutlineChapter(workId: string | undefined) {
  const client = useNexusClient();
  const qc = useQueryClient();
  const errorToast = useErrorToast();
  return useMutation({
    mutationFn: ({ chapter, request }: { chapter: number; request: OutlinePatchChapterRequest }) =>
      client.patchOutlineChapter(workId!, chapter, request),
    onSuccess: (_data, variables) => {
      void qc.invalidateQueries({ queryKey: queryKeys.outline.detail(workId ?? '') });
      void qc.invalidateQueries({ queryKey: queryKeys.chapters.lists() });
      void qc.invalidateQueries({
        queryKey: queryKeys.chapters.detail(workId ?? '', variables.chapter),
      });
      // Invalidate the chapter's outline read so the content editor's
      // useChapterOutline cache refetches after a content patch. Without this,
      // the stale cache reverts the editor to the pre-save content once the
      // local save state settles to 'clean'. Use the chapter-specific prefix
      // (no trailing volume-query object) so it matches regardless of the
      // volume query the editor read with.
      void qc.invalidateQueries({
        queryKey: [...queryKeys.chapters.outlines(), workId ?? '', variables.chapter],
      });
    },
    onError: (error) => errorToast(error, 'Could not update chapter'),
  });
}

/** `POST /v1/local/works/{work_id}/timeline/patch` — timeline event edits. */
export function usePatchTimelineEvent(workId: string | undefined) {
  const client = useNexusClient();
  const qc = useQueryClient();
  const errorToast = useErrorToast();
  return useMutation({
    mutationFn: (request: TimelinePatchEventRequest) =>
      client.patchTimelineEvent(workId!, request),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: queryKeys.outline.detail(workId ?? '') });
    },
    onError: (error) => errorToast(error, 'Could not update timeline'),
  });
}

/**
 * Type guard for the outline-specific 409 conflict error.
 *
 * The daemon returns `error.code === 'outline_conflict'` with details carrying
 * the current server revision; the canvas uses this to offer refetch/reapply.
 */
export function isOutlineConflictError(
  error: unknown,
): error is NexusClientError & { status: 409; code: 'outline_conflict' } {
  return error instanceof NexusClientError && error.status === 409 && error.code === 'outline_conflict';
}

export type { OutlinePatchResponse };
