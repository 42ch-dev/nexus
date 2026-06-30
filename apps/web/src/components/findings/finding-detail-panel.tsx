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
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Label } from '@/components/ui/label';
import { Select } from '@/components/ui/select';
import { useUpdateFinding } from '@/api/queries';
import {
  FINDING_STATUSES,
  isTerminalStatus,
  isValidTransition,
  nextStatuses,
} from '@/lib/findings-lifecycle';
import { formatRelative, shortId } from '@/lib/format';
import type { FindingDetailResponse, UpdateFindingRequest } from '@42ch/nexus-contracts';

/** DAO `VALID_SEVERITIES` (`crates/nexus-local-db/src/findings.rs:104`). */
const SEVERITY_OPTIONS = ['info', 'minor', 'major', 'blocker'] as const;

/** DAO `VALID_TARGET_EXECUTORS` (`findings.rs:192`). */
const TARGET_EXECUTOR_OPTIONS = ['write', 'brainstorm', 'master', 'none'] as const;

interface FindingDetailPanelProps {
  workId: string;
  finding: FindingDetailResponse;
}

/**
 * Compute the minimal PATCH diff between the canonical finding and the edited
 * form state. Only changed fields are included so the wire stays small and
 * unchanged fields are not risked.
 *
 * `rule_suggestion` is a plain string on the wire
 * (`update-finding-request.schema.json` — `"type": "string"`, not nullable);
 * clearing the field sends an empty string.
 */
function buildPatch(
  finding: FindingDetailResponse,
  form: InlineForm,
): UpdateFindingRequest | null {
  const patch: UpdateFindingRequest = {};
  if (form.title !== finding.title) patch.title = form.title;
  if (form.description !== finding.description) patch.description = form.description;
  if (form.severity !== finding.severity) patch.severity = form.severity;
  if (form.kind !== finding.kind) patch.kind = form.kind;
  if (form.ruleSuggestion !== (finding.rule_suggestion ?? '')) {
    patch.rule_suggestion = form.ruleSuggestion;
  }
  return Object.keys(patch).length > 0 ? patch : null;
}

interface InlineForm {
  title: string;
  description: string;
  severity: string;
  kind: string;
  ruleSuggestion: string;
}

function formFromFinding(f: FindingDetailResponse): InlineForm {
  return {
    title: f.title,
    description: f.description,
    severity: f.severity,
    kind: f.kind,
    ruleSuggestion: f.rule_suggestion ?? '',
  };
}

export function FindingDetailPanel({ workId, finding }: FindingDetailPanelProps) {
  const updateFinding = useUpdateFinding();
  const [form, setForm] = useState<InlineForm>(() => formFromFinding(finding));

  // Re-sync local form when the selected finding changes (row switch) or the
  // server returns a canonical update that invalidates the snapshot.
  useEffect(() => {
    setForm(formFromFinding(finding));
  }, [finding.finding_id, finding.updated_at]);

  const patch = useMemo(() => buildPatch(finding, form), [finding, form]);
  const isDirty = patch !== null;
  const reachable = nextStatuses(finding.status);
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
            reachable.map((s) => (
              <Button
                key={s}
                type="button"
                variant="secondary"
                size="small"
                disabled={pending}
                onClick={() => transition(s)}
                aria-label={`Advance finding to ${s.replace(/_/g, ' ')}`}
              >
                {s === 'in_review' ? 'In Review' : s.charAt(0).toUpperCase() + s.slice(1)}
              </Button>
            ))
          )}
        </div>
        {/* Disabled affordances for the remaining non-reachable statuses make the
            6-state machine legible without offering illegal transitions. */}
        {!terminal && reachable.length < FINDING_STATUSES.length - 1 && (
          <div className="flex flex-wrap items-center gap-2">
            <span className="text-copy-13 text-gray-700">Not reachable:</span>
            {FINDING_STATUSES.filter(
              (s) => s !== finding.status && !isValidTransition(finding.status, s),
            ).map((s) => (
              <Badge key={s} className="opacity-50">
                {s === 'in_review' ? 'In Review' : s.charAt(0).toUpperCase() + s.slice(1)}
              </Badge>
            ))}
          </div>
        )}
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
      <section className="flex flex-col gap-3">
        <div className="flex flex-col gap-1.5">
          <Label htmlFor="finding-title">Title</Label>
          <input
            id="finding-title"
            type="text"
            value={form.title}
            onChange={(e) => setForm((f) => ({ ...f, title: e.target.value }))}
            disabled={pending}
            className="h-10 w-full rounded-control border border-gray-alpha-400 bg-background-100 px-3 text-copy-14 text-gray-1000 disabled:bg-gray-100 disabled:text-gray-700"
          />
        </div>
        <div className="flex flex-col gap-1.5">
          <Label htmlFor="finding-description">Description</Label>
          <textarea
            id="finding-description"
            value={form.description}
            onChange={(e) => setForm((f) => ({ ...f, description: e.target.value }))}
            disabled={pending}
            rows={4}
            className="min-h-[96px] w-full rounded-control border border-gray-alpha-400 bg-background-100 px-3 py-2 text-copy-14 text-gray-1000 disabled:bg-gray-100 disabled:text-gray-700"
          />
        </div>
        <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
          <div className="flex flex-col gap-1.5">
            <Label htmlFor="finding-severity">Severity</Label>
            <Select
              id="finding-severity"
              value={form.severity}
              onChange={(e) => setForm((f) => ({ ...f, severity: e.target.value }))}
              disabled={pending}
            >
              {SEVERITY_OPTIONS.map((opt) => (
                <option key={opt} value={opt}>
                  {opt.charAt(0).toUpperCase() + opt.slice(1)}
                </option>
              ))}
            </Select>
          </div>
          <div className="flex flex-col gap-1.5">
            <Label htmlFor="finding-kind">Kind</Label>
            <input
              id="finding-kind"
              type="text"
              value={form.kind}
              onChange={(e) => setForm((f) => ({ ...f, kind: e.target.value }))}
              disabled={pending}
              className="h-10 w-full rounded-control border border-gray-alpha-400 bg-background-100 px-3 text-copy-14 text-gray-1000 disabled:bg-gray-100 disabled:text-gray-700"
            />
          </div>
        </div>
        <div className="flex flex-col gap-1.5">
          <Label htmlFor="finding-rule-suggestion">Rule Suggestion</Label>
          <input
            id="finding-rule-suggestion"
            type="text"
            value={form.ruleSuggestion}
            onChange={(e) => setForm((f) => ({ ...f, ruleSuggestion: e.target.value }))}
            disabled={pending}
            placeholder="Clear to remove the rule suggestion"
            className="h-10 w-full rounded-control border border-gray-alpha-400 bg-background-100 px-3 text-copy-14 text-gray-1000 disabled:bg-gray-100 disabled:text-gray-700"
          />
        </div>
        <div className="flex items-center gap-2">
          <Button
            type="button"
            variant="primary"
            size="small"
            onClick={saveInline}
            disabled={!isDirty || pending}
          >
            Save Changes
          </Button>
          <Button
            type="button"
            variant="tertiary"
            size="small"
            onClick={resetInline}
            disabled={!isDirty || pending}
          >
            Reset
          </Button>
          {isDirty && (
            <span className="text-copy-13 text-gray-700">Unsaved changes</span>
          )}
        </div>
      </section>

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
