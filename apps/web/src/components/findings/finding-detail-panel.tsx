/**
 * Finding detail/inspector panel — V1.77 findings-remediation surface.
 *
 * Spec: `.mstar/specs/web-ui.md` §23 + `findings-lifecycle.md` §4.
 * Three remediation affordances consuming `PATCH .../findings/{id}`:
 *   1. Status transitions (6-state; invalid disabled per server adjacency).
 *   2. `target_executor` assignment (brainstorm/write/master/none).
 *   3. Inline edit of title/description/severity/kind/rule_suggestion.
 *
 * Layout (D4 LOCKED): detail-panel + row-action hybrid. This panel mounts
 * beside the findings table for the selected row.
 */
import { useEffect, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { FindingStatusBadge } from '@/components/status-badge';
import { Button } from '@/components/ui/button';
import { Label } from '@/components/ui/label';
import { Select } from '@/components/ui/select';
import { useUpdateFinding } from '@/api/queries';
import { FINDING_STATUSES, isTerminalStatus, isValidTransition } from '@/lib/findings-lifecycle';
import { formatRelative, shortId } from '@/lib/format';
import type { FindingDetailResponse } from '@42ch/nexus-contracts';

import {
  FindingInlineEditForm,
  buildPatch,
  formFromFinding,
  type InlineForm,
} from './finding-inline-edit-form';

/** DAO `VALID_TARGET_EXECUTORS` (`findings.rs:192`). */
const TARGET_EXECUTOR_OPTIONS = ['write', 'brainstorm', 'master', 'none'] as const;

interface FindingDetailPanelProps {
  workId: string;
  finding: FindingDetailResponse;
}

export function FindingDetailPanel({ workId, finding }: FindingDetailPanelProps) {
  const { t } = useTranslation('findings');
  const updateFinding = useUpdateFinding();
  const [form, setForm] = useState<InlineForm>(() => formFromFinding(finding));

  // Re-sync local form only on row switch (finding_id change). Do NOT depend on
  // finding.updated_at: the server bumps it on every status transition, which
  // would silently discard unsaved inline edits mid-triage. The status field is
  // read directly from finding.status (not form state), so it stays live without
  // a re-sync. resetInline() remains the manual re-sync from server state.
  useEffect(() => {
    setForm(formFromFinding(finding));
  }, [finding.finding_id]);

  const patch = useMemo(() => buildPatch(finding, form), [finding, form]);
  const terminal = isTerminalStatus(finding.status);

  const transition = (status: string) => {
    updateFinding.mutate({ workId, findingId: finding.finding_id, patch: { status } });
  };

  const assignExecutor = (target_executor: string) => {
    updateFinding.mutate({
      workId,
      findingId: finding.finding_id,
      patch: { target_executor },
    });
  };

  const saveInline = () => {
    if (!patch) return;
    updateFinding.mutate({ workId, findingId: finding.finding_id, patch });
  };

  const resetInline = () => setForm(formFromFinding(finding));

  const pending = updateFinding.isPending;

  return (
    <div className="flex flex-col gap-4">
      {/* ── Status transitions ─────────────────────────────────────────── */}
      <section className="flex flex-col gap-2">
        <Label className="text-gray-900">{t('detail.statusLabel')}</Label>
        <div className="flex flex-wrap items-center gap-2">
          <FindingStatusBadge status={finding.status} />
          {terminal ? (
            <span className="text-copy-13 text-gray-700">{t('detail.terminal')}</span>
          ) : (
            FINDING_STATUSES.filter((s) => s !== finding.status).map((s) => (
              <Button
                key={s}
                type="button"
                variant="secondary"
                size="small"
                disabled={pending || !isValidTransition(finding.status, s)}
                onClick={() => transition(s)}
                aria-label={t('detail.advanceAria', { status: t(`status.${s}` as const) })}
              >
                {t(`status.${s}` as const)}
              </Button>
            ))
          )}
        </div>
      </section>

      {/* ── Target executor assignment ─────────────────────────────────── */}
      <section className="flex flex-col gap-1.5">
        <Label htmlFor="finding-target-executor">{t('detail.targetExecutorLabel')}</Label>
        <Select
          id="finding-target-executor"
          value={finding.target_executor}
          onChange={(e) => assignExecutor(e.target.value)}
          disabled={pending}
        >
          {TARGET_EXECUTOR_OPTIONS.map((opt) => (
            <option key={opt} value={opt}>
              {t(`executors.${opt}` as const)}
            </option>
          ))}
        </Select>
        <p className="text-copy-13 text-gray-700">{t('detail.targetExecutorHelp')}</p>
      </section>

      {/* ── Inline edit ────────────────────────────────────────────────── */}
      <FindingInlineEditForm
        form={form}
        setForm={setForm}
        patch={patch}
        pending={pending}
        onSave={saveInline}
        onReset={resetInline}
      />

      {/* ── Context readout ────────────────────────────────────────────── */}
      <section className="flex flex-col gap-1 border-t border-gray-alpha-400 pt-3 text-copy-13 text-gray-900">
        <div className="flex flex-wrap gap-x-6 gap-y-1">
          <span data-testid="finding-context-chapter" className="tabular-nums text-gray-1000">
            {t('detail.chapterLabel', { chapter: finding.chapter ?? '—' })}
          </span>
          {finding.routing_hint && (
            <span data-testid="finding-context-routing" className="text-gray-1000">
              {t('detail.routingLabel', { routing: finding.routing_hint })}
            </span>
          )}
          <span>
            {t('detail.idLabel', { id: shortId(finding.finding_id) })}
          </span>
        </div>
        <div className="flex flex-wrap gap-x-6 gap-y-1">
          <span>
            {t('detail.createdLabel', { date: formatRelative(formatIso(finding.created_at)) })}
          </span>
          <span>
            {t('detail.updatedLabel', { date: formatRelative(formatIso(finding.updated_at)) })}
          </span>
        </div>
      </section>
    </div>
  );
}

/**
 * The DAO stores timestamps as epoch seconds; `formatRelative` expects an ISO
 * string. Convert defensively (already-ISO passes through; numbers are treated
 * as epoch seconds).
 */
function formatIso(ts: number | string | undefined | null): string | undefined {
  if (ts === undefined || ts === null) return undefined;
  if (typeof ts === 'string') return ts;
  if (typeof ts === 'number') {
    return Number.isFinite(ts) ? new Date(ts * 1000).toISOString() : undefined;
  }
  return undefined;
}
