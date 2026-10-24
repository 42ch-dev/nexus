/**
 * Compute Modules page (Control Room — READ + Run) — V1.114 P2 T4 / V1.147
 * P1 T3 / V1.147 P2 T3 / v221-modules-rail T2.
 *
 * `/modules` is the single owner of the Modules surface: list/detail/query/
 * error live here once, rendered as the page body.
 *
 * V1.147 P1: the detail panel gains the Run Studio — World selector, guided
 * form (manifest `schemas.invocation` → first-class controls), Advanced JSON
 * disclosure, Run → proposal inspector with Accept/Discard, and Runs history.
 *
 * V1.147 P2 T3: the Run Studio body lives in the shared
 * `components/compute/run-studio.tsx` (qc1 W-004 hook extraction — the
 * Timeline Run Module entry mounts the SAME studio with context pre-fill).
 * This page owns the deep-link selection mechanism: `?module=<id>&run=<id>`
 * (compute node "Open Run" → Modules run detail) and `?world=<id>` +
 * `?branch=<id>` (Timeline Run Module entry → World + fork branch pre-filled
 * — Bugbot 1). Module selection + run opening write the params back so
 * refresh keeps the detail.
 *
 * v221-modules-rail T2 — the markup moved to
 * `components/developer/presentational/modules-page-chrome.tsx` (pure view,
 * no transport / router / i18n) so the Design Studio fixture can render it
 * with static copy. This file keeps only app wiring: queries, router reads
 * and the copy assembly.
 */
import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useSearchParams } from 'react-router';
import type { TFunction } from 'i18next';

import { useComputeModule, useComputeModules } from '@/api/queries';
import { RunStudio } from '@/components/compute/run-studio';
import {
  ModuleDetailChrome,
  ModulesPageChrome,
  type ModuleDetailCopy,
  type ModulesPageCopy,
} from '@/components/developer/presentational/modules-page-chrome';
import { isOrchestrationEngineUnavailable } from '@/lib/nexus/errors';

/** `/modules` page entry — the registry's `load` target. */
export function ModulesPage() {
  return <ModulesPageBody />;
}

/** Page body (the tests mount it directly). */
export function ModulesPageBody() {
  const { t } = useTranslation('modules');
  const modules = useComputeModules();

  // V1.147 P2 T3 — deep-link selection mechanism (consistent with the
  // `?layer=` search-param convention on the Timeline surface). Seeds the
  // initial module selection from `?module=`; `?run=` + `?world=` thread
  // into the detail panel (Run inspector + World pre-fill). User selection
  // writes the params back (replace) so refresh keeps the detail.
  const [searchParams, setSearchParams] = useSearchParams();
  const [selectedId, setSelectedId] = useState<string | null>(() =>
    searchParams.get('module'),
  );

  // V1.147 PR #194 — deep-link `?module=` re-sync. The page stays mounted
  // when a second compute-node "Open Run" navigates to the same route
  // (`/modules?module=…` — search-params-only), so the `useState` seed alone
  // would keep the stale module selected. Keyed on the URL VALUE: external
  // navigation (Timeline "Open Run", back/forward) switches the detail panel,
  // and `?module=` removal clears the selection. User-driven writes go
  // through `selectModule` (state + URL set together), so when the effect
  // fires on those the value is already consistent — a no-op set on an
  // unchanged value.
  const moduleParam = searchParams.get('module');
  useEffect(() => {
    setSelectedId(moduleParam);
  }, [moduleParam]);

  // Bugbot 1 — `?branch=` pre-fill for the Run Studio, normalized like the
  // Timeline's branch derivation (`?branch=` empty → undefined/root — never
  // an empty-string branch id, which the daemon would reject as invalid).
  const branchParam = searchParams.get('branch');
  const branchSeed = branchParam && branchParam.length > 0 ? branchParam : undefined;

  function selectModule(id: string) {
    setSelectedId(id);
    // Preserve the `?world=` / `?branch=` entry pre-fill (Timeline Run
    // Module entry); a module change invalidates the deep-linked run context.
    const next = new URLSearchParams(searchParams);
    next.set('module', id);
    next.delete('run');
    setSearchParams(next, { replace: true });
  }

  return (
    <ModulesPageChrome
      copy={pageCopy(t)}
      list={{
        items: modules.data,
        isLoading: modules.isLoading,
        isError: modules.isError,
        isEngineUnavailable:
          modules.isError && isOrchestrationEngineUnavailable(modules.error),
        isRefreshing: modules.isFetching,
        onRefresh: () => modules.refetch(),
      }}
      selectedId={selectedId}
      onSelect={selectModule}
      detail={
        selectedId ? (
          <ModuleDetailPanel
            moduleId={selectedId}
            initialWorldId={searchParams.get('world') ?? undefined}
            initialBranchId={branchSeed}
            initialRunId={searchParams.get('run') ?? undefined}
            onRunOpen={(runId) => {
              // Keep `?world=` (entry pre-fill) alongside the run deep link
              // so refresh preserves the whole selection.
              const next = new URLSearchParams(searchParams);
              next.set('module', selectedId);
              next.set('run', runId);
              setSearchParams(next, { replace: true });
            }}
          />
        ) : null
      }
    />
  );
}

