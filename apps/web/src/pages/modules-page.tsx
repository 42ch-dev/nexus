/**
 * Compute Modules body (Control Room — READ + Run) — V1.114 P2 T4 / V1.131 P2 /
 * V1.147 P1 T3.
 *
 * List/detail/query/error live here once. Settings modal mounts the body as
 * the `modules` section; `/modules` is a compatibility redirect only.
 *
 * V1.147 P1: the detail panel gains the Run Studio — World selector, guided
 * form (manifest `schemas.invocation` → first-class controls), Advanced JSON
 * disclosure, Run → proposal inspector with Accept/Discard, and Runs history.
 * All form/proposal/runs chrome is thin app wiring over promoted
 * `@42ch/nexus-ui` primitives; copy, data, and callbacks stay app-owned.
 */
import { Cpu, Play, RefreshCw } from 'lucide-react';
import { useEffect, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import type { TFunction } from 'i18next';
import { Navigate } from 'react-router-dom';

import {
  ProposalSections,
  RunFormFields,
  RunStatusBadge,
  RunsTable,
  type EntityPickerEntry,
  type ProposalSectionsCopy,
  type RunFormCopy,
  type RunStatus,
  type RunsTableCopy,
  type RunTableRow,
} from '@42ch/nexus-ui';
import type { ModuleDetail, RunDetail, RunResponse } from '@42ch/nexus-contracts';

import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Dialog, DialogContent } from '@/components/ui/dialog';
import { Label } from '@/components/ui/label';
import { EmptyState, ErrorState, LoadingState, UnavailableState } from '@/components/ui/states';
import { Select } from '@/components/ui/select';
import {
  flattenPages,
  useAcceptRun,
  useComputeModule,
  useComputeModules,
  useComputeRun,
  useComputeRuns,
  useDiscardRun,
  useNarrativeWorlds,
  useRunCompute,
} from '@/api/queries';
import { toInvocationSchema } from '@/api/run-studio-schemas';
import { useWorldKbGraph } from '@/lib/canvas/use-world-kb-data';
import { formatDateTime, shortId } from '@/lib/format';
import { isOrchestrationEngineUnavailable } from '@/lib/nexus/errors';
import { useToast } from '@/lib/use-toast';
import { cn } from '@/lib/utils';

/** Compatibility adapter — product entry is Settings modal `modules` section. */
export function ModulesPage() {
  return <Navigate to="/settings/modules" replace />;
}

/**
 * Status filter options for the Runs history (spec §4 + plan Global
 * Constraints "module/world/status filter"). Values are the wire statuses the
 * daemon matches against the persisted row; labels are the product status
 * names (Needs review = `succeeded`).
 */
const RUN_STATUS_FILTER_OPTIONS: { value: RunStatus; labelKey: string }[] = [
  { value: 'running', labelKey: 'run.runsStatus.running' },
  { value: 'succeeded', labelKey: 'run.runsStatus.needsReview' },
  { value: 'applied', labelKey: 'run.runsStatus.applied' },
  { value: 'discarded', labelKey: 'run.runsStatus.discarded' },
  { value: 'failed', labelKey: 'run.runsStatus.failed' },
];

