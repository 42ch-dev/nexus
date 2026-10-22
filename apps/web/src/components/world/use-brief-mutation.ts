/**
 * Brief save mutation (plan v1219-world-module T2, QC fix wave) — the
 * create-on-absent `patch-entity` write for the era KnowledgeEntry that backs
 * the Brief layer (`POST .../kb/patch-entity`, architect-confirmed contract):
 *
 *   - Create: the client mints `entity_id` (`kb_<uuid>`), sends
 *     `expected_version: 0`, and patches
 *     `{ title, block_type: 'era', body: { attributes: { world_summary } } }`.
 *   - Edit: the same route with the observed entity `version` as
 *     `expected_version` (per-row OCC). The patch merges `world_summary` into
 *     the EXISTING body and does not touch the title — unrelated attributes
 *     on the era survive an edit (QC critical: body replacement data loss).
 *   - 409 `world_kb_conflict` on create = minted-id collision → remint and
 *     retry exactly once. On edit it means a concurrent write bumped the
 *     revision: refresh the graph (so the mounted editor observes the fresh
 *     version) and rethrow — the editor copy promises 可直接重试.
 *
 * The World KB graph query is invalidated inside `onSuccess` (returned
 * promise is awaited by `mutateAsync`), so by the time the editor leaves
 * edit mode the eras prop already contains the saved entity — the saved
 * card renders without a reload and without an empty-state flash. The Work
 * unlock also happens in `onSuccess`, BEFORE the awaited invalidation: a tab
 * close in the refetch window must not leave Work locked after a saved
 * Brief. The first-transition gate (`!isUnlocked`) keeps edit re-saves
 * silent (D8/D17). Errors propagate to the caller untouched — the editor
 * owns the inline failure copy and the draft (never touched here).
 */
import { useMutation, useQueryClient } from '@tanstack/react-query';

import { isUnlocked, unlock } from '@/lib/activation-store';
import { useActiveCreatorId } from '@/lib/active-creator-context';
import { useNexusClient } from '@/lib/client-context';
import { isWorldKbConflictError } from '@/lib/canvas/use-world-kb-data';
import { queryKeys } from '@/lib/nexus/query-keys';
import type { NexusClient } from '@/lib/nexus';
import type { WorldKbPatchEntityResponse } from '@42ch/nexus-contracts';

export interface SaveBriefInput {
  text: string;
  /** Observed entity for an edit; omit to create. */
  entity?: { entityId: string; version: number; body?: Record<string, unknown> };
}

export interface UseSaveBriefOptions {
  /**
   * Fired when this save performs the first-transition Work unlock — the
   * layer renders the announce UI (`世界已就绪` card + toast) here.
   */
  onFirstUnlock?: () => void;
}

/** Mint a fresh World KB entity id (mirrors the pack-import convention). */
export function mintBriefEntityId(): string {
  return `kb_${crypto.randomUUID().replaceAll('-', '')}`;
}

function patchCreate(
  client: NexusClient,
  worldId: string,
  entityId: string,
  text: string,
): Promise<WorldKbPatchEntityResponse> {
  return client.worldKbPatchEntity(worldId, {
    entity_id: entityId,
    expected_version: 0,
    patch: {
      title: text,
      block_type: 'era',
      body: { attributes: { world_summary: text } },
    },
  });
}

function patchEdit(
  client: NexusClient,
  worldId: string,
  entity: NonNullable<SaveBriefInput['entity']>,
  text: string,
): Promise<WorldKbPatchEntityResponse> {
  const existingBody = entity.body ?? {};
  const existingAttrs =
    existingBody.attributes && typeof existingBody.attributes === 'object'
      ? (existingBody.attributes as Record<string, unknown>)
      : {};
  return client.worldKbPatchEntity(worldId, {
    entity_id: entity.entityId,
    expected_version: entity.version,
    // Body is a REPLACEMENT on the wire — merge, never overwrite: keep every
    // existing top-level key and attribute, upsert only `world_summary`.
    // The canonical title stays untouched on edit.
    patch: {
      body: { ...existingBody, attributes: { ...existingAttrs, world_summary: text } },
    },
  });
}

export function useSaveBrief(worldId: string, opts: UseSaveBriefOptions = {}) {
  const client = useNexusClient();
  const qc = useQueryClient();
  const creatorId = useActiveCreatorId();
  return useMutation({
    mutationFn: async ({ text, entity }: SaveBriefInput): Promise<WorldKbPatchEntityResponse> => {
      if (entity) {
        try {
          return await patchEdit(client, worldId, entity, text);
        } catch (error) {
          // 409 on edit = concurrent write — refresh the graph so the mounted
          // editor picks up the fresh revision, then rethrow for the inline
          // retry copy.
          if (isWorldKbConflictError(error)) {
            await qc.invalidateQueries({ queryKey: queryKeys.worldKb.graph(worldId) });
          }
          throw error;
        }
      }
      try {
        return await patchCreate(client, worldId, mintBriefEntityId(), text);
      } catch (error) {
        // 409 on create = the minted id already exists — remint, retry once.
        if (!isWorldKbConflictError(error)) throw error;
        return patchCreate(client, worldId, mintBriefEntityId(), text);
      }
    },
    onSuccess: async () => {
      // Unlock at patch settle — BEFORE the awaited invalidation — so the
      // activation survives a tab close in the refetch window. The
      // first-transition gate keeps edit re-saves silent (D8/D17); any save
      // can be the first transition (an era may predate the Brief).
      if (creatorId && !isUnlocked(creatorId, 'work')) {
        unlock(creatorId, 'work');
        opts.onFirstUnlock?.();
      }
      // Awaited by mutateAsync: the caller can leave edit mode immediately
      // after save without an empty-state flash.
      await qc.invalidateQueries({ queryKey: queryKeys.worldKb.graph(worldId) });
    },
  });
}
