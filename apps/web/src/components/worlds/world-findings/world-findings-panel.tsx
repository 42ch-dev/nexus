/**
 * World Findings panel — V1.166 P2 (DR-64 surfacing half, Task 1).
 *
 * Read-only list of world-scoped check findings (mental pair + rule-derived)
 * consumed from `GET /v1/daemon/worlds/:world_id/findings` via
 * {@link useWorldFindings}. Control Room list/sections conventions — NOT a
 * canvas mount and NOT the Work `FindingDetailPanel` remediation surface
 * (PD-2): this panel has zero write controls (no accept/resolve/remediate).
 *
 * Vocabulary contract (PD-2, locked): severity renders spoke `info|warning|error`
 * verbatim plus an open-string fallback — never remapped to the work-findings
 * `minor/major/blocker`; `kind` is an open string shown verbatim. List order is
 * exactly what the API returns (newest-first, 500-cap); `truncated: true`
 * renders honest copy mirroring the daemon cap.
 *
 * Optional per-item read-only expand shows the full description, the target
 * entry id, and the created timestamp. No filtering/sorting/polling (out of
 * scope — roadmap).
 */
import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { ChevronDown } from 'lucide-react';

import { useWorldFindings } from '@/api/queries';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { EmptyState, ErrorState, LoadingState } from '@/components/ui/states';
import { cn } from '@/lib/utils';
import { formatDateTime, shortId } from '@/lib/format';
import type { WorldFindingsListResponse } from '@42ch/nexus-contracts';

import { WorldSeverityBadge } from './world-severity-badge';

type WorldFinding = WorldFindingsListResponse['findings'][number];

export function WorldFindingsPanel({ worldId }: { worldId: string }) {
  const { t } = useTranslation('worldFindings');
  const findings = useWorldFindings(worldId);

  return (
    <Card className="shadow-card" data-testid="world-findings-panel">
      <CardHeader>
        <div className="flex flex-wrap items-start justify-between gap-2">
          <div>
            <CardTitle>{t('panel.title')}</CardTitle>
            <CardDescription>{t('panel.description')}</CardDescription>
          </div>
          {findings.data && findings.data.findings.length > 0 ? (
            <span className="text-copy-13 text-gray-700" data-testid="world-findings-count">
              {t('panel.count', { count: findings.data.findings.length })}
            </span>
          ) : null}
        </div>
      </CardHeader>
      <CardContent className="flex flex-col gap-3">
        <p className="text-copy-13 text-gray-700" data-testid="world-findings-structural-note">
          {t('panel.structuralNote')}
        </p>
        {findings.isLoading ? (
          <LoadingState label={t('panel.loading')} />
        ) : findings.isError ? (
          <ErrorState
            title={t('panel.errorTitle')}
            description={t('panel.errorDescription')}
            onRetry={() => void findings.refetch()}
          />
        ) : !findings.data || findings.data.findings.length === 0 ? (
          <EmptyState
            title={t('panel.emptyTitle')}
            description={t('panel.emptyDescription')}
          />
        ) : (
          <div className="flex flex-col gap-3">
            {findings.data.truncated ? (
              <p
                className="rounded-control border border-gray-alpha-300 bg-gray-alpha-100 px-3 py-2 text-copy-13 text-gray-900"
                data-testid="world-findings-truncated"
              >
                {t('panel.truncated')}
              </p>
            ) : null}
            <ul className="flex flex-col gap-2">
              {findings.data.findings.map((finding) => (
                <WorldFindingRow key={finding.finding_id} finding={finding} />
              ))}
            </ul>
          </div>
        )}
      </CardContent>
    </Card>
  );
}

/**
 * One finding row. Collapsed: severity badge (spoke verbatim) + kind (open
 * string verbatim) + title + shortened target entry id. Expanded (read-only
 * toggle): full description, full target entry id, created timestamp. No
 * interactive affordance writes — `aria-expanded` toggles display only.
 */
function WorldFindingRow({ finding }: { finding: WorldFinding }) {
  const { t } = useTranslation('worldFindings');
  const [expanded, setExpanded] = useState(false);

  return (
    <li className="rounded-control border border-gray-alpha-300 bg-background-100">
      <button
        type="button"
        onClick={() => setExpanded((e) => !e)}
        aria-expanded={expanded}
        aria-label={
          expanded
            ? t('panel.collapseAria', { id: finding.finding_id })
            : t('panel.expandAria', { id: finding.finding_id })
        }
        className="flex w-full items-center gap-2 rounded-control px-3 py-2 text-left hover:bg-gray-alpha-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-700 focus-visible:ring-offset-2"
        data-testid="world-finding-row"
      >
        <WorldSeverityBadge severity={finding.severity} />
        {finding.kind ? (
          <span className="text-copy-13 text-gray-700" data-testid="world-finding-kind">
            {finding.kind}
          </span>
        ) : null}
        <span className="min-w-0 flex-1 truncate text-copy-14 text-gray-1000">
          {finding.title}
        </span>
        {finding.target_entry_id ? (
          <span className="text-label-12-mono text-gray-700" data-testid="world-finding-target">
            {shortId(finding.target_entry_id)}
          </span>
        ) : null}
        <ChevronDown
          className={cn('h-4 w-4 shrink-0 text-gray-700 transition-transform duration-state', expanded && 'rotate-180')}
          aria-hidden
        />
      </button>
      {expanded ? (
        <div className="flex flex-col gap-2 border-t border-gray-alpha-300 px-3 py-2" data-testid="world-finding-detail">
          <p className="text-copy-13 text-gray-800">{finding.description}</p>
          <dl className="flex flex-wrap gap-x-6 gap-y-1 text-copy-13">
            {finding.target_entry_id ? (
              <div className="flex items-baseline gap-1">
                <dt className="text-gray-700">{t('panel.targetEntry')}</dt>
                <dd className="text-label-12-mono text-gray-900" data-testid="world-finding-target-full">
                  {finding.target_entry_id}
                </dd>
              </div>
            ) : null}
            {finding.created_at ? (
              <div className="flex items-baseline gap-1">
                <dt className="text-gray-700">{t('panel.created')}</dt>
                <dd className="text-gray-900">{formatDateTime(finding.created_at)}</dd>
              </div>
            ) : null}
          </dl>
        </div>
      ) : null}
    </li>
  );
}
