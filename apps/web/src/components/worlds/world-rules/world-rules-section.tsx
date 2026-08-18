/**
 * World Rules section — V1.166 P2 (DR-64 surfacing half, Task 2) extended by
 * V1.169 P2 T1 with the inline create form (DF-82).
 *
 * Rules section mounted BELOW the findings panel on the world findings page
 * (PD-2). Consumes `GET /v1/daemon/worlds/:world_id/rules` via
 * {@link useWorldRules} with the generated `WorldRulesListResponse` types;
 * the inline {@link RuleForm} (create mode) posts through the V1.169 P1
 * write route and invalidates the list on success (new row in read order).
 *
 * Vocabulary contract (PD-1/PD-2, locked): status renders spoke
 * `draft|active|deprecated` verbatim plus an open-string fallback — ALL
 * statuses are visible so authors see what auto-include skips (only `active`
 * auto-includes); `kind` and `severity_hint` render verbatim. Each rule shows
 * a constraint-carrier summary rendered defensively from the DTO's first-class
 * `constraint` object (AR-2/AR-3 — unknown family never crashes, absent
 * constraint → no summary row). List order is exactly what the API returns
 * (`canonical_name ASC, rule_id ASC`); `truncated: true` renders honest copy
 * mirroring the AR-3 500-cap.
 *
 * Authoring: CardHeader **Add rule** CTA (same CTA as the empty state) opens
 * one inline create form at a time; rows stay read-only expand toggles (edit
 * + Deactivate land in T2). No modal/Dialog, no raw-JSON editor (plan locks).
 */
import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { ChevronDown, Plus } from 'lucide-react';

import { useWorldRules } from '@/api/queries';
import { WorldSeverityBadge } from '@/components/worlds/world-findings/world-severity-badge';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { EmptyState, ErrorState, LoadingState } from '@/components/ui/states';
import { cn } from '@/lib/utils';
import type { WorldRulesListResponse } from '@42ch/nexus-contracts';

import { renderConstraintSummary } from './constraint-summary';
import { RuleForm } from './rule-form';
import { WorldRuleStatusBadge } from './world-rule-status-badge';

type WorldRule = WorldRulesListResponse['rules'][number];

export function WorldRulesSection({ worldId }: { worldId: string }) {
  const { t } = useTranslation('worldRules');
  const rules = useWorldRules(worldId);
  const [creating, setCreating] = useState(false);

  return (
    <Card className="shadow-card" data-testid="world-rules-section">
      <CardHeader>
        <div className="flex flex-wrap items-start justify-between gap-2">
          <div>
            <CardTitle>{t('section.title')}</CardTitle>
            <CardDescription>{t('section.description')}</CardDescription>
          </div>
          <div className="flex items-center gap-2">
            {rules.data && rules.data.rules.length > 0 ? (
              <span className="text-copy-13 text-gray-700" data-testid="world-rules-count">
                {t('section.count', { count: rules.data.rules.length })}
              </span>
            ) : null}
            <Button
              type="button"
              variant="secondary"
              size="small"
              onClick={() => setCreating(true)}
              data-testid="world-rules-add-rule"
            >
              <Plus className="h-4 w-4" aria-hidden /> {t('section.addRule')}
            </Button>
          </div>
        </div>
      </CardHeader>
      <CardContent className="flex flex-col gap-3">
        <p className="text-copy-13 text-gray-700" data-testid="world-rules-structural-note">
          {t('section.structuralNote')}
        </p>
        {creating ? <RuleForm worldId={worldId} onClose={() => setCreating(false)} /> : null}
        {rules.isLoading ? (
          <LoadingState label={t('section.loading')} />
        ) : rules.isError ? (
          <ErrorState
            title={t('section.errorTitle')}
            description={t('section.errorDescription')}
            onRetry={() => void rules.refetch()}
          />
        ) : !rules.data || rules.data.rules.length === 0 ? (
          <EmptyState
            title={t('section.emptyTitle')}
            description={t('section.emptyDescription')}
            action={
              <Button
                type="button"
                variant="secondary"
                size="small"
                onClick={() => setCreating(true)}
                data-testid="world-rules-empty-add-rule"
              >
                <Plus className="h-4 w-4" aria-hidden /> {t('section.addRule')}
              </Button>
            }
          />
        ) : (
          <div className="flex flex-col gap-3">
            {rules.data.truncated ? (
              <p
                className="rounded-control border border-gray-alpha-300 bg-gray-alpha-100 px-3 py-2 text-copy-13 text-gray-900"
                data-testid="world-rules-truncated"
              >
                {t('section.truncated')}
              </p>
            ) : null}
            <ul className="flex flex-col gap-2">
              {rules.data.rules.map((rule) => (
                <WorldRuleRow key={rule.rule_id} rule={rule} />
              ))}
            </ul>
          </div>
        )}
      </CardContent>
    </Card>
  );
}

