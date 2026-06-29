/**
 * World KB canvas data hooks — TanStack Query bindings for the V1.73 P0 slice.
 *
 * Read the entity graph + pending candidates and expose the two patch
 * mutations (`world_kb.patch_entity`, `world_kb.promote_candidate`). Mutations
 * invalidate the graph so the canvas stays fresh after each successful write;
 * callers handle 409 conflicts via {@link isWorldKbConflictError}.
 */
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';

import { useNexusClient } from '@/lib/client-context';
import { NexusClientError } from '@/lib/nexus';
import { queryKeys } from '@/lib/nexus/query-keys';
import { useToast } from '@/lib/use-toast';
import type {
  WorldKbCandidatesResponse,
  WorldKbGraphResponse,
  WorldKbPatchEntityRequest,
  WorldKbPatchEntityResponse,
  WorldKbPromoteCandidateRequest,
  WorldKbPromoteCandidateResponse,
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

/**
 * Structured detail carried inside the canonical ErrorResponse `details` field
 * when a World KB patch is rejected because `expected_version` is stale (409).
 */
export interface WorldKbConflictDetails {
  current_version: number;
  entity_id: string;
  conflicting_path: string;
  recovery_hint: string;
}

/** Read the entity graph projection (entities + source-anchor provenance). */
export function useWorldKbGraph(worldId: string | undefined) {
  const client = useNexusClient();
  return useQuery({
    queryKey: queryKeys.worldKb.graph(worldId ?? ''),
    queryFn: async () => client.getWorldKbGraph(worldId!),
    enabled: Boolean(worldId),
    staleTime: 5_000,
  });
}

/** Read pending promotion candidates (cursor-paginated). */
export function useWorldKbCandidates(worldId: string | undefined) {
  const client = useNexusClient();
  return useQuery({
    queryKey: queryKeys.worldKb.candidates(worldId ?? ''),
    queryFn: async () => client.getWorldKbCandidates(worldId!),
    enabled: Boolean(worldId),
    staleTime: 5_000,
  });
}

/** `POST .../kb/patch-entity` — entity title/body/aliases/block_type edit (per-row OCC). */
export function usePatchWorldKbEntity(worldId: string | undefined) {
  const client = useNexusClient();
  const qc = useQueryClient();
  const errorToast = useErrorToast();
  return useMutation({
    mutationFn: (request: WorldKbPatchEntityRequest) =>
      client.worldKbPatchEntity(worldId!, request),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: queryKeys.worldKb.graph(worldId ?? '') });
    },
    onError: (error) => {
      // 409 conflicts → inspector renders the conflict modal; 422 validation →
      // inspector renders inline field errors. Mirror the promotion path's
      // non-conflict guard (extended to also skip 422 validation) so everything
      // else (500/403/dropped network) is surfaced as a toast instead of being
      // silently swallowed. Both guards check `instanceof NexusClientError`, so
      // the real client errors (which ARE instances) are excluded; the per-call
      // handler still owns the 409/422 UX paths.
      if (!isWorldKbConflictError(error) && !isWorldKbValidationError(error)) {
        errorToast(error, 'Could not save entity');
      }
    },
  });
}

/** `POST .../kb/promote-candidate` — adopt/reject/merge a pending candidate. */
export function usePromoteWorldKbCandidate(worldId: string | undefined) {
  const client = useNexusClient();
  const qc = useQueryClient();
  const errorToast = useErrorToast();
  return useMutation({
    mutationFn: (request: WorldKbPromoteCandidateRequest) =>
      client.worldKbPromoteCandidate(worldId!, request),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: queryKeys.worldKb.all });
    },
    onError: (error) => {
      // 409 conflicts are resolved by the promotion inspector's conflict modal;
      // only surface non-conflict failures as toasts.
      if (!isWorldKbConflictError(error)) {
        errorToast(error, 'Could not promote candidate');
      }
    },
  });
}

/**
 * Type guard for the World KB 409 conflict error.
 *
 * The daemon returns `error.code === 'world_kb_conflict'` with details carrying
 * the canonical per-row version; the canvas uses this to open the conflict modal.
 */
export function isWorldKbConflictError(
  error: unknown,
): error is NexusClientError & { status: 409; code: 'world_kb_conflict'; details: WorldKbConflictDetails } {
  return error instanceof NexusClientError && error.status === 409 && error.code === 'world_kb_conflict';
}

/**
 * Type guard for the World KB 422 validation error.
 *
 * The daemon returns `error.code === 'world_kb_validation_failed'` with details
 * carrying `validation_summary`; the entity inspector renders these inline.
 */
export function isWorldKbValidationError(
  error: unknown,
): error is NexusClientError & { status: 422; code: 'world_kb_validation_failed' } {
  return (
    error instanceof NexusClientError &&
    error.status === 422 &&
    error.code === 'world_kb_validation_failed'
  );
}

export type {
  WorldKbCandidatesResponse,
  WorldKbGraphResponse,
  WorldKbPatchEntityRequest,
  WorldKbPatchEntityResponse,
  WorldKbPromoteCandidateRequest,
  WorldKbPromoteCandidateResponse,
};