/** Shared list/detail body — reused by SettingsModulesSection (no duplicate hooks). */
export function ModulesPageBody() {
  const { t } = useTranslation('modules');
  const modules = useComputeModules();
  const [selectedId, setSelectedId] = useState<string | null>(null);

  return (
    <Card className="shadow-card" data-testid="modules-page-body">
      <CardHeader>
        <div className="flex items-center justify-between gap-2">
          <div>
            <CardTitle>{t('title')}</CardTitle>
            <CardDescription>{t('description')}</CardDescription>
          </div>
          <Button
            type="button"
            variant="tertiary"
            size="small"
            onClick={() => modules.refetch()}
            disabled={modules.isFetching}
            aria-label={t('refreshAria')}
          >
            <RefreshCw
              className={`h-4 w-4 ${modules.isFetching ? 'animate-spin' : ''}`}
              aria-hidden
            />
            {t('refresh')}
          </Button>
        </div>
      </CardHeader>
      <CardContent>
        {modules.isError ? (
          isOrchestrationEngineUnavailable(modules.error) ? (
            <UnavailableState
              title={t('engineUnavailableTitle')}
              description={t('engineUnavailableDescription')}
              onRetry={() => modules.refetch()}
            />
          ) : (
            <ErrorState
              title={t('errorTitle')}
              description={t('errorDescription')}
              onRetry={() => modules.refetch()}
            />
          )
        ) : modules.isLoading ? (
          <LoadingState label={t('loading')} />
        ) : !modules.data || modules.data.length === 0 ? (
          <EmptyState title={t('emptyTitle')} description={t('emptyDescription')} />
        ) : (
          <div className="grid grid-cols-1 gap-4 lg:grid-cols-3">
            <div className="flex flex-col gap-2 lg:col-span-1">
              <p className="text-label-12 uppercase tracking-wide text-gray-700">
                {t('listTitle')}
              </p>
              <ul className="flex flex-col gap-2" aria-label={t('listAriaLabel')}>
                {modules.data.map((m) => (
                  <li key={m.module_id}>
                    <button
                      type="button"
                      aria-label={m.name}
                      aria-pressed={selectedId === m.module_id}
                      onClick={() => setSelectedId(m.module_id)}
                      className={cn(
                        'flex w-full flex-col gap-2 rounded-card border p-4 text-left transition-colors duration-state ease-standard focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-1000 dark:focus-visible:ring-blue-700 focus-visible:ring-offset-2 focus-visible:ring-offset-background-100',
                        selectedId === m.module_id
                          ? 'border-blue-1000 bg-gray-alpha-100 dark:border-blue-700'
                          : 'border-gray-alpha-400 bg-background-100 hover:bg-background-200',
                      )}
                    >
                      <div className="flex items-center justify-between gap-2">
                        <span className="text-label-14 font-medium text-gray-1000">{m.name}</span>
                        <span className="text-copy-13-mono text-gray-700">{m.version}</span>
                      </div>
                      {m.description && (
                        <p className="text-copy-13 text-gray-900">{m.description}</p>
                      )}
                      <div className="flex flex-wrap items-center gap-1">
                        {m.required_key_block_types.map((type) => (
                          <Badge key={type} variant="neutral">
                            {type}
                          </Badge>
                        ))}
                        {m.battle_report_kind && (
                          <Badge variant="preset">{m.battle_report_kind}</Badge>
                        )}
                      </div>
                    </button>
                  </li>
                ))}
              </ul>
            </div>
            <div className="lg:col-span-2">
              {selectedId ? (
                <ModuleDetailPanel moduleId={selectedId} />
              ) : (
                <div className="flex h-full min-h-[240px] flex-col items-center justify-center gap-2 rounded-card border border-dashed border-gray-alpha-400 p-6 text-center">
                  <Cpu className="h-8 w-8 text-gray-500" aria-hidden />
                  <p className="text-heading-16 font-heading text-gray-1000">{t('selectTitle')}</p>
                  <p className="max-w-sm text-copy-14 text-gray-900">{t('selectDescription')}</p>
                </div>
              )}
            </div>
          </div>
        )}
      </CardContent>
    </Card>
  );
}

