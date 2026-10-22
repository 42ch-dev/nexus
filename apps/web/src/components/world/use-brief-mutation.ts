/**
 * Brief save mutation (plan v1219-world-module T2) — the create-on-absent
 * `patch-entity` write for the era KnowledgeEntry that backs the Brief layer
 * (`POST .../kb/patch-entity`, architect-confirmed contract):
 *
 *   - Create: the client mints `entity_id` (`kb_<uuid>`), sends
 *     `expected_version: 0`, and patches
 *     `{ title, block_type: 'era', body: { attributes: { world_summary } } }`.
 *   - Edit: the same route with the observed entity `version` as
 *     `expected_version` (per-row OCC).
 *   - 409 `world_kb_conflict` on create = minted-id collision → remint and
 *     retry exactly once.
 *
 * The World KB graph query is invalidated on settle so the saved card renders
 * without a reload. Errors propagate to the caller untouched — the editor
 * owns the inline failure copy and the draft (never touched here).
 */
import { useMutation, useQueryClient } from '@tanstack/react-query';

import { useNexusClient } from '@/lib/client-context';
import { isWorldKbConflictError } from '@/lib/canvas/use-world-kb-data';
import { queryKeys } from '@/lib/nexus/query-keys';
import type { NexusClient } from '@/lib/nexus';
import type { WorldKbPatchEntityResponse } from '@42ch/nexus-contracts';

export interface SaveBriefInput {
  text: string;
  /** Observed entity for an edit; omit to create. */
  entity?: { entityId: string; version: number };
}

/** Mint a fresh World KB entity id (mirrors the pack-import convention). */
export function mintBriefEntityId(): string {
  return `kb_${crypto.randomUUID().replaceAll('-', '')}`;
}

function patchBrief(
  client: NexusClient,
  worldId: string,
  entityId: string,
  expectedVersion: number,
  text: string,
): Promise<WorldKbPatchEntityResponse> {
  return client.worldKbPatchEntity(worldId, {
    entity_id: entityId,
    expected_version: expectedVersion,
    patch: {
      title: text,
      body: { attributes: { world_summary: text } },
      block_type: 'era',
    },
  });
}

export function useSaveBrief(worldId: string) {
  const client = useNexusClient();
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async ({ text, entity }: SaveBriefInput): Promise<WorldKbPatchEntityResponse> => {
      if (entity) {
        return patchBrief(client, worldId, entity.entityId, entity.version, text);
      }
      try {
        return await patchBrief(client, worldId, mintBriefEntityId(), 0, text);
      } catch (error) {
        // 409 on create = the minted id already exists — remint, retry once.
        if (!isWorldKbConflictError(error)) throw error;
        return patchBrief(client, worldId, mintBriefEntityId(), 0, text);
      }
    },
    onSettled: () => {
      void qc.invalidateQueries({ queryKey: queryKeys.worldKb.graph(worldId) });
    },
  });
}
