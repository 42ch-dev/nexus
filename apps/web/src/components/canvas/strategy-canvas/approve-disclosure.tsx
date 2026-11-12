/**
 * Approve disclosure — the simulation limit, rendered where approval happens
 * (`.mstar/research/p2-simulator-limits.md` §3, five required elements).
 *
 * The modal is disclosure-first: the trajectory's `simulation_level`,
 * `stubbed_judges`, `effect_summary` and `predicted_waits` decide the copy.
 * A status-only "simulated" badge is deliberately not renderable here — the
 * five elements either render from the trajectory or the surface is not
 * approval-ready.
 *
 * States are kept apart so none of them reads as approval-ready:
 * `idle` (not simulated), `failed` (simulate refused), `stale` (the candidate
 * changed after the run) and `runtime` share a disabled Approve.
 */
import { useTranslation } from 'react-i18next';

import type { SimulationTrajectory } from '@42ch/nexus-contracts';

import { Dialog, DialogContent } from '@/components/ui/dialog';
import type { SimulationStatus } from '@/lib/canvas/use-simulation-trajectory';

export interface ApproveDisclosureProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  status: SimulationStatus;
  trajectory: SimulationTrajectory | null;
  error: unknown;
  onApprove: () => void;
  /** Re-run the static walk for the current candidate (the stale/failed recovery). */
  onRun?: () => void;
}

export function ApproveDisclosure({
  open,
  onOpenChange,
  status,
  trajectory,
  error,
  onApprove,
  onRun,
}: ApproveDisclosureProps) {
  const { t } = useTranslation('canvas');

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent title={t('strategy.approve.title')}>
        {status === 'idle' ? (
          <p data-testid="approve-not-simulated" className="mt-2 text-copy-13 text-gray-900">
            {t('strategy.approve.notSimulated')}
          </p>
        ) : null}

        {status === 'running' ? (
          <p className="mt-2 text-copy-13 text-gray-900">{t('strategy.approve.running')}</p>
        ) : null}

        {status === 'failed' ? (
          <p
            data-testid="approve-simulate-failed"
            role="alert"
            className="mt-2 rounded-card border border-red-700 px-3 py-2 text-copy-13 text-red-1000"
          >
            {t('strategy.approve.failed', {
              message:
                error instanceof Error
                  ? error.message
                  : String(error ?? ''),
            })}
          </p>
        ) : null}

        {status === 'stale' ? (
          <p
            data-testid="approve-stale"
            role="alert"
            className="mt-2 rounded-card border border-amber-700 px-3 py-2 text-copy-13 text-gray-1000"
          >
            {t('strategy.approve.stale')}
          </p>
        ) : null}

        {trajectory ? (
          <dl className="mt-3 flex flex-col gap-3">
            <div>
              <dt className="text-label-12 text-gray-700">
                {t('strategy.approve.simulationLevel')}
              </dt>
              <dd data-testid="approve-simulation-level" className="text-copy-13 text-gray-1000">
                {trajectory.simulation_level in LEVEL_COPY
                  ? t(LEVEL_COPY[trajectory.simulation_level])
                  : t('strategy.approve.level.unsupported', {
                      level: trajectory.simulation_level,
                    })}
              </dd>
            </div>

            <div>
              <dt className="text-label-12 text-gray-700">
                {t('strategy.approve.stubbedJudges')}
              </dt>
              <dd data-testid="approve-stubbed-judges" className="text-copy-13 text-gray-1000">
                {t('strategy.approve.stubbedJudgesCount', {
                  count: trajectory.stubbed_judges.length,
                })}
                {trajectory.stubbed_judges.length > 0 ? (
                  <details className="mt-1">
                    <summary className="cursor-pointer text-copy-13 text-purple-700">
                      {t('strategy.approve.stubbedJudgesExpand')}
                    </summary>
                    <ul className="mt-1 flex flex-col gap-0.5 font-mono text-label-12 text-gray-900">
                      {trajectory.stubbed_judges.map((id) => (
                        <li key={id}>{id}</li>
                      ))}
                    </ul>
                  </details>
                ) : null}
              </dd>
            </div>

            <div>
              <dt className="text-label-12 text-gray-700">
                {t('strategy.approve.effectSummary')}
              </dt>
              <dd data-testid="approve-effect-summary" className="text-copy-13 text-gray-1000">
                {t('strategy.approve.effectFence', { effect: trajectory.effect_summary })}
              </dd>
            </div>

            <div>
              <dt className="text-label-12 text-gray-700">
                {t('strategy.approve.predictedWaits')}
              </dt>
              <dd data-testid="approve-predicted-waits" className="text-copy-13 text-gray-1000">
                {t('strategy.approve.predictedWaitsLabel', {
                  count: trajectory.predicted_waits.length,
                })}
                {trajectory.predicted_waits.length > 0 ? (
                  <ul className="mt-1 flex flex-col gap-0.5 font-mono text-label-12 text-gray-900">
                    {trajectory.predicted_waits.map((wait) => (
                      <li key={`${wait.reason}:${wait.subject}`}>
                        {wait.reason} · {wait.subject}
                      </li>
                    ))}
                  </ul>
                ) : null}
              </dd>
            </div>
          </dl>
        ) : null}

        <p data-testid="approve-no-output-preview" className="mt-3 text-copy-13 text-gray-900">
          {t('strategy.approve.doesNotPreviewOutput')}
        </p>

        <div className="mt-4 flex justify-end gap-2">
          {onRun && status !== 'ready' && status !== 'running' ? (
            <button
              type="button"
              onClick={onRun}
              className="rounded-control border border-gray-alpha-400 px-3 py-1.5 text-button-12 text-gray-900 hover:bg-gray-alpha-100"
            >
              {t('strategy.approve.rerun')}
            </button>
          ) : null}
          <button
            type="button"
            onClick={onApprove}
            disabled={status !== 'ready'}
            className="rounded-control bg-purple-700 px-3 py-1.5 text-button-12 text-white hover:bg-purple-800 disabled:cursor-not-allowed disabled:opacity-50"
          >
            {t('strategy.approve.approve')}
          </button>
        </div>
      </DialogContent>
    </Dialog>
  );
}

/** `simulation_level` → copy. Unknown levels are surfaced as unsupported, never assumed. */
const LEVEL_COPY: Record<string, string> = {
  reachability: 'strategy.approve.level.reachability',
};