function ModuleDetailPanel({ moduleId }: { moduleId: string }) {
  const { t } = useTranslation('modules');
  const detail = useComputeModule(moduleId);

  if (detail.isLoading) {
    return <LoadingState label={t('detail.loading')} />;
  }
  if (detail.isError) {
    if (isOrchestrationEngineUnavailable(detail.error)) {
      return (
        <UnavailableState
          title={t('detail.engineUnavailableTitle')}
          description={t('detail.engineUnavailableDescription')}
          onRetry={() => detail.refetch()}
        />
      );
    }
    return (
      <ErrorState
        title={t('detail.errorTitle')}
        description={t('detail.errorDescription')}
        onRetry={() => detail.refetch()}
      />
    );
  }
  if (!detail.data) {
    return null;
  }

  const m = detail.data;

  return (
    <div className="flex flex-col gap-4 rounded-card border border-gray-alpha-400 bg-background-100 p-6">
      <p className="text-label-12 uppercase tracking-wide text-gray-700">{t('detail.title')}</p>
      <div className="flex flex-col gap-1 sm:flex-row sm:items-start sm:justify-between sm:gap-4">
        <div>
          <h2 className="text-heading-20 font-heading text-gray-1000">{m.name}</h2>
          <p className="text-copy-13-mono text-gray-700">{m.module_id}</p>
        </div>
        <Badge variant="preset">{t('detail.version', { version: m.version })}</Badge>
      </div>

      {m.description && <p className="text-copy-14 text-gray-900">{m.description}</p>}

      <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
        <DetailField label={t('detail.abiVersion')} value={m.nexus_abi_version} />
        <DetailField label={t('detail.author')} value={m.author} />
        <DetailField label={t('detail.computeExport')} value={m.compute_export} />
        <DetailField label={t('detail.initExport')} value={m.init_export} />
        {m.max_fuel !== undefined && (
          <DetailField label={t('detail.maxFuel')} value={m.max_fuel} />
        )}
        {m.max_memory_mib !== undefined && (
          <DetailField label={t('detail.maxMemory')} value={m.max_memory_mib} />
        )}
        {m.max_wall_time_ms !== undefined && (
          <DetailField label={t('detail.maxWallTime')} value={m.max_wall_time_ms} />
        )}
      </div>

      {m.required_key_block_types.length > 0 && (
        <div className="flex flex-col gap-1">
          <p className="text-label-12 uppercase tracking-wide text-gray-700">
            {t('detail.requiredKeyBlockTypes')}
          </p>
          <div className="flex flex-wrap gap-1">
            {m.required_key_block_types.map((type) => (
              <Badge key={type} variant="neutral">
                {type}
              </Badge>
            ))}
          </div>
        </div>
      )}

      {m.host_functions && m.host_functions.length > 0 && (
        <div className="flex flex-col gap-1">
          <p className="text-label-12 uppercase tracking-wide text-gray-700">
            {t('detail.hostFunctions')}
          </p>
          <div className="flex flex-wrap gap-1">
            {m.host_functions.map((fn) => (
              <Badge key={fn} variant="neutral">
                {fn}
              </Badge>
            ))}
          </div>
        </div>
      )}

      {m.battle_report_kind && (
        <DetailField label={t('detail.battleReportKind')} value={m.battle_report_kind} />
      )}

      {m.schemas && (
        <div className="flex flex-col gap-2">
          <p className="text-label-12 uppercase tracking-wide text-gray-700">
            {t('detail.schemas')}
          </p>
          <SchemaBlock
            title={t('detail.keyBlockAttributes')}
            value={m.schemas.key_block_attributes}
          />
          <SchemaBlock title={t('detail.keyBlockState')} value={m.schemas.key_block_state} />
          <SchemaBlock title={t('detail.invocation')} value={m.schemas.invocation} />
          <SchemaBlock title={t('detail.battleReport')} value={m.schemas.battle_report} />
        </div>
      )}

      <RunStudio module={m} />
    </div>
  );
}

/**
 * Run Studio — Settings → Modules detail author journey (V1.147 P1, behavior
 * spec §1 P1 / §2 / §3 / §4 / §6). Thin app wiring over the promoted
 * `@42ch/nexus-ui` primitives: this component owns t() copy, data hooks, and
 * callbacks only — no form/proposal/table logic is re-implemented here.
 *
 * Flow: World → guided form (pickers filtered by `required_key_block_types`)
 * → Run → proposal inspector → Accept / Discard (confirm) → Runs history.
 */
