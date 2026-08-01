/**
 * Run Studio — shared app component for the Compute Run author journey
 * (V1.147 P1, behavior spec §1 / §2 / §3 / §4 / §6). Thin app wiring over the
 * promoted `@42ch/nexus-ui` primitives: this component owns t() copy, data
 * hooks, and callbacks only — no form/proposal/table logic is re-implemented
 * here.
 *
 * Flow: World → guided form (pickers filtered by `required_key_block_types`)
 * → Run → proposal inspector → Accept / Discard (confirm) → Runs history.
 *
 * V1.147 P2 T3 — extracted from `modules-page.tsx` (absorbs qc1 W-004, the
 * deferred hook extraction) so the Timeline's Run Module entry can mount the
 * SAME studio with context pre-fill — the plan's "no parallel studio" hard
 * gate. `initialWorldId` pre-fills the World selector (Timeline entry:
 * World/branch pre-filled per spec §1 P2); `initialRunId` deep-links the run
 * inspector (compute node "Open Run" → Settings → Modules run detail).
 * Selected-event context is intentionally NOT pre-filled (the entry is a
 * fresh-run context; forcing event context would violate the "don't force
 * it" constraint — spec §1 P2 optional, plan brief).
 *
 * Branch scope: the studio sends no `branch_id` on Run — the daemon defaults
 * to the World's current branch (root fallback), which matches the World
 * Timeline's world-state source.
 */
import { useEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import type { TFunction } from 'i18next';
import { Play } from 'lucide-react';

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

import { Button } from '@/components/ui/button';
import { Dialog, DialogContent } from '@/components/ui/dialog';
import { Label } from '@/components/ui/label';
import { ErrorState, LoadingState } from '@/components/ui/states';
import { Select } from '@/components/ui/select';
import {
  flattenPages,
  useAcceptRun,
  useClearRuns,
  useComputeRun,
  useComputeRuns,
  useDiscardRun,
  useNarrativeWorlds,
  useRunCompute,
} from '@/api/queries';
import { toInvocationSchema } from '@/api/run-studio-schemas';
import { useWorldKbGraph } from '@/lib/canvas/use-world-kb-data';
import { formatDateTime, shortId } from '@/lib/format';
import { useToast } from '@/lib/use-toast';

export interface RunStudioProps {
  /** Module detail — the studio is module-scoped (spec §1 P1). */
  module: ModuleDetail;
  /**
   * V1.147 P2 T3 — pre-filled World (Timeline Run Module entry). Omitted /
   * empty → the explicit World picker (spec §3: "otherwise explicit
   * picker").
   */
  initialWorldId?: string;
  /**
   * V1.147 P2 T3 — deep-linked run id (compute node "Open Run"). The run
   * inspector opens immediately; status flips (Needs review → Applied /
   * Discarded) render from the shared run-detail query.
   */
  initialRunId?: string;
  /**
   * V1.147 P2 T3 — fired when the author opens a run from the Runs table
   * (or the deep-linked inspector lands). The host writes the selection
   * back to the URL (`?module=<id>&run=<runId>`) so refresh keeps the
   * detail. Optional — the studio works without URL write-back.
   */
  onRunOpen?: (runId: string) => void;
}

/** Status filter options for the Runs history (spec §4 + plan Global
 * Constraints "module/world/status filter"). Values are the wire statuses the
 * daemon matches against the persisted row; labels are the product status
 * names (Needs review = `succeeded`). */
const RUN_STATUS_FILTER_OPTIONS: { value: RunStatus; labelKey: string }[] = [
  { value: 'running', labelKey: 'run.runsStatus.running' },
  { value: 'succeeded', labelKey: 'run.runsStatus.needsReview' },
  { value: 'applied', labelKey: 'run.runsStatus.applied' },
  { value: 'discarded', labelKey: 'run.runsStatus.discarded' },
  { value: 'failed', labelKey: 'run.runsStatus.failed' },
];

export function RunStudio({ module, initialWorldId, initialRunId, onRunOpen }: RunStudioProps) {
  const { t } = useTranslation('modules');
  const { toast } = useToast();

  // ── World scope ────────────────────────────────────────────────────────────
  // No active-World shell state exists for the Settings surface, so the World
  // selector defaults to the explicit picker (behavior spec §3: "otherwise
  // explicit picker") — or the Timeline pre-fill when the entry came from the
  // World Timeline (V1.147 P2 T3). KnowledgeEntry pickers re-derive when it
  // changes.
  const worlds = useNarrativeWorlds();
  const [worldId, setWorldId] = useState(initialWorldId ?? '');
  const worldGraph = useWorldKbGraph(worldId || undefined);

  // V1.147 PR #194 — deep-link `?world=` re-sync. The Settings Modules
  // section stays mounted when a second Timeline "Run Module" entry lands on
  // the same route (`/settings/modules?module=…&world=…` — search-params-only
  // navigation), so the `useState(initialWorldId ?? '')` seed alone would
  // keep the previous World selected — a Run would execute against the wrong
  // World. The ref tracks the last-seen prop VALUE: a new value switches the
  // selector (and `?world=` removal resets it to the empty placeholder),
  // while an unchanged prop never clobbers a World the author picked manually
  // (mirror of the round-1 `initialRunId` re-sync pattern).
  //
  // Round 3 — a deep-link switch must also reset the invocation authoring
  // state exactly like the manual `handleWorldChange`: otherwise the entity
  // IDs / params filled for the previous World would be submitted against the
  // new World. Inspector parity is intentional — the manual path does not
  // touch the run inspector, so this does not either (`initialRunId` re-sync
  // owns run-inspector changes).
  const lastWorldSeedRef = useRef(initialWorldId);
  useEffect(() => {
    if (initialWorldId !== lastWorldSeedRef.current) {
      lastWorldSeedRef.current = initialWorldId;
      setWorldId(initialWorldId ?? '');
      resetInvocation();
    }
  }, [initialWorldId]);

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

  /** Reset the invocation authoring state (guided values + Advanced JSON) to
   * pristine — shared by the manual World switch and the deep-link `?world=`
   * re-sync (PR #194 round 3) so a Run can never submit one World's params
   * against another World. The run inspector is intentionally untouched
   * (manual-path parity; `initialRunId` re-sync owns inspector changes). */
  function resetInvocation() {
    setValues({});
    setJsonText('{}');
    setJsonDirty(false);
    setJsonError(null);
  }

  function handleWorldChange(next: string) {
    setWorldId(next);
    resetInvocation();
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
  const clearRuns = useClearRuns();

  // ── Clear history (V1.147 P3 T2) ─────────────────────────────────────────
  // Server-backed `DELETE /runs?world_id=` (plan Clear-history lock): the
  // daemon REQUIRES a World scope, so the affordance is gated on the World
  // filter — no world-wide purge is possible. Terminal runs only
  // (applied|discarded|failed); running / needs-review rows are kept.
  const [clearTargetWorldId, setClearTargetWorldId] = useState<string | null>(null);
  const clearScopeWorldId = runWorldFilter !== '' ? runWorldFilter : null;

  // ── Inspector state ────────────────────────────────────────────────────────
  // Fresh runs render from the POST response (carries the truncated flag);
  // runs reopened from history render from the detail query. Accept/Discard
  // move the inspector onto the detail query so the status flip (Needs review
  // → Applied/Discarded) renders from refetched data.
  //
  // V1.147 P2 T3 — `initialRunId` deep-links the run inspector (compute node
  // "Open Run" → Settings → Modules run detail).
  const [latestRun, setLatestRun] = useState<RunResponse | null>(null);
  const [inspectorRunId, setInspectorRunId] = useState<string | null>(
    initialRunId ?? null,
  );
  const [discardTargetId, setDiscardTargetId] = useState<string | null>(null);

  // V1.147 PR #194 — deep-link `?run=` re-sync. The Settings Modules section
  // stays mounted when a second compute-node "Open Run" navigates to the same
  // route (`/settings/modules?module=…&run=…` — search params only), so the
  // `useState(initialRunId ?? null)` seed alone would keep the stale run open.
  // Keyed on the prop VALUE: a new `initialRunId` switches the inspector;
  // removing `?run=` (module switch deletes the param) closes it. The effect
  // never fires from a render alone, so the POST-success fresh-run inspector
  // (`latestRun` after `setInspectorRunId(null)`) is not clobbered while the
  // prop stays unchanged.
  useEffect(() => {
    setInspectorRunId(initialRunId ?? null);
    setLatestRun(null);
  }, [initialRunId]);
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

  function handleClearConfirmed() {
    if (!clearTargetWorldId) return;
    const worldId = clearTargetWorldId;
    setClearTargetWorldId(null);
    clearRuns.mutate(
      { worldId },
      {
        onSuccess: (res) => {
          toast({ variant: 'success', title: t('run.clearToast', { count: res.deleted }) });
          // A cleared (terminal) run open in the inspector no longer exists —
          // close the inspector honestly; needs-review/running rows survive
          // Clear and stay open.
          if (
            inspectorRun &&
            (inspectorRun.status === 'applied' ||
              inspectorRun.status === 'discarded' ||
              inspectorRun.status === 'failed')
          ) {
            setInspectorRunId(null);
            setLatestRun(null);
          }
        },
      },
    );
  }

  function openRun(runId: string) {
    setInspectorRunId(runId);
    setLatestRun(null);
    onRunOpen?.(runId);
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
            {/* Clear history (V1.147 P3 T2) — server-backed DELETE. The
                daemon REQUIRES `world_id`, so the button is gated on the
                World filter (never a world-wide purge); running / needs-review
                rows are always kept. */}
            <div className="flex items-end">
              <Button
                type="button"
                variant="secondary"
                size="small"
                onClick={() => {
                  if (clearScopeWorldId) setClearTargetWorldId(clearScopeWorldId);
                }}
                disabled={clearScopeWorldId === null || clearRuns.isPending}
                title={clearScopeWorldId === null ? t('run.clearDisabledHint') : undefined}
                aria-label={t('run.clearHistory')}
                data-testid="run-runs-clear"
              >
                {t('run.clearHistory')}
              </Button>
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

      <ClearHistoryDialog
        worldId={clearTargetWorldId}
        worldTitle={clearTargetWorldId ? (worldTitleById.get(clearTargetWorldId) ?? clearTargetWorldId) : undefined}
        open={clearTargetWorldId !== null}
        pending={clearRuns.isPending}
        onCancel={() => setClearTargetWorldId(null)}
        onConfirm={handleClearConfirmed}
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

/**
 * Destructive Clear-history confirmation (V1.147 P3 T2, behavior spec §4
 * Retain / clear). The daemon deletes terminal runs (applied|discarded|
 * failed) for the selected World; running / needs-review rows are kept. The
 * confirm names the World and the permanent nature of the delete — Applied
 * runs are included with the stronger confirm copy per §4.
 */
function ClearHistoryDialog({
  worldId,
  worldTitle,
  open,
  pending,
  onCancel,
  onConfirm,
}: {
  worldId: string | null;
  worldTitle: string | undefined;
  open: boolean;
  pending: boolean;
  onCancel: () => void;
  onConfirm: () => void;
}) {
  const { t } = useTranslation('modules');
  return (
    <Dialog open={open} onOpenChange={(next) => !next && onCancel()}>
      {open && (
        <DialogContent
          title={t('run.clearConfirmTitle')}
          description={t('run.clearConfirmDescription', { world: worldTitle ?? worldId })}
        >
          <p className="text-copy-14 text-gray-900">{t('run.clearConfirmWarning')}</p>
          <div className="mt-4 flex justify-end gap-2">
            <Button type="button" variant="secondary" size="small" onClick={onCancel}>
              {t('run.cancel')}
            </Button>
            <Button
              type="button"
              variant="primary"
              size="small"
              onClick={onConfirm}
              disabled={pending}
            >
              {t('run.clearConfirmButton')}
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
