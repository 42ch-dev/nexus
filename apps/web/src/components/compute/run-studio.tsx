/**
 * Run Studio — shared app component for the Compute Run author journey
 * (V1.147 P1, behavior spec §1 / §2 / §3 / §4 / §6).
 *
 * Flow: World → guided form (pickers filtered by `required_key_block_types`)
 * → Run → proposal inspector → Accept / Discard (confirm) → Runs history.
 *
 * V1.147 P2 T3 — extracted from the modules page body (absorbs qc1 W-004, the
 * deferred hook extraction) so the Timeline's Run Module entry can mount the
 * SAME studio with context pre-fill — the plan's "no parallel studio" hard
 * gate. `initialWorldId` pre-fills the World selector (Timeline entry:
 * World/branch pre-filled per spec §1 P2); `initialRunId` deep-links the run
 * inspector (compute node "Open Run" → `/modules` run detail).
 * Selected-event context is intentionally NOT pre-filled (the entry is a
 * fresh-run context; forcing event context would violate the "don't force
 * it" constraint — spec §1 P2 optional, plan brief).
 *
 * Branch scope (Bugbot 1): the studio threads the Timeline entry's `?branch=`
 * pre-fill into the Run request as `branch_id`, so compute run/accept writes
 * the forked branch — not the World root. Omitted when the entry came from
 * the root Timeline (no `?branch=`): the daemon defaults to the World's
 * current branch (root fallback), matching the root Timeline's source.
 *
 * v221-modules-rail T2 — the markup moved to
 * `components/developer/presentational/run-studio-chrome.tsx` (pure view, no
 * transport / router / i18n) so the Design Studio fixture can render it with
 * static copy. This file keeps every hook, query, mutation, toast and i18n
 * lookup, and mounts the two destructive confirm dialogs as siblings of the
 * chrome. Public props are unchanged.
 */
import { useEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import type { TFunction } from 'i18next';

import type {
  EntityPickerEntry,
  ProposalSectionsCopy,
  RunFormCopy,
  RunStatus,
  RunsTableCopy,
  RunTableRow,
} from '@42ch/nexus-ui';
import type { ModuleDetail, RunResponse } from '@42ch/nexus-contracts';

import { Button } from '@/components/ui/button';
import { Dialog, DialogContent } from '@/components/ui/dialog';
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
import { RunStudioChrome, type RunStudioCopy } from '@/components/developer/presentational/run-studio-chrome';
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
   * Bugbot 1 — pre-filled fork branch (Timeline Run Module entry from a
   * forked-branch view, `?branch=`). Threaded into the Run request as
   * `branch_id`; omitted/undefined → the daemon defaults to the World's
   * current branch (root).
   */
  initialBranchId?: string;
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

export function RunStudio({
  module,
  initialWorldId,
  initialBranchId,
  initialRunId,
  onRunOpen,
}: RunStudioProps) {
  const { t } = useTranslation('modules');
  const { toast } = useToast();

  // ── World scope ────────────────────────────────────────────────────────────
  // No active-World shell state exists for the modules surface, so the World
  // selector defaults to the explicit picker (behavior spec §3: "otherwise
  // explicit picker") — or the Timeline pre-fill when the entry came from the
  // World Timeline (V1.147 P2 T3). KnowledgeEntry pickers re-derive when it
  // changes.
  const worlds = useNarrativeWorlds();
  const [worldId, setWorldId] = useState(initialWorldId ?? '');
  const worldGraph = useWorldKbGraph(worldId || undefined);

  // Bugbot 1 — `?branch=` pre-fill re-sync, mirroring the `?world=` pattern
  // below: the Modules page stays mounted across Timeline "Run Module"
  // entries (search-params-only navigation), so the state seed alone would
  // keep the previous fork's branch on the next run request — a Run would
  // write the wrong branch. Keyed on the prop VALUE with a ref guard: a new
  // `?branch=` switches the scoped branch, `?branch=` removal resets to
  // undefined (world root), and an unchanged prop never clobbers state. The
  // invocation values are NOT reset here — fork branches share the World's
  // KB, so params remain valid within the same World (the `?world=` re-sync
  // below owns cross-World resets).
  const [branchId, setBranchId] = useState(initialBranchId ?? undefined);
  const lastBranchSeedRef = useRef(initialBranchId);
  useEffect(() => {
    if (initialBranchId !== lastBranchSeedRef.current) {
      lastBranchSeedRef.current = initialBranchId;
      setBranchId(initialBranchId ?? undefined);
    }
  }, [initialBranchId]);

  // V1.147 PR #194 — deep-link `?world=` re-sync. The Modules page stays
  // mounted when a second Timeline "Run Module" entry lands on the same route
  // (`/modules?module=…&world=…` — search-params-only navigation), so the
  // `useState(initialWorldId ?? '')` seed alone would keep the previous World
  // selected — a Run would execute against the wrong World. The ref tracks
  // the last-seen prop VALUE: a new value switches the selector (and
  // `?world=` removal resets it to the empty placeholder), while an unchanged
  // prop never clobbers a World the author picked manually (mirror of the
  // round-1 `initialRunId` re-sync pattern).
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
  const [advancedOpen, setAdvancedOpen] = useState(false);

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
  // "Open Run" → `/modules` run detail).
  const [latestRun, setLatestRun] = useState<RunResponse | null>(null);
  const [inspectorRunId, setInspectorRunId] = useState<string | null>(
    initialRunId ?? null,
  );
  const [discardTargetId, setDiscardTargetId] = useState<string | null>(null);

  // V1.147 PR #194 round 4 — a module switch must reset the invocation
  // authoring state exactly like a World switch. The Modules page stays
  // mounted when the author clicks another module in the list or a
  // `?module=` deep-link switches while the target module's detail is cached
  // (no LoadingState remount), so the `useState` seeds alone would keep the
  // previous module's guided values + Advanced JSON — a Run would submit one
  // module's params against another. Mirrors the World re-sync: keyed on the
  // prop VALUE (`module.module_id`) with a ref guard, so the initial mount is
  // a no-op and an unchanged prop never clears the form the author is
  // filling. The run inspector is cleared here too — a module change
  // invalidates the previous module's run context, and the `latestRun`
  // fresh-run path has no `?run=` URL backing for the run re-sync effect to
  // close (selectModule deletes `?run=` with the same semantics for the
  // deep-linked path). Declared before the `initialRunId` re-sync effect so
  // a same-commit `?module=&run=` deep-link change re-opens the new run
  // after this reset.
  const lastModuleIdRef = useRef(module.module_id);
  useEffect(() => {
    if (module.module_id !== lastModuleIdRef.current) {
      lastModuleIdRef.current = module.module_id;
      resetInvocation();
      setInspectorRunId(null);
      setLatestRun(null);
    }
  }, [module.module_id]);

  // V1.147 PR #194 — deep-link `?run=` re-sync. The Modules page stays
  // mounted when a second compute-node "Open Run" navigates to the same route
  // (`/modules?module=…&run=…` — search params only), so the
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
      {
        world_id: worldId,
        module_id: module.module_id,
        invocation_params: params,
        // Bugbot 1 — fork branch scope (from the Timeline `?branch=`
        // pre-fill); undefined = world root, daemon default.
        branch_id: branchId ?? undefined,
      },
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
    <>
      <RunStudioChrome
        copy={runStudioCopy(t)}
        world={{
          options: worlds.data ?? [],
          value: worldId,
          onChange: handleWorldChange,
          isLoading: worlds.isLoading,
          isError: worlds.isError,
          onRetry: () => worlds.refetch(),
        }}
        form={{
          schema: invocationSchema,
          requiredKeyBlockTypes: module.required_key_block_types,
          values,
          onChange: setField,
          entityEntries,
          copy: runFormCopy(t),
          // No World selected → the KB graph is not queried, so the form
          // renders without gating on it.
          graph:
            worldId !== ''
              ? {
                  isLoading: worldGraph.isLoading,
                  isError: worldGraph.isError,
                  onRetry: () => worldGraph.refetch(),
                }
              : null,
        }}
        advanced={{
          open: advancedOpen,
          onOpenChange: setAdvancedOpen,
          json: jsonText,
          onJsonChange: (value) => {
            setJsonText(value);
            setJsonDirty(true);
            setJsonError(null);
          },
          error: jsonError,
        }}
        run={{
          pending: runCompute.isPending,
          canRun,
          onRun: handleRun,
          detailLoading: Boolean(inspectorRunId) && runDetail.isLoading,
          detailError: Boolean(inspectorRunId) && runDetail.isError,
          onDetailRetry: () => void runDetail.refetch(),
        }}
        proposal={{
          run: inspectorRun
            ? {
                runIdShort: shortId(inspectorRun.run_id),
                status: inspectorRun.status,
                proposals: inspectorRun.proposals,
                error: inspectorRun.error,
              }
            : null,
          truncated: latestRun?.truncated ?? false,
          acceptPending: acceptRun.isPending,
          discardPending: discardRun.isPending,
          onAccept: handleAccept,
          onDiscard: () => {
            const runId = inspectorRunId ?? latestRun?.run_id;
            if (runId) setDiscardTargetId(runId);
          },
        }}
        runs={{
          rows: runRows,
          copy: runsTableCopy(t),
          isLoading: runs.isLoading,
          isError: runs.isError,
          onRetry: () => runs.refetch(),
          statusFilter: runStatusFilter,
          onStatusFilterChange: setRunStatusFilter,
          worldFilter: runWorldFilter,
          onWorldFilterChange: setRunWorldFilter,
          clearScopeWorldId,
          clearPending: clearRuns.isPending,
          onClear: () => {
            if (clearScopeWorldId) setClearTargetWorldId(clearScopeWorldId);
          },
          onOpenRun: openRun,
          hasNextPage: runs.hasNextPage,
          isFetchingNextPage: runs.isFetchingNextPage,
          onLoadMore: () => void runs.fetchNextPage(),
        }}
      />

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
    </>
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

/** Flat copy for the studio chrome — assembled from the `modules` namespace. */
function runStudioCopy(t: TFunction): RunStudioCopy {
  return {
    title: t('run.title'),
    worldLabel: t('run.worldLabel'),
    worldPlaceholder: t('run.worldPlaceholder'),
    branchNote: t('run.branchNote'),
    noWorldHint: t('run.noWorldHint'),
    worldsErrorTitle: t('run.worldsErrorTitle'),
    worldsErrorDescription: t('run.worldsErrorDescription'),
    graphErrorTitle: t('run.worldGraphErrorTitle'),
    graphErrorDescription: t('run.worldGraphErrorDescription'),
    graphLoading: t('run.kbGraphLoading'),
    advancedJson: t('run.advancedJson'),
    advancedJsonWarning: t('run.advancedJsonWarning'),
    runButton: t('run.runButton'),
    runPending: t('run.runPending'),
    runLoadingDetail: t('run.runLoadingDetail'),
    runErrorTitle: t('run.runErrorTitle'),
    runErrorDescription: t('run.runErrorDescription'),
    runsTitle: t('run.runsTitle'),
    statusFilter: t('run.runsTable.statusFilter'),
    allStatuses: t('run.runsTable.allStatuses'),
    worldFilter: t('run.runsTable.worldFilter'),
    allWorlds: t('run.runsTable.allWorlds'),
    clearHistory: t('run.clearHistory'),
    clearDisabledHint: t('run.clearDisabledHint'),
    runsLoading: t('run.runsLoading'),
    runsErrorTitle: t('run.runsErrorTitle'),
    runsErrorDescription: t('run.runsErrorDescription'),
    loadMore: t('run.runsTable.loadMore'),
    loadingMore: t('run.runsTable.loadingMore'),
    statusOptions: RUN_STATUS_FILTER_OPTIONS.map((option) => ({
      value: option.value,
      label: t(option.labelKey),
    })),
    statusLabel: (status) => runStatusLabel(t, status),
    accept: t('run.accept'),
    discard: t('run.discard'),
    acceptAria: t('run.acceptAria'),
    discardAria: t('run.discardAria'),
    acceptNote: t('run.acceptNote'),
    stoppedTitle: t('run.stoppedTitle'),
    failedTitle: t('run.failedTitle'),
    stoppedDescription: t('run.stoppedDescription'),
    failedDescription: (reason) => t('run.failedDescription', { reason }),
    failedCodeLabel: t('run.failedCodeLabel'),
    appliedNote: t('run.appliedNote'),
    discardedNote: t('run.discardedNote'),
    runningNote: t('run.runningNote'),
    formCopy: runFormCopy(t),
    proposalsCopy: proposalCopy(t),
    runsTableCopy: runsTableCopy(t),
  };
}

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