function RunStudio({ module }: { module: ModuleDetail }) {
  const { t } = useTranslation('modules');
  const { toast } = useToast();

  // ── World scope ────────────────────────────────────────────────────────────
  // No active-World shell state exists for the Settings surface, so the World
  // selector defaults to the explicit picker (behavior spec §3: "otherwise
  // explicit picker"). KnowledgeEntry pickers re-derive when it changes.
  const worlds = useNarrativeWorlds();
  const [worldId, setWorldId] = useState('');
  const worldGraph = useWorldKbGraph(worldId || undefined);

  // ── Guided form + Advanced JSON (best-effort sync) ────────────────────────
  // Narrow the generated open index signature at the app boundary through the
  // structural guard (qc1 W-003 fix) — the primitive consumes the structural
  // InvocationSchema fragment; shape drift is rejected instead of cast.
  const invocationSchema = toInvocationSchema(module.schemas?.invocation);

  const [values, setValues] = useState<Record<string, unknown>>({});
  const [jsonText, setJsonText] = useState('{}');
  const [jsonDirty, setJsonDirty] = useState(false);
  const [jsonError, setJsonError] = useState<string | null>(null);

  // Keep the raw-JSON disclosure in sync with guided-form edits until the
  // author touches it ("form fields sync best-effort" — JSON edits never
  // propagate back into the guided controls).
  useEffect(() => {
    if (!jsonDirty) {
      setJsonText(JSON.stringify(values, null, 2));
    }
  }, [values, jsonDirty]);

  // Parsed Advanced-JSON payload while `jsonDirty`; `null` when the text is
  // not a complete JSON object. This is the enablement source of truth for the
  // JSON path (qc2 W-002 fix): a valid complete JSON object can satisfy Run
  // even when the guided `required` fields are empty — the server/manifest
  // validation stays the source of truth on submit (behavior spec §3), and
  // the guided `required` gate only applies while the guided form is the
  // active input.
  const jsonObject = useMemo(() => {
    if (!jsonDirty) return null;
    try {
      const parsed: unknown = JSON.parse(jsonText);
      if (parsed === null || typeof parsed !== 'object' || Array.isArray(parsed)) {
        return null;
      }
      return parsed as Record<string, unknown>;
    } catch {
      return null;
    }
  }, [jsonDirty, jsonText]);

  const setField = (name: string, value: unknown) =>
    setValues((prev) => ({ ...prev, [name]: value }));

  function handleWorldChange(next: string) {
    setWorldId(next);
    setValues({});
    setJsonText('{}');
    setJsonDirty(false);
    setJsonError(null);
  }

  // ── Entity picker entries (KnowledgeEntry list from the World KB graph) ────
  const entityEntries = useMemo(() => {
    const entries: Record<string, EntityPickerEntry[]> = {};
    if (!worldGraph.data) return entries;
    const requiredTypes = new Set(module.required_key_block_types);
    const matching = worldGraph.data.entities
      .filter(
        (e) =>
          requiredTypes.has(e.block_type) && e.status !== 'deleted' && e.status !== 'rejected',
      )
      .map((e) => ({ id: e.key_block_id, title: e.canonical_name }));
    for (const name of Object.keys(invocationSchema?.properties ?? {})) {
      if (name.endsWith('_id')) {
        entries[name] = matching;
      }
    }
    return entries;
  }, [worldGraph.data, invocationSchema, module.required_key_block_types]);

  // ── Runs history (scoped to this module — context filter per spec §4) ─────
  // Filter chrome on module detail (qc1 W-005 / qc3 W-4 fix): status + World
  // narrow the module-scoped history; each filter view caches independently
  // (the filter is part of the query key). No daemon DELETE API exists for
  // Clear history in P1, so filtering is the honest affordance — a
  // server-backed Clear is registered as a residual for PM.
  const [runStatusFilter, setRunStatusFilter] = useState<'' | RunStatus>('');
  const [runWorldFilter, setRunWorldFilter] = useState('');
  const runs = useComputeRuns({
    module_id: module.module_id,
    status: runStatusFilter || undefined,
    world_id: runWorldFilter || undefined,
  });
  const runCompute = useRunCompute();
  const acceptRun = useAcceptRun();
  const discardRun = useDiscardRun();

  // ── Inspector state ────────────────────────────────────────────────────────
  // Fresh runs render from the POST response (carries the truncated flag);
  // runs reopened from history render from the detail query. Accept/Discard
  // move the inspector onto the detail query so the status flip (Needs review
  // → Applied/Discarded) renders from refetched data.
  const [latestRun, setLatestRun] = useState<RunResponse | null>(null);
  const [inspectorRunId, setInspectorRunId] = useState<string | null>(null);
  const [discardTargetId, setDiscardTargetId] = useState<string | null>(null);
  const runDetail = useComputeRun(inspectorRunId ?? undefined);
  const inspectorRun = inspectorRunId ? runDetail.data : latestRun;

  // Client mirrors the manifest's obvious `required` list (spec §3); the
  // server/manifest validation stays the source of truth on submit.
  const requiredFilled = useMemo(() => {
    const required = invocationSchema?.required ?? [];
    return required.every((name) => {
      const value = values[name];
      return value !== undefined && value !== null && value !== '';
    });
  }, [invocationSchema, values]);

  // Run enablement (qc2 W-002 fix): while the Advanced JSON path is the
  // active input (`jsonDirty`), a complete valid JSON object satisfies
  // enablement even when guided `required` fields are empty — the author's
  // JSON is what gets submitted and the server validates it on submit.
  const canRun =
    worldId !== '' &&
    (jsonDirty ? jsonObject !== null : requiredFilled) &&
    !runCompute.isPending;

  function handleRun() {
    let params: Record<string, unknown> = values;
    if (jsonDirty) {
      if (!jsonObject) {
        setJsonError(t('run.jsonInvalid'));
        return;
      }
      params = jsonObject;
    }
    setJsonError(null);
    runCompute.mutate(
      { world_id: worldId, module_id: module.module_id, invocation_params: params },
      {
        onSuccess: (res) => {
          setLatestRun(res);
          setInspectorRunId(null);
        },
      },
    );
  }

  function handleAccept() {
    const runId = inspectorRunId ?? latestRun?.run_id;
    if (!runId) return;
    // Whole-accept: Accept commits all proposals atomically (spec §2 locked
    // granularity); per-item event unchecking is the optional-if-cheap path
    // and is intentionally not surfaced here.
    acceptRun.mutate(
      { runId },
      {
        onSuccess: () => {
          toast({ variant: 'success', title: t('run.acceptedToast') });
          setInspectorRunId(runId);
          setLatestRun(null);
        },
      },
    );
  }

  function handleDiscardConfirmed() {
    if (!discardTargetId) return;
    const runId = discardTargetId;
    setDiscardTargetId(null);
    discardRun.mutate(runId, {
      onSuccess: () => {
        toast({ variant: 'success', title: t('run.discardedToast') });
        setInspectorRunId(runId);
        setLatestRun(null);
      },
    });
  }

  function openRun(runId: string) {
    setInspectorRunId(runId);
    setLatestRun(null);
  }

  // ── Row mapping for the Runs table ────────────────────────────────────────
  const worldTitleById = useMemo(() => {
    const map = new Map<string, string>();
    for (const w of worlds.data ?? []) map.set(w.world_id, w.title);
    return map;
  }, [worlds.data]);

  const runRows: RunTableRow[] = useMemo(
    () =>
      flattenPages(runs.data).map((r) => ({
        runId: r.run_id,
        moduleName: module.name,
        moduleVersion: r.module_version,
        worldTitle: worldTitleById.get(r.world_id) ?? r.world_id,
        status: r.status,
        statusLabel: runStatusLabel(t, r.status),
        startedAt: formatDateTime(r.created_at),
        finishedAt: formatDateTime(r.updated_at),
      })),
    [runs.data, worldTitleById, module.name, t],
  );

  return (
    <div className="flex flex-col gap-4 border-t border-gray-alpha-300 pt-4" data-testid="run-studio">
      <p className="text-label-12 uppercase tracking-wide text-gray-700">{t('run.title')}</p>

      {/* Product chrome: World selector (always present, spec §3). A data-feed
          failure here must not collapse into a silent empty select (qc3 W-3
          fix) — surface the ErrorState with a retry instead. */}
      {worlds.isError ? (
        <ErrorState
          title={t('run.worldsErrorTitle')}
          description={t('run.worldsErrorDescription')}
          onRetry={() => worlds.refetch()}
        />
      ) : (
        <div className="flex max-w-xs flex-col gap-1.5">
          <Label htmlFor="run-studio-world">{t('run.worldLabel')}</Label>
          <Select
            id="run-studio-world"
            value={worldId}
            onChange={(event) => handleWorldChange(event.target.value)}
            disabled={worlds.isLoading}
            data-testid="run-studio-world"
          >
            <option value="" disabled>
              {t('run.worldPlaceholder')}
            </option>
            {(worlds.data ?? []).map((w) => (
              <option key={w.world_id} value={w.world_id}>
                {w.title}
              </option>
            ))}
          </Select>
          <p className="text-copy-13 text-gray-700">
            {worldId ? t('run.branchNote') : t('run.noWorldHint')}
          </p>
        </div>
      )}

      {worldId !== '' && worldGraph.isError ? (
        <ErrorState
          title={t('run.worldGraphErrorTitle')}
          description={t('run.worldGraphErrorDescription')}
          onRetry={() => worldGraph.refetch()}
        />
      ) : worldId !== '' && worldGraph.isLoading ? (
        <LoadingState label={t('run.kbGraphLoading')} />
      ) : (
        <RunFormFields
          schema={invocationSchema}
          requiredKeyBlockTypes={module.required_key_block_types}
          values={values}
          onChange={setField}
          entityEntries={entityEntries}
          copy={runFormCopy(t)}
          idPrefix="run-form"
        />
      )}

      {/* Advanced JSON escape hatch — collapsed by default; form syncs best-effort. */}
      <details
        data-testid="run-studio-advanced-json"
        className="rounded-control border border-gray-alpha-300 bg-background-100 p-3"
      >
        <summary className="cursor-pointer text-label-14 text-gray-1000">
          {t('run.advancedJson')}
        </summary>
        <p className="mt-2 text-copy-13 text-gray-700">{t('run.advancedJsonWarning')}</p>
        <textarea
          aria-label={t('run.advancedJson')}
          value={jsonText}
          onChange={(event) => {
            setJsonText(event.target.value);
            setJsonDirty(true);
            setJsonError(null);
          }}
          spellCheck={false}
          className="mt-2 h-40 w-full rounded-control border border-gray-alpha-400 bg-background-200 p-3 font-mono text-copy-13 text-gray-1000 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-1000 dark:focus-visible:ring-blue-700"
          data-testid="run-studio-json-textarea"
        />
        {jsonError && (
          <p className="mt-2 text-copy-13 text-red-700" role="alert" data-testid="run-studio-json-error">
            {jsonError}
          </p>
        )}
      </details>

      <div className="flex items-center gap-3">
        <Button
          type="button"
          variant="primary"
          onClick={handleRun}
          disabled={!canRun}
          data-testid="run-studio-run"
        >
          <Play className="h-4 w-4" aria-hidden />
          {runCompute.isPending ? t('run.runPending') : t('run.runButton')}
        </Button>
      </div>

      {inspectorRunId && runDetail.isLoading ? (
        <LoadingState label={t('run.runLoadingDetail')} />
      ) : (
        inspectorRun && (
          <RunInspector
            run={inspectorRun}
            truncated={latestRun?.truncated ?? false}
            acceptPending={acceptRun.isPending}
            discardPending={discardRun.isPending}
            onAccept={handleAccept}
            onDiscard={() => {
              const runId = inspectorRunId ?? latestRun?.run_id;
              if (runId) setDiscardTargetId(runId);
            }}
          />
        )
      )}

      <div className="flex flex-col gap-2">
        <div className="flex flex-wrap items-end justify-between gap-2">
          <p className="text-label-12 uppercase tracking-wide text-gray-700">
            {t('run.runsTitle')}
          </p>
          {/* Filter chrome (spec §4 + plan Global Constraints). Status + World
              narrow the module-scoped history. Clear history needs a daemon
              DELETE API that does not exist in P1 — filtering ships now and a
              server-backed Clear is registered as a residual (no invented
              backend route, no disabled "coming soon" affordance). */}
          <div className="flex flex-wrap items-end gap-2">
            <div className="flex flex-col gap-1">
              <Label htmlFor="run-runs-status-filter" className="text-label-12 text-gray-700">
                {t('run.runsTable.statusFilter')}
              </Label>
              <Select
                id="run-runs-status-filter"
                value={runStatusFilter}
                onChange={(event) => setRunStatusFilter(event.target.value as '' | RunStatus)}
                className="w-44"
                data-testid="run-runs-status-filter"
              >
                <option value="">{t('run.runsTable.allStatuses')}</option>
                {RUN_STATUS_FILTER_OPTIONS.map((option) => (
                  <option key={option.value} value={option.value}>
                    {t(option.labelKey)}
                  </option>
                ))}
              </Select>
            </div>
            <div className="flex flex-col gap-1">
              <Label htmlFor="run-runs-world-filter" className="text-label-12 text-gray-700">
                {t('run.runsTable.worldFilter')}
              </Label>
              <Select
                id="run-runs-world-filter"
                value={runWorldFilter}
                onChange={(event) => setRunWorldFilter(event.target.value)}
                disabled={worlds.isLoading || worlds.isError}
                className="w-44"
                data-testid="run-runs-world-filter"
              >
                <option value="">{t('run.runsTable.allWorlds')}</option>
                {(worlds.data ?? []).map((w) => (
                  <option key={w.world_id} value={w.world_id}>
                    {w.title}
                  </option>
                ))}
              </Select>
            </div>
          </div>
        </div>

        {/* Data-feed errors must not collapse into "No runs yet" (qc3 W-3
            fix): a failed runs query renders ErrorState, and a loading list
            renders a placeholder instead of flashing the empty state. */}
        {runs.isLoading ? (
          <LoadingState label={t('run.runsLoading')} />
        ) : runs.isError ? (
          <ErrorState
            title={t('run.runsErrorTitle')}
            description={t('run.runsErrorDescription')}
            onRetry={() => runs.refetch()}
          />
        ) : (
          <RunsTable rows={runRows} copy={runsTableCopy(t)} onOpenRun={openRun} />
        )}

        {/* Cursor pagination (qc1 W-002 / qc3 W-2 fix): runs beyond the first
            page — including older Needs-review rows — must stay reachable. */}
        {runs.hasNextPage && (
          <Button
            type="button"
            variant="tertiary"
            size="small"
            onClick={() => void runs.fetchNextPage()}
            disabled={runs.isFetchingNextPage}
            data-testid="run-runs-load-more"
          >
            {runs.isFetchingNextPage ? t('run.runsTable.loadingMore') : t('run.runsTable.loadMore')}
          </Button>
        )}
      </div>

      <DiscardRunDialog
        open={discardTargetId !== null}
        onCancel={() => setDiscardTargetId(null)}
        onConfirm={handleDiscardConfirmed}
      />
    </div>
  );
}

