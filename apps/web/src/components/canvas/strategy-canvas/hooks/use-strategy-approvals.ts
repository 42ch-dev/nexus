/**
 * Approval-list read for the compose path (v241 P3.5 T4).
 *
 * `GET /v1/daemon/strategies/{id}/approvals` returns every approval newest first — the same
 * total order the live status read uses for `newest_approval`. The status read alone reports
 * only the newest approval, so it cannot name a superseded one; this list is what lets the
 * Apply control be aimed at an older approval, which is the rollback path.
 *
 * A successful approve invalidates this key (the approve hook) so the newly frozen snapshot
 * appears as the first target without a reload.
 */
import { useQuery } from '@tanstack/react-query';

import type { StrategyApprovals } from '@42ch/nexus-contracts';

import { useNexusClient } from '@/lib/client-context';
import { queryKeys } from '@/lib/nexus/query-keys';

export function useStrategyApprovals(presetId: string, enabled: boolean) {
  const client = useNexusClient();
  return useQuery<StrategyApprovals>({
    queryKey: queryKeys.strategies.approvals(presetId),
    queryFn: () => client.listStrategyApprovals(presetId),
    enabled,
  });
}
