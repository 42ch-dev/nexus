/**
 * Finding detail/inspector panel — V1.77 findings-remediation surface.
 *
 * Spec: `.mstar/knowledge/specs/web-ui.md` §23 + `findings-lifecycle.md` §4.
 * Three remediation affordances consuming `PATCH .../findings/{id}`:
 *   1. Status transitions (6-state; invalid disabled per server adjacency).
 *   2. `target_executor` assignment (brainstorm/write/master/none).
 *   3. Inline edit of title/description/severity/kind/rule_suggestion.
 *
 * Layout (D4 LOCKED): detail-panel + row-action hybrid. This panel mounts
 * beside the findings table for the selected row.
 */
import { useEffect, useMemo, useState } from 'react';

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
        <Label className="text-gray-900">Status</Label>
        <div className="flex flex-wrap items-center gap-2">
          <FindingStatusBadge status={finding.status} />
          {terminal ? (
            <span className="text-copy-13 text-gray-700">Terminal — no further transitions.</span>
          ) : (
            FINDING_STATUSES.filter((s) => s !== finding.status).map((s) => (
              <Button
                key={s}
                type="button"
                variant="secondary"
                size="small"
                disabled={pending || !isValidTransition(finding.status, s)}
                onClick={() => transition(s)}
                aria-label={`Advance finding to ${s.replace(/_/g, ' ')}`}
              >
                {s === 'in_review' ? 'In Review' : s.charAt(0).toUpperCase() + s.slice(1)}
              </Button>
            ))
          )}
        </div>
      </section>

      {/* ── Target executor assignment ─────────────────────────────────── */}
      <section className="flex flex-col gap-1.5">
        <Label htmlFor="finding-target-executor">Target Executor</Label>
        <Select
          id="finding-target-executor"
          value={finding.target_executor}
          onChange={(e) => assignExecutor(e.target.value)}
          disabled={pending}
        >
          {TARGET_EXECUTOR_OPTIONS.map((opt) => (
            <option key={opt} value={opt}>
              {opt.charAt(0).toUpperCase() + opt.slice(1)}
            </option>
          ))}
        </Select>
        <p className="text-copy-13 text-gray-700">
          Routes the finding for triage. Re-running a preset stays a deliberate canvas action.
        </p>
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
            Chapter: {finding.chapter ?? '—'}
          </span>
          {finding.routing_hint && (
            <span data-testid="finding-context-routing" className="text-gray-1000">
              Routing: {finding.routing_hint}
            </span>
          )}
          <span>
            ID: <span className="text-copy-13-mono text-gray-700">{shortId(finding.finding_id)}</span>
          </span>
        </div>
        <div className="flex flex-wrap gap-x-6 gap-y-1">
          <span>Created: <span className="text-gray-1000">{formatRelative(formatIso(finding.created_at))}</span></span>
          <span>Updated: <span className="text-gray-1000">{formatRelative(formatIso(finding.updated_at))}</span></span>
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