/** Result inspector for one Run — succeeded (proposals + Accept/Discard), failed, or terminal. */
function RunInspector({
  run,
  truncated,
  acceptPending,
  discardPending,
  onAccept,
  onDiscard,
}: {
  run: RunResponse | RunDetail;
  truncated?: boolean;
  acceptPending: boolean;
  discardPending: boolean;
  onAccept: () => void;
  onDiscard: () => void;
}) {
  const { t } = useTranslation('modules');
  const isLimit =
    run.error !== undefined &&
    ['compute_fuel_exhausted', 'compute_wall_time_exceeded', 'compute_memory_cap_exceeded'].includes(
      run.error.code,
    );

  return (
    <div
      className="flex flex-col gap-4 rounded-card border border-gray-alpha-300 bg-background-100 p-4"
      data-testid="run-inspector"
    >
      <div className="flex flex-wrap items-center gap-3">
        <RunStatusBadge status={run.status} label={runStatusLabel(t, run.status)} />
        <span className="text-copy-13-mono text-gray-700">{shortId(run.run_id)}</span>
      </div>

      {run.status === 'succeeded' && run.proposals && (
        <>
          <ProposalSections proposals={run.proposals} truncated={truncated} copy={proposalCopy(t)} />
          <div className="flex flex-wrap items-center gap-3">
            <Button
              type="button"
              variant="primary"
              onClick={onAccept}
              disabled={acceptPending}
              aria-label={t('run.acceptAria')}
              data-testid="run-inspector-accept"
            >
              {t('run.accept')}
            </Button>
            <Button
              type="button"
              variant="secondary"
              onClick={onDiscard}
              disabled={discardPending}
              aria-label={t('run.discardAria')}
              data-testid="run-inspector-discard"
            >
              {t('run.discard')}
            </Button>
            <p className="text-copy-13 text-gray-700">{t('run.acceptNote')}</p>
          </div>
        </>
      )}

      {run.status === 'failed' && run.error && (
        <div
          data-testid="run-inspector-failed"
          className="rounded-card border border-error-surface-border bg-error-surface p-4"
        >
          <p className="text-label-14 font-medium text-gray-1000">
            {isLimit ? t('run.stoppedTitle') : t('run.failedTitle')}
          </p>
          <p className="mt-1 text-copy-13 text-gray-700">
            {isLimit
              ? t('run.stoppedDescription')
              : t('run.failedDescription', { reason: run.error.message ?? '' })}
          </p>
          <p className="mt-2 text-copy-13-mono text-gray-700">
            {t('run.failedCodeLabel')}: {run.error.code}
          </p>
        </div>
      )}

      {run.status === 'applied' && (
        <p className="text-copy-13 text-gray-700" data-testid="run-inspector-applied-note">
          {t('run.appliedNote')}
        </p>
      )}
      {run.status === 'discarded' && (
        <p className="text-copy-13 text-gray-700" data-testid="run-inspector-discarded-note">
          {t('run.discardedNote')}
        </p>
      )}
      {run.status === 'running' && (
        <p className="text-copy-13 text-gray-700" data-testid="run-inspector-running-note">
          {t('run.runningNote')}
        </p>
      )}
    </div>
  );
}

