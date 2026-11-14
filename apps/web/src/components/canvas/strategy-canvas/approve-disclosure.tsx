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
import type { ApproveRenderState } from './hooks/use-approve-strategy';
import type { ApplyRenderState } from './hooks/use-apply-strategy';

export interface ApproveDisclosureProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** Authoritative surface state: `'stale'` alone renders the stale banner. */
  status: SimulationStatus;
  /**
   * The finer staleness fact `status` cannot carry: a stale snapshot whose
   * replacement walk reports `running` (or whose re-read reports `failed`)
   * still has to show the banner. Optional — a caller that passes only
   * `status` is complete, and the banner follows `status === 'stale'`.
   */
  isStale?: boolean;
  trajectory: SimulationTrajectory | null;
  error: unknown;
  /**
   * The approval write path. Absent in builds that do not wire it (tracked as
   * I-000415): Approve then renders disabled with the reason, never as a
   * clickable no-op. Only the compose path supplies this (T6); the flag-off
   * surface keeps the disabled disclosure.
   */
  onApprove?: () => void;
  /**
   * Compose-path approve outcome: the frozen snapshot, or a named refusal.
   * Absent until the wired handler ran. The approval summary states that
   * approval freezes a candidate and does not publish, apply or execute.
   */
  approval?: ApproveRenderState | null;
  /**
   * Compose-path gate that disables Approve with its named reason even when a
   * current successful trajectory exists — an unbound required slot. Absent
   * off the compose path.
   */
  gateReason?: string | null;
  /**
   * The apply write path, supplied only once an approval exists: apply installs
   * that frozen snapshot as the running identity. Absent → no Apply control.
   */
  onApply?: () => void;
  /**
   * Compose-path apply outcome: the appended apply row, or the refusal's own
   * typed code. Absent until the wired handler ran.
   */
  application?: ApplyRenderState | null;
  /** Re-run the static walk for the current candidate (the stale/failed recovery). */
  onRun?: () => void;
}