/** Manifest detail wiring — one query, then the chrome with the studio slot. */
function ModuleDetailPanel({
  moduleId,
  initialWorldId,
  initialBranchId,
  initialRunId,
  onRunOpen,
}: {
  moduleId: string;
  /** V1.147 P2 T3 — Timeline Run Module entry pre-fill (`?world=`). */
  initialWorldId?: string;
  /** Bugbot 1 — Timeline Run Module entry fork pre-fill (`?branch=`). */
  initialBranchId?: string;
  /** V1.147 P2 T3 — compute node "Open Run" deep link (`?run=`). */
  initialRunId?: string;
  /** V1.147 P2 T3 — Runs-table open → URL write-back (`?module&run`). */
  onRunOpen?: (runId: string) => void;
}) {
  const { t } = useTranslation('modules');
  const detail = useComputeModule(moduleId);

  return (
    <ModuleDetailChrome
      copy={detailCopy(t)}
      detail={{
        data: detail.data,
        isLoading: detail.isLoading,
        isError: detail.isError,
        isEngineUnavailable:
          detail.isError && isOrchestrationEngineUnavailable(detail.error),
        onRetry: () => detail.refetch(),
      }}
      runStudio={
        detail.data ? (
          <RunStudio
            module={detail.data}
            initialWorldId={initialWorldId}
            initialBranchId={initialBranchId}
            initialRunId={initialRunId}
            onRunOpen={onRunOpen}
          />
        ) : null
      }
    />
  );
}

/* ── Caller-owned copy (i18n lives in the app, not in the chrome) ─────────── */

function pageCopy(t: TFunction): ModulesPageCopy {
  return {
    title: t('title'),
    description: t('description'),
    refresh: t('refresh'),
    refreshAria: t('refreshAria'),
    loading: t('loading'),
    listTitle: t('listTitle'),
    listAriaLabel: t('listAriaLabel'),
    errorTitle: t('errorTitle'),
    errorDescription: t('errorDescription'),
    engineUnavailableTitle: t('engineUnavailableTitle'),
    engineUnavailableDescription: t('engineUnavailableDescription'),
    emptyTitle: t('emptyTitle'),
    emptyDescription: t('emptyDescription'),
    selectTitle: t('selectTitle'),
    selectDescription: t('selectDescription'),
  };
}

function detailCopy(t: TFunction): ModuleDetailCopy {
  return {
    title: t('detail.title'),
    version: (version) => t('detail.version', { version }),
    loading: t('detail.loading'),
    errorTitle: t('detail.errorTitle'),
    errorDescription: t('detail.errorDescription'),
    engineUnavailableTitle: t('detail.engineUnavailableTitle'),
    engineUnavailableDescription: t('detail.engineUnavailableDescription'),
    abiVersion: t('detail.abiVersion'),
    author: t('detail.author'),
    computeExport: t('detail.computeExport'),
    initExport: t('detail.initExport'),
    maxFuel: t('detail.maxFuel'),
    maxMemory: t('detail.maxMemory'),
    maxWallTime: t('detail.maxWallTime'),
    requiredKeyBlockTypes: t('detail.requiredKeyBlockTypes'),
    hostFunctions: t('detail.hostFunctions'),
    battleReportKind: t('detail.battleReportKind'),
    schemas: t('detail.schemas'),
    keyBlockAttributes: t('detail.keyBlockAttributes'),
    keyBlockState: t('detail.keyBlockState'),
    invocation: t('detail.invocation'),
    battleReport: t('detail.battleReport'),
  };
}