/** Destructive Discard confirmation (spec §2 — Discard is always author-confirmed). */
function DiscardRunDialog({
  open,
  onCancel,
  onConfirm,
}: {
  open: boolean;
  onCancel: () => void;
  onConfirm: () => void;
}) {
  const { t } = useTranslation('modules');
  return (
    <Dialog open={open} onOpenChange={(next) => !next && onCancel()}>
      {open && (
        <DialogContent
          title={t('run.discardConfirmTitle')}
          description={t('run.discardConfirmDescription')}
        >
          <p className="text-copy-14 text-gray-900">{t('run.discardConfirmWarning')}</p>
          <div className="mt-4 flex justify-end gap-2">
            <Button type="button" variant="secondary" size="small" onClick={onCancel}>
              {t('run.cancel')}
            </Button>
            <Button type="button" variant="primary" size="small" onClick={onConfirm}>
              {t('run.discardConfirmButton')}
            </Button>
          </div>
        </DialogContent>
      )}
    </Dialog>
  );
}

/* ── Caller-owned copy builders (i18n lives in the app, not the package) ───── */

function runFormCopy(t: TFunction): RunFormCopy {
  return {
    emptyTitle: t('run.form.emptyTitle'),
    emptyDescription: t('run.form.emptyDescription'),
    unsupportedFieldNote: t('run.form.unsupportedFieldNote'),
    entityPlaceholder: t('run.form.entityPlaceholder'),
    selectPlaceholder: t('run.form.selectPlaceholder'),
    entityEmptyTitle: t('run.form.entityEmptyTitle'),
    entityEmptyDescription: t('run.form.entityEmptyDescription'),
  };
}

