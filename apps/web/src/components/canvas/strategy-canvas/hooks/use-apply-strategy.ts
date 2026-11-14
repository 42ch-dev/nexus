/**
 * Apply wiring for the compose path (v241 P3.5 T2).
 *
 * `POST /v1/daemon/strategies/{id}/apply` installs one already-approved
 * snapshot as the strategy's running identity. The body names the approval by
 * its content hash only, so a rollback is the same call aimed at a superseded
 * approval's hash — there is no second mechanism.
 *
 * Every refusal carries its typed `reason` in `details`: the code itself is the
 * message, so `already_applied` never renders as a generic failure. Nothing
 * auto-retries; the applied identity comes from the response, never a reload.
 */
import { useCallback, useState } from 'react';
import { useQueryClient } from '@tanstack/react-query';

import type { StrategyAppliedStatus } from '@42ch/nexus-contracts';

import { useNexusClient } from '@/lib/client-context';
import { NexusClientError } from '@/lib/nexus';
import { queryKeys } from '@/lib/nexus/query-keys';

/** One immutable apply row, as the wire returns it. */
export type AppliedStrategy = NonNullable<StrategyAppliedStatus['current_apply']>;

export type ApplyRenderState =
  | { kind: 'pending' }
  | { kind: 'applied'; applied: AppliedStrategy }
  | { kind: 'refused'; message: string };

/** The refusal's own code; only an untyped failure falls back to its message. */
export function applyRefusalMessage(error: NexusClientError): string {
  const details = (error.details ?? {}) as Record<string, unknown>;
  const reason = details.reason;
  return typeof reason === 'string' && reason !== '' ? reason : error.message;
}

export function useApplyStrategy(presetId: string) {
  const client = useNexusClient();
  const queryClient = useQueryClient();
  const [state, setState] = useState<ApplyRenderState | null>(null);

  const apply = useCallback(
    async (approval: { content_hash: string }) => {
      setState({ kind: 'pending' });
      try {
        const applied = await client.applyStrategy(presetId, {
          approval_content_hash: approval.content_hash,
        });
        setState({ kind: 'applied', applied });
        // The status read is the badge's source; an apply supersedes it.
        void queryClient.invalidateQueries({ queryKey: queryKeys.strategies.applied(presetId) });
        return applied;
      } catch (error) {
        if (error instanceof NexusClientError) {
          setState({ kind: 'refused', message: applyRefusalMessage(error) });
        } else {
          setState({
            kind: 'refused',
            message: error instanceof Error ? error.message : String(error),
          });
        }
        throw error;
      }
    },
    [client, presetId, queryClient],
  );

  return {
    application: state,
    apply,
    dismissApplication: useCallback(() => setState(null), []),
  };
}