export function ApproveDisclosure({
  open,
  onOpenChange,
  status,
  isStale = false,
  trajectory,
  error,
  onApprove,
  approval = null,
  gateReason = null,
  onApply,
  application = null,
  onRun,
}: ApproveDisclosureProps) {
  const { t } = useTranslation('canvas');
  // `status` owns the banner; `isStale` only re-adds it while a newer fact
  // (`running`, `failed`) outranks staleness in `status`.
  const showStale = status === 'stale' || isStale;

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
            {t('strategy.approve.failed', { message: errorMessage(error) })}
          </p>
        ) : null}

        {showStale ? (
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

        {gateReason ? (
          <p
            data-testid="approve-gate"
            role="alert"
            className="mt-2 rounded-card border border-amber-700 px-3 py-2 text-copy-13 text-gray-1000"
          >
            {gateReason}
          </p>
        ) : null}

        {onApprove ? null : (
          <p
            data-testid="approve-unavailable"
            className="mt-2 rounded-card border border-gray-alpha-400 px-3 py-2 text-copy-13 text-gray-900"
          >
            {t('strategy.approve.approvalUnavailable')}
          </p>
        )}

        {approval?.kind === 'pending' ? (
          <p data-testid="approve-pending" className="mt-2 text-copy-13 text-gray-900">
            {t('strategy.approve.pending')}
          </p>
        ) : null}

        {approval?.kind === 'refused' ? (
          <p
            data-testid="approve-refused"
            role="alert"
            className="mt-2 rounded-card border border-amber-700 px-3 py-2 text-copy-13 text-gray-1000"
          >
            {approval.message}
          </p>
        ) : null}

        {approval?.kind === 'approved' ? (
          <div data-testid="approve-approved" className="mt-3 flex flex-col gap-2">
            <p className="rounded-card border border-teal-700 px-3 py-2 text-copy-13 text-gray-1000">
              {t('strategy.approve.frozen', { approvalId: approval.snapshot.approval_id })}
            </p>
            <p className="text-copy-13 text-gray-900">{t('strategy.approve.noPublish')}</p>
            <dl className="flex flex-col gap-2">
              <div>
                <dt className="text-label-12 text-gray-700">{t('strategy.approve.pins')}</dt>
                <dd className="font-mono text-label-12 text-gray-1000">
                  <ul>
                    {approval.snapshot.pins.map((pin) => (
                      <li key={`${pin.id}@${pin.version}`} data-testid="approve-pin">
                        {pin.id}@{pin.version} · {pin.content_hash}
                      </li>
                    ))}
                  </ul>
                </dd>
              </div>
              <div>
                <dt className="text-label-12 text-gray-700">
                  {t('strategy.approve.simulationIdentity')}
                </dt>
                <dd data-testid="approve-simulation-identity" className="font-mono text-label-12 text-gray-1000">
                  {approval.snapshot.trajectory.ir_content_hash}
                </dd>
              </div>
              <div>
                <dt className="text-label-12 text-gray-700">{t('strategy.approve.candidateRevision')}</dt>
                <dd className="font-mono text-label-12 text-gray-1000">
                  {approval.snapshot.candidate_revision}
                </dd>
              </div>
            </dl>

            {application?.kind === 'pending' ? (
              <p data-testid="apply-pending" className="text-copy-13 text-gray-900">
                {t('strategy.apply.pending')}
              </p>
            ) : null}

            {application?.kind === 'refused' ? (
              <p
                data-testid="apply-refused"
                role="alert"
                className="rounded-card border border-amber-700 px-3 py-2 text-copy-13 text-gray-1000"
              >
                {application.message}
              </p>
            ) : null}

            {application?.kind === 'applied' ? (
              <div data-testid="apply-applied" className="flex flex-col gap-2">
                <p className="rounded-card border border-teal-700 px-3 py-2 text-copy-13 text-gray-1000">
                  {t('strategy.apply.applied', {
                    applyId: application.applied.apply_id,
                    appliedAt: application.applied.applied_at,
                  })}
                </p>
                <dl className="flex flex-col gap-2">
                  <div>
                    <dt className="text-label-12 text-gray-700">
                      {t('strategy.apply.approval')}
                    </dt>
                    <dd
                      data-testid="apply-approval"
                      className="font-mono text-label-12 text-gray-1000"
                    >
                      {application.applied.approval_id}
                    </dd>
                  </div>
                  <div>
                    <dt className="text-label-12 text-gray-700">
                      {t('strategy.apply.contentHash')}
                    </dt>
                    <dd
                      data-testid="apply-content-hash"
                      className="font-mono text-label-12 text-gray-1000"
                    >
                      {application.applied.content_hash}
                    </dd>
                  </div>
                </dl>
              </div>
            ) : null}

            {onApply ? (
              <div>
                <button
                  type="button"
                  onClick={onApply}
                  disabled={application?.kind === 'pending'}
                  className="rounded-control bg-purple-700 px-3 py-1.5 text-button-12 text-white hover:bg-purple-800 disabled:cursor-not-allowed disabled:opacity-50"
                >
                  {t('strategy.apply.action')}
                </button>
              </div>
            ) : null}
          </div>
        ) : null}

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
            disabled={status !== 'ready' || !onApprove || Boolean(gateReason)}
            className="rounded-control bg-purple-700 px-3 py-1.5 text-button-12 text-white hover:bg-purple-800 disabled:cursor-not-allowed disabled:opacity-50"
          >
            {t('strategy.approve.approve')}
          </button>
        </div>
      </DialogContent>
    </Dialog>
  );
}

/** A throw of any shape still names its cause instead of rendering an empty message. */
function errorMessage(error: unknown): string {
  if (error instanceof Error) return error.message;
  if (typeof error === 'string') return error;
  if (typeof error === 'object' && error !== null) {
    try {
      return JSON.stringify(error);
    } catch {
      // Cyclic or BigInt-bearing error object: naming it is better than blank.
      return 'unserializable error';
    }
  }
  return String(error);
}

/** `simulation_level` → copy. Unknown levels are surfaced as unsupported, never assumed. */
const LEVEL_COPY: Record<string, string> = {
  reachability: 'strategy.approve.level.reachability',
};
