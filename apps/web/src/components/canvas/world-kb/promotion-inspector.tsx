/**
 * World KB promotion inspector — adopts / rejects / merges a pending promotion
 * candidate via `world_kb.promote_candidate` (V1.73 P0 A6).
 *
 * Per-row OCC on `kb_extract_jobs.version`. Conflicts (409) are handed to the
 * parent canvas, which renders the `promote_candidate` conflict modal. The
 * merge flow selects a target confirmed KeyBlock from the graph projection.
 */
import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { Select } from '@/components/ui/select';
import { Label } from '@/components/ui/label';
import { Button } from '@/components/ui/button';
import {
  isWorldKbValidationError,
  usePromoteWorldKbCandidate,
} from '@/lib/canvas/use-world-kb-data';
import type { WorldKbCandidateProjection, WorldKbEntityProjection } from '@42ch/nexus-contracts';
import type { WorldKbPromoteAction, WorldKbCanonicalStatus } from './world-kb-conflict-modal';
import { BLOCK_TYPE_LABELS, type WorldKbNodeData } from './types';

export interface PromotionInspectorProps {
  worldId: string;
  /** The selected pending-candidate node. */
  node: WorldKbNodeData;
  /** The canonical candidate projection backing the node. */
  candidate: WorldKbCandidateProjection;
  /** Confirmed KeyBlocks in this world (merge targets). */
  confirmedEntities: WorldKbEntityProjection[];
  /** Called when a 409 conflict is detected. */
  onConflict: (payload: {
    currentVersion: number;
    candidateName: string;
    newStatus: WorldKbCanonicalStatus;
    action: WorldKbPromoteAction;
    mergeTargetId?: string;
    mergeTargetLabel?: string;
  }) => void;
  /** External reseed signal (e.g. after "Use current" in the conflict modal). */
  reseedSignal?: number;
}

