/**
 * Live apply-state read for the compose path (v241 P3.5 T3).
 *
 * `GET /v1/daemon/strategies/{id}/applied` returns the current apply row plus
 * the newest approval. The surface compares the two `approval_id`s — never
 * timestamps — to decide whether the live apply is behind the newest
 * approval. This query is the first mounted reader of
 * `queryKeys.strategies.applied`; a successful apply invalidates that key.
 */
import { useQuery } from '@tanstack/react-query';

import type { StrategyAppliedStatus } from '@42ch/nexus-contracts';

import { useNexusClient } from '@/lib/client-context';
import { queryKeys } from '@/lib/nexus/query-keys';

export function useStrategyApplied(presetId: string, enabled: boolean) {
  const client = useNexusClient();
  return useQuery<StrategyAppliedStatus>({
    queryKey: queryKeys.strategies.applied(presetId),
    queryFn: () => client.getStrategyApplied(presetId),
    enabled,
  });
}
