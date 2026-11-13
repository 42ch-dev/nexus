/**
 * Approve wiring for the compose path (v239-p3-canvas-compose-ui T6).
 *
 * `POST /v1/daemon/strategies/{id}/approve` freezes the simulated candidate
 * into an immutable snapshot — it does not publish, apply, schedule or
 * execute. The real route replaces the disabled fallback ONLY on the compose
 * path; the flag-off surface keeps the shipped reason-bearing disabled
 * disclosure (T7).
 *
 * Every refusal keeps its named recovery copy instead of a generic error:
 * `slot_unbound` names the slot and pattern, `pin_changed` names both hashes,
 * `strategy_already_approved` (409) names the snapshot that already froze the
 * content. Nothing auto-retries.
 */
import { useCallback, useState } from 'react';

import type { ApprovedStrategySnapshot, SimulationTrajectory } from '@42ch/nexus-contracts';

import { useNexusClient } from '@/lib/client-context';
import { i18n } from '@/lib/i18n/config';
import { NexusClientError } from '@/lib/nexus';

export type ApproveRenderState =
  | { kind: 'pending' }
  | { kind: 'approved'; snapshot: ApprovedStrategySnapshot }
  | { kind: 'refused'; message: string };

/** UI copy for a named 422 refusal; unknown shapes fall back to the wire message. */
export function approveRefusalMessage(error: NexusClientError): string {
  const details = (error.details ?? {}) as Record<string, unknown>;
  const str = (value: unknown) => (typeof value === 'string' ? value : '');
  switch (details.reason) {
    case 'slot_unbound':
      return i18n.t('strategy.approve.slotUnbound', {
        ns: 'canvas',
        slot: str(details.slot),
        pattern: str(details.package_id),
      });
    case 'pin_changed':
      return i18n.t('strategy.approve.pinChanged', {
        ns: 'canvas',
        id: str(details.package_id),
        version: str(details.package_version),
        recorded: str(details.recorded_content_hash),
        current: str(details.scanned_content_hash),
      });
    default:
      return error.message;
  }
}

export function useApproveStrategy(presetId: string) {
  const client = useNexusClient();
  const [state, setState] = useState<ApproveRenderState | null>(null);

  const approve = useCallback(
    async (args: { baseRevision: number; trajectory: SimulationTrajectory | null }) => {
      setState({ kind: 'pending' });
      try {
        const snapshot = await client.approveStrategy(presetId, {
          base_revision: args.baseRevision,
          trajectory: args.trajectory,
        });
        setState({ kind: 'approved', snapshot });
        return snapshot;
      } catch (error) {
        if (error instanceof NexusClientError) {
          setState({
            kind: 'refused',
            message:
              error.status === 422 ? approveRefusalMessage(error) : error.message,
          });
        } else {
          setState({
            kind: 'refused',
            message: error instanceof Error ? error.message : String(error),
          });
        }
        throw error;
      }
    },
    [client, presetId],
  );

  return { approval: state, approve, dismissApproval: useCallback(() => setState(null), []) };
}