export function PromotionInspector({
  worldId,
  node,
  candidate,
  confirmedEntities,
  onConflict,
  reseedSignal,
}: PromotionInspectorProps) {
  const { t } = useTranslation('canvas');
  const promote = usePromoteWorldKbCandidate(worldId);
  const [action, setAction] = useState<WorldKbPromoteAction>('adopt');
  const [mergeTargetId, setMergeTargetId] = useState<string>('');
  const [validationErrors, setValidationErrors] = useState<string[]>([]);

  useEffect(() => {
    setAction('adopt');
    setMergeTargetId('');
    setValidationErrors([]);
  }, [candidate.candidate_id, reseedSignal]); // eslint-disable-line react-hooks/exhaustive-deps

  // Backend `promote_merge` only accepts `confirmed` or `manual` targets
  // (world_kb.rs promote_merge status guard); any other status that has leaked
  // into `confirmedEntities` (e.g. `merged`/`rejected`/`deprecated`/`deleted`)
  // would 422 at submit with no client-side hint. Filter the dropdown to the
  // backend's allowed set so users never pick a target the daemon will reject.
  // (greptile 4→5)
  const mergeTargets = confirmedEntities.filter(
    (e) =>
      e.block_type === candidate.block_type &&
      (e.status?.toLowerCase() === 'confirmed' || e.status?.toLowerCase() === 'manual'),
  );

  function handleSubmit() {
    setValidationErrors([]);
    if (action === 'merge' && !mergeTargetId) {
      setValidationErrors([t('worldKb.promotionInspector.selectMergeTarget')]);
      return;
    }
    const mergeTarget = mergeTargets.find((e) => e.key_block_id === mergeTargetId);
    promote.mutate(
      {
        job_id: candidate.job_id,
        candidate_id: candidate.candidate_id,
        action,
        expected_version: node.version,
        merge_target_id: action === 'merge' ? mergeTargetId : undefined,
      },
      {
        onError: (error) => {
          if (isWorldKbValidationError(error)) {
            const details = error.details as { validation_summary?: { errors?: string[] } } | undefined;
            setValidationErrors(details?.validation_summary?.errors ?? [t('worldKb.promotionInspector.validationFailed')]);
            return;
          }
          const details = error as unknown as {
            status: number;
            details?: {
              current_version?: number;
              conflicting_path?: string;
              entity_id?: string;
              recovery_hint?: string;
            };
          };
          if (details.status === 409) {
            // Infer the canonical promotion action from the recovery hint /
            // conflicting path when available; default to the user's action tense.
            const hint = details.details?.recovery_hint ?? '';
            const newStatus: WorldKbCanonicalStatus = inferCanonicalStatus(hint, action);
            onConflict({
              currentVersion: details.details?.current_version ?? node.version,
              candidateName: candidate.canonical_name,
              newStatus,
              action,
              mergeTargetId: action === 'merge' ? mergeTargetId : undefined,
              mergeTargetLabel: mergeTarget?.canonical_name,
            });
          }
        },
      },
    );
  }

  return (
    <form
      className="flex flex-col gap-3"
      onSubmit={(e) => {
        e.preventDefault();
        handleSubmit();
      }}
    >
      <div className="flex items-center justify-between gap-2">
        <h3 className="text-heading-16 font-heading text-gray-1000">{t('worldKb.promotionInspector.title')}</h3>
        <span className="rounded-pill bg-canvas-worldkb-promotion-pending/15 px-1.5 py-0.5 text-label-12 text-canvas-worldkb-promotion-pending">
          {t('worldKb.promotionInspector.pendingBadge', { version: node.version })}
        </span>
      </div>
      <p className="text-copy-13 text-gray-700">{t('worldKb.promotionInspector.description')}</p>

      <dl className="grid grid-cols-1 gap-2 rounded-card border border-gray-alpha-300 bg-background-100 p-3">
        <Row label={t('worldKb.promotionInspector.field.name')} value={candidate.canonical_name || t('worldKb.promotionInspector.unnamed')} />
        <Row label={t('worldKb.promotionInspector.field.blockType')} value={BLOCK_TYPE_LABELS[candidate.block_type]} />
        <Row label={t('worldKb.promotionInspector.field.job')} value={<span className="font-mono text-copy-13-mono">{shortId(candidate.job_id)}</span>} />
      </dl>

      <fieldset className="flex flex-col gap-2">
        <legend className="text-label-14 font-medium text-gray-1000">{t('worldKb.promotionInspector.decisionLegend')}</legend>
        {(['adopt', 'reject', 'merge'] as const).map((a) => (
          <label key={a} className="flex items-center gap-2 text-copy-14 text-gray-900">
            <input
              type="radio"
              name="wkbp-action"
              value={a}
              checked={action === a}
              onChange={() => setAction(a)}
              className="h-4 w-4"
            />
            <span>
              {t(`worldKb.promotionInspector.action.${a}.label`)}
              <span className="text-copy-13 text-gray-700"> — {t(`worldKb.promotionInspector.action.${a}.help`)}</span>
            </span>
          </label>
        ))}
      </fieldset>

      {action === 'merge' ? (
        <div className="flex flex-col gap-1">
          <Label htmlFor="wkbp-merge">
            {t('worldKb.promotionInspector.mergeInto', { blockType: BLOCK_TYPE_LABELS[candidate.block_type] })}
          </Label>
          {mergeTargets.length === 0 ? (
            <p className="rounded-card border border-amber-700/30 bg-amber-700/10 p-2 text-copy-13 text-amber-1000">
              {t('worldKb.promotionInspector.noMergeTargets', { blockType: BLOCK_TYPE_LABELS[candidate.block_type] })}
            </p>
          ) : (
            <Select
              id="wkbp-merge"
              value={mergeTargetId}
              onChange={(e) => setMergeTargetId(e.target.value)}
            >
              <option value="">{t('worldKb.promotionInspector.selectTarget')}</option>
              {mergeTargets.map((e) => (
                <option key={e.key_block_id} value={e.key_block_id}>
                  {e.canonical_name} (v{e.version})
                </option>
              ))}
            </Select>
          )}
        </div>
      ) : null}

      {validationErrors.length > 0 ? (
        <ul
          className="rounded-card border border-red-700/30 bg-red-700/10 p-3 text-copy-13 text-red-1000"
          aria-live="polite"
        >
          {validationErrors.map((err, i) => (
            <li key={i}>{err}</li>
          ))}
        </ul>
      ) : null}

      <div className="flex items-center justify-end gap-2">
        <Button type="submit" disabled={promote.isPending || (action === 'merge' && !mergeTargetId)}>
          {promote.isPending ? t('worldKb.promotionInspector.submitting') : t(`worldKb.promotionInspector.action.${action}.label`)}
        </Button>
      </div>
    </form>
  );
}

function Row({ label, value }: { label: string; value: React.ReactNode }) {
  return (
    <div className="flex items-center justify-between gap-2">
      <dt className="text-label-12 text-gray-700">{label}</dt>
      <dd className="text-copy-14 text-gray-1000">{value}</dd>
    </div>
  );
}

function shortId(id: string): string {
  return id.length > 12 ? `${id.slice(0, 8)}…` : id;
}

/** Best-effort inference of the canonical promotion state from the recovery hint. */
function inferCanonicalStatus(hint: string, action: WorldKbPromoteAction): WorldKbCanonicalStatus {
  const h = hint.toLowerCase();
  if (h.includes('adopt') || h.includes('confirm')) return 'adopted';
  if (h.includes('reject')) return 'rejected';
  if (h.includes('merge')) return 'merged';
  // Fallback: mirror the user's pending action as the canonical tense.
  return action === 'adopt' ? 'adopted' : action === 'reject' ? 'rejected' : 'merged';
}