function proposalCopy(t: TFunction): ProposalSectionsCopy {
  return {
    reportTitle: t('run.proposals.reportTitle'),
    knowledgeUpdatesTitle: t('run.proposals.knowledgeUpdatesTitle'),
    timelineEventsTitle: t('run.proposals.timelineEventsTitle'),
    newKnowledgeTitle: t('run.proposals.newKnowledgeTitle'),
    truncatedNote: t('run.proposals.truncatedNote'),
    untitledEventLabel: t('run.proposals.untitledEventLabel'),
    affectedEntriesLabel: (count: number) => t('run.proposals.affectedEntries', { count }),
    newEntryLabel: t('run.proposals.newEntryLabel'),
  };
}

function runsTableCopy(t: TFunction): RunsTableCopy {
  return {
    moduleColumn: t('run.runsTable.moduleColumn'),
    worldColumn: t('run.runsTable.worldColumn'),
    statusColumn: t('run.runsTable.statusColumn'),
    startedColumn: t('run.runsTable.startedColumn'),
    finishedColumn: t('run.runsTable.finishedColumn'),
    runIdColumn: t('run.runsTable.runIdColumn'),
    openRunLabel: t('run.runsTable.openRunLabel'),
    copyIdLabel: t('run.runsTable.copyIdLabel'),
    emptyTitle: t('run.runsTable.emptyTitle'),
    emptyDescription: t('run.runsTable.emptyDescription'),
  };
}

