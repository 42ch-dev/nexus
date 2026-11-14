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

import type {
  SimulationTrajectory,
  StrategyAppliedStatus,
  StrategyApprovals,
} from '@42ch/nexus-contracts';

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
   * The apply write path: apply installs the chosen approval as the running
   * identity. It is aimed at a listed approval, never at whatever the dialog
   * happens to hold — that is how a superseded approval stays reachable.
   * Absent → no Apply control.
   */
  onApply?: (approval: StrategyApprovals['items'][number]) => void;
  /**
   * Every approval of this strategy, newest first (the status read's total
   * order). The status read alone carries only the newest approval, so a
   * rollback target exists only here. Absent or empty → no Apply control.
   */
  approvals?: StrategyApprovals['items'] | null;
  /**
   * The `approval_id` Apply is aimed at. Unset → the newest listed approval,
   * which is the happy path after an approve.
   */
  applyTargetId?: string | null;
  /** Aim Apply at another listed approval (the rollback target). */
  onSelectApplyTarget?: (approvalId: string) => void;
  /** The approvals read failed; its message replaces a silently missing Apply control. */
  approvalsError?: string | null;
  /**
   * Compose-path apply outcome: the appended apply row, or the refusal's own
   * typed code. Absent until the wired handler ran.
   */
  application?: ApplyRenderState | null;
  /** Re-run the static walk for the current candidate (the stale/failed recovery). */
  onRun?: () => void;
  /**
   * Live apply-state read (compose path): which approval is running and the
   * newest approval, to badge a live apply that is not the newest. Absent →
   * no live block and no badge. The badge compares `approval_id`s only.
   */
  appliedStatus?: StrategyAppliedStatus | null;
  /** The live read has not returned its first row yet (no `appliedStatus` data). */
  appliedStatusPending?: boolean;
  /** The live read failed; its message names the failure instead of a silent "no apply". */
  appliedStatusError?: string | null;
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
  approvals = null,
  applyTargetId = null,
  onSelectApplyTarget,
  approvalsError = null,
  application = null,
  onRun,
  appliedStatus = null,
  appliedStatusPending = false,
  appliedStatusError = null,
}: ApproveDisclosureProps) {
  const { t } = useTranslation('canvas');
  // `status` owns the banner; `isStale` only re-adds it while a newer fact
  // (`running`, `failed`) outranks staleness in `status`.
  const showStale = status === 'stale' || isStale;
  // Apply is aimed at a listed approval, defaulting to the newest. A stale
  // selection (an approval a refetch no longer returns) falls back to newest.
  const applyTarget =
    approvals?.find((candidate) => candidate.approval_id === applyTargetId) ??
    approvals?.[0] ??
    null;
  const liveApprovalId = appliedStatus?.current_apply?.approval_id ?? null;
  const newestApprovalId = appliedStatus?.newest_approval?.approval_id ?? null;
  // Three cases, `approval_id`s only (D11). Aimed at the live approval → the
  // apply is an `already_applied` refusal, not a rollback. Aimed at the newest
  // → applying moves forward, so the "newer approval available" badge stands
  // alone. Aimed at anything else → applying it moves production back.
  const showRollbackNote =
    applyTarget !== null &&
    liveApprovalId !== null &&
    applyTarget.approval_id !== liveApprovalId &&
    applyTarget.approval_id !== newestApprovalId;

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

        {appliedStatusError ? (
          <p
            data-testid="apply-live-error"
            role="alert"
            className="rounded-card border border-amber-700 px-3 py-2 text-copy-13 text-gray-1000"
          >
            {t('strategy.apply.liveUnreadable', { message: appliedStatusError })}
          </p>
        ) : appliedStatusPending && !appliedStatus ? (
          <p data-testid="apply-live-pending" className="text-copy-13 text-gray-900">
            {t('strategy.apply.livePending')}
          </p>
        ) : null}

        {appliedStatus?.current_apply ? (
          <div data-testid="apply-live" className="mt-3 flex flex-col gap-2">
            <p className="rounded-card border border-teal-700 px-3 py-2 text-copy-13 text-gray-1000">
              {t('strategy.apply.live', {
                approvalId: appliedStatus.current_apply.approval_id,
                appliedAt: appliedStatus.current_apply.applied_at,
              })}
            </p>
            {/* D9: informational, never a warning; it blocks neither apply nor
                rollback and disappears when the live approval is the newest. */}
            {appliedStatus.newest_approval &&
            appliedStatus.newest_approval.approval_id !==
              appliedStatus.current_apply.approval_id ? (
              <p
                data-testid="apply-newer-available"
                className="rounded-card border border-purple-700 px-3 py-2 text-copy-13 text-purple-1000"
              >
                {t('strategy.apply.newerApprovalAvailable')}
              </p>
            ) : null}
          </div>
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
          </div>
        ) : null}

        {approvalsError ? (
          <p
            data-testid="apply-targets-error"
            role="alert"
            className="mt-2 rounded-card border border-amber-700 px-3 py-2 text-copy-13 text-gray-1000"
          >
            {t('strategy.apply.targetsUnreadable', { message: approvalsError })}
          </p>
        ) : null}

        {onApply && approvals && approvals.length > 0 && applyTarget ? (
          <div data-testid="apply-targets" className="mt-3 flex flex-col gap-2">
            <label className="flex flex-col gap-1 text-label-12 text-gray-700">
              {t('strategy.apply.target')}
              <select
                data-testid="apply-target-select"
                value={applyTarget.approval_id}
                onChange={(event) => onSelectApplyTarget?.(event.target.value)}
                className="rounded-control border border-gray-alpha-400 bg-background-100 px-3 py-2 text-gray-1000 focus:border-blue-1000 dark:focus:border-blue-700"
              >
                {approvals.map((candidate) => (
                  <option key={candidate.approval_id} value={candidate.approval_id}>
                    {t('strategy.apply.targetOption', {
                      approvalId: candidate.approval_id,
                      revision: candidate.candidate_revision,
                    })}
                  </option>
                ))}
              </select>
            </label>

            <button
              type="button"
              onClick={() => onApply(applyTarget)}
              disabled={application?.kind === 'pending'}
              className="rounded-control bg-purple-700 px-3 py-1.5 text-button-12 text-white hover:bg-purple-800 disabled:cursor-not-allowed disabled:opacity-50"
            >
              {t('strategy.apply.action')}
            </button>
            {/* Rollback is this same control aimed at another listed approval:
                history is append-only, so the copy never implies deletion. The
                note appears only when the target is neither the live apply nor
                the newest approval — only then does applying move production
                back. Aimed at the live one it is an `already_applied` refusal;
                aimed at the newest it moves forward even while the D9 badge
                says a newer approval exists. */}
            {showRollbackNote ? (
              <p data-testid="apply-rollback-note" className="text-copy-13 text-gray-900">
                {t('strategy.apply.rollbackNote')}
              </p>
            ) : null}

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