/**
 * One rule row. Collapsed: status badge (spoke verbatim) + canonical name +
 * kind (open string verbatim) + severity hint badge (T1 token mapping).
 * Expanded (read-only toggle): the human `statement` plus a definition list
 * with the constraint-carrier summary and the target entry types. No
 * interactive affordance writes — `aria-expanded` toggles display only.
 */
function WorldRuleRow({ rule }: { rule: WorldRule }) {
  const { t } = useTranslation('worldRules');
  const [expanded, setExpanded] = useState(false);
  const constraintSummary = renderConstraintSummary(rule.constraint);

  return (
    <li className="rounded-control border border-gray-alpha-300 bg-background-100">
      <button
        type="button"
        onClick={() => setExpanded((e) => !e)}
        aria-expanded={expanded}
        aria-label={
          expanded ? t('section.collapseAria', { id: rule.rule_id }) : t('section.expandAria', { id: rule.rule_id })
        }
        className="flex w-full items-center gap-2 rounded-control px-3 py-2 text-left hover:bg-gray-alpha-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-700 focus-visible:ring-offset-2"
        data-testid="world-rule-row"
      >
        {rule.status ? <WorldRuleStatusBadge status={rule.status} /> : null}
        <span className="min-w-0 flex-1 truncate text-copy-14 text-gray-1000">
          {rule.canonical_name}
        </span>
        {rule.kind ? (
          <span className="text-copy-13 text-gray-700" data-testid="world-rule-kind">
            {rule.kind}
          </span>
        ) : null}
        {rule.severity_hint ? <WorldSeverityBadge severity={rule.severity_hint} /> : null}
        <ChevronDown
          className={cn('h-4 w-4 shrink-0 text-gray-700 transition-transform duration-state', expanded && 'rotate-180')}
          aria-hidden
        />
      </button>
      {expanded ? (
        <div className="flex flex-col gap-2 border-t border-gray-alpha-300 px-3 py-2" data-testid="world-rule-detail">
          {rule.statement ? <p className="text-copy-13 text-gray-800">{rule.statement}</p> : null}
          <dl className="flex flex-wrap gap-x-6 gap-y-1 text-copy-13">
            {constraintSummary ? (
              <div className="flex items-baseline gap-1">
                <dt className="text-gray-700">{t('section.constraint')}</dt>
                <dd className="text-label-12-mono text-gray-900" data-testid="world-rule-constraint">
                  {constraintSummary}
                </dd>
              </div>
            ) : null}
            <div className="flex items-baseline gap-1">
              <dt className="text-gray-700">{t('section.targetTypes')}</dt>
              <dd className="text-gray-900" data-testid="world-rule-target-types">
                {rule.target_entry_types.length > 0
                  ? rule.target_entry_types.join(', ')
                  : t('section.allEntryTypes')}
              </dd>
            </div>
          </dl>
        </div>
      ) : null}
    </li>
  );
}