/** Product status labels per behavior spec §4 (caller-owned, never in the package). */
function runStatusLabel(t: TFunction, status: RunStatus): string {
  switch (status) {
    case 'running':
      return t('run.runsStatus.running');
    case 'succeeded':
      return t('run.runsStatus.needsReview');
    case 'failed':
      return t('run.runsStatus.failed');
    case 'applied':
      return t('run.runsStatus.applied');
    case 'discarded':
      return t('run.runsStatus.discarded');
  }
}

function DetailField({ label, value }: { label: string; value: string | number | undefined }) {
  if (value === undefined || value === null || value === '') {
    return null;
  }

  return (
    <div className="flex flex-col gap-1">
      <p className="text-label-12 uppercase tracking-wide text-gray-700">{label}</p>
      <p className="text-copy-14 text-gray-1000">{value}</p>
    </div>
  );
}

function SchemaBlock({ title, value }: { title: string; value: Record<string, unknown> | undefined }) {
  return (
    <div className="flex flex-col gap-1">
      <p className="text-label-12 uppercase tracking-wide text-gray-700">{title}</p>
      <pre className="overflow-x-auto rounded-control bg-background-300 p-3 text-copy-13-mono text-gray-900">
        {value ? JSON.stringify(value, null, 2) : '—'}
      </pre>
    </div>
  );
}
