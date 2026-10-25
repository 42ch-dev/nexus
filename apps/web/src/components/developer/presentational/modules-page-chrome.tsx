/**
 * Presentational body of the `/modules` surface (plan v221-modules-rail T2).
 *
 * The product page (`components/developer/modules-page.tsx`) owns every hook,
 * query, router read and i18n lookup; this module owns the markup so the
 * Design Studio fixture can render the same tree from static view models with
 * literal copy.
 *
 * Boundary: no transport, no router, no i18n, no QueryClient. Copy arrives in
 * `copy` props, data as view models, behavior as callbacks. The Run Studio is
 * a slot (`runStudio`) so the wiring decides whether it mounts at all.
 */
import { Cpu, RefreshCw } from 'lucide-react';
import type { ReactNode } from 'react';

import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { EmptyState, ErrorState, LoadingState, UnavailableState } from '@/components/ui/states';
import { cn } from '@/lib/utils';

/** List row — structural subset of the daemon's module summary. */
export interface ModuleSummaryView {
  module_id: string;
  name: string;
  version: string;
  description?: string;
  required_key_block_types: string[];
  battle_report_kind?: string;
}

/** Manifest panel — structural subset of the daemon's module detail. */
export interface ModuleManifestView {
  module_id: string;
  name: string;
  version: string;
  nexus_abi_version: number;
  compute_export: string;
  init_export: string;
  description?: string;
  author?: string;
  host_functions?: string[];
  required_key_block_types: string[];
  battle_report_kind?: string;
  max_fuel?: number;
  max_memory_mib?: number;
  max_wall_time_ms?: number;
  schemas?: {
    key_block_attributes?: Record<string, unknown>;
    key_block_state?: Record<string, unknown>;
    invocation?: Record<string, unknown>;
    battle_report?: Record<string, unknown>;
  };
}

export interface ModulesPageCopy {
  title: string;
  description: string;
  refresh: string;
  refreshAria: string;
  loading: string;
  listTitle: string;
  listAriaLabel: string;
  errorTitle: string;
  errorDescription: string;
  engineUnavailableTitle: string;
  engineUnavailableDescription: string;
  emptyTitle: string;
  emptyDescription: string;
  selectTitle: string;
  selectDescription: string;
}

export interface ModuleDetailCopy {
  title: string;
  /** `Manifest v1.0.0` — caller-owned so plural/format stays in i18n. */
  version: (version: string) => string;
  loading: string;
  errorTitle: string;
  errorDescription: string;
  engineUnavailableTitle: string;
  engineUnavailableDescription: string;
  abiVersion: string;
  author: string;
  computeExport: string;
  initExport: string;
  maxFuel: string;
  maxMemory: string;
  maxWallTime: string;
  requiredKeyBlockTypes: string;
  hostFunctions: string;
  battleReportKind: string;
  schemas: string;
  keyBlockAttributes: string;
  keyBlockState: string;
  invocation: string;
  battleReport: string;
}

/** The list query, reduced to what the chrome renders. */
export interface ModulesListView {
  items?: readonly ModuleSummaryView[];
  isLoading: boolean;
  isError: boolean;
  /** Idle/unstarted orchestration engine renders the neutral state instead. */
  isEngineUnavailable?: boolean;
  isRefreshing: boolean;
  onRefresh: () => void;
}

export interface ModulesPageChromeProps {
  copy: ModulesPageCopy;
  list: ModulesListView;
  selectedId: string | null;
  onSelect: (moduleId: string) => void;
  /**
   * Detail panel slot. Rendered only when a module is selected; `null` keeps
   * the "select a module" prompt. The wiring decides which studio fills the
   * panel's own `runStudio` slot.
   */
  detail: ReactNode;
}

/** `/modules` page shell: header, list, selection prompt, detail slot. */
export function ModulesPageChrome({
  copy,
  list,
  selectedId,
  onSelect,
  detail,
}: ModulesPageChromeProps) {
  return (
    <Card className="shadow-card" data-testid="modules-page-body">
      <CardHeader>
        <div className="flex items-center justify-between gap-2">
          <div>
            <CardTitle>{copy.title}</CardTitle>
            <CardDescription>{copy.description}</CardDescription>
          </div>
          <Button
            type="button"
            variant="tertiary"
            size="small"
            onClick={list.onRefresh}
            disabled={list.isRefreshing}
            aria-label={copy.refreshAria}
          >
            <RefreshCw
              className={`h-4 w-4 ${list.isRefreshing ? 'animate-spin' : ''}`}
              aria-hidden
            />
            {copy.refresh}
          </Button>
        </div>
      </CardHeader>
      <CardContent>
        {list.isError ? (
          list.isEngineUnavailable ? (
            <UnavailableState
              title={copy.engineUnavailableTitle}
              description={copy.engineUnavailableDescription}
              onRetry={list.onRefresh}
            />
          ) : (
            <ErrorState
              title={copy.errorTitle}
              description={copy.errorDescription}
              onRetry={list.onRefresh}
            />
          )
        ) : list.isLoading ? (
          <LoadingState label={copy.loading} />
        ) : !list.items || list.items.length === 0 ? (
          <EmptyState title={copy.emptyTitle} description={copy.emptyDescription} />
        ) : (
          <div className="grid grid-cols-1 gap-4 lg:grid-cols-3">
            <div className="flex flex-col gap-2 lg:col-span-1">
              <p className="text-label-12 uppercase tracking-wide text-gray-700">
                {copy.listTitle}
              </p>
              <ul className="flex flex-col gap-2" aria-label={copy.listAriaLabel}>
                {list.items.map((m) => (
                  <li key={m.module_id}>
                    <button
                      type="button"
                      aria-label={m.name}
                      aria-pressed={selectedId === m.module_id}
                      onClick={() => onSelect(m.module_id)}
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
                detail
              ) : (
                <div className="flex h-full min-h-[240px] flex-col items-center justify-center gap-2 rounded-card border border-dashed border-gray-alpha-400 p-6 text-center">
                  <Cpu className="h-8 w-8 text-gray-500" aria-hidden />
                  <p className="text-heading-16 font-heading text-gray-1000">{copy.selectTitle}</p>
                  <p className="max-w-sm text-copy-14 text-gray-900">{copy.selectDescription}</p>
                </div>
              )}
            </div>
          </div>
        )}
      </CardContent>
    </Card>
  );
}

/** The detail query, reduced to what the manifest panel renders. */
export interface ModuleDetailView {
  data?: ModuleManifestView | null;
  isLoading: boolean;
  isError: boolean;
  /** Idle/unstarted orchestration engine renders the neutral state instead. */
  isEngineUnavailable?: boolean;
  onRetry: () => void;
}

export interface ModuleDetailChromeProps {
  copy: ModuleDetailCopy;
  detail: ModuleDetailView;
  /** The Run Studio mount point; the wiring owns its queries and state. */
  runStudio?: ReactNode;
}

/** Manifest detail panel — manifest fields, schemas, and the studio slot. */
export function ModuleDetailChrome({ copy, detail, runStudio }: ModuleDetailChromeProps) {
  if (detail.isLoading) {
    return <LoadingState label={copy.loading} />;
  }
  if (detail.isError) {
    if (detail.isEngineUnavailable) {
      return (
        <UnavailableState
          title={copy.engineUnavailableTitle}
          description={copy.engineUnavailableDescription}
          onRetry={detail.onRetry}
        />
      );
    }
    return (
      <ErrorState
        title={copy.errorTitle}
        description={copy.errorDescription}
        onRetry={detail.onRetry}
      />
    );
  }
  if (!detail.data) {
    return null;
  }

  const m = detail.data;

  return (
    <div className="flex flex-col gap-4 rounded-card border border-gray-alpha-400 bg-background-100 p-6">
      <p className="text-label-12 uppercase tracking-wide text-gray-700">{copy.title}</p>
      <div className="flex flex-col gap-1 sm:flex-row sm:items-start sm:justify-between sm:gap-4">
        <div>
          <h2 className="text-heading-20 font-heading text-gray-1000">{m.name}</h2>
          <p className="text-copy-13-mono text-gray-700">{m.module_id}</p>
        </div>
        <Badge variant="preset">{copy.version(m.version)}</Badge>
      </div>

      {m.description && <p className="text-copy-14 text-gray-900">{m.description}</p>}

      <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
        <DetailField label={copy.abiVersion} value={m.nexus_abi_version} testId="module-abi-version" />
        <DetailField label={copy.author} value={m.author} />
        <DetailField label={copy.computeExport} value={m.compute_export} />
        <DetailField label={copy.initExport} value={m.init_export} />
        {m.max_fuel !== undefined && <DetailField label={copy.maxFuel} value={m.max_fuel} />}
        {m.max_memory_mib !== undefined && (
          <DetailField label={copy.maxMemory} value={m.max_memory_mib} />
        )}
        {m.max_wall_time_ms !== undefined && (
          <DetailField label={copy.maxWallTime} value={m.max_wall_time_ms} />
        )}
      </div>

      {m.required_key_block_types.length > 0 && (
        <div className="flex flex-col gap-1">
          <p className="text-label-12 uppercase tracking-wide text-gray-700">
            {copy.requiredKeyBlockTypes}
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
            {copy.hostFunctions}
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
        <DetailField label={copy.battleReportKind} value={m.battle_report_kind} />
      )}

      {m.schemas && (
        <div className="flex flex-col gap-2">
          <p className="text-label-12 uppercase tracking-wide text-gray-700">{copy.schemas}</p>
          <SchemaBlock title={copy.keyBlockAttributes} value={m.schemas.key_block_attributes} />
          <SchemaBlock title={copy.keyBlockState} value={m.schemas.key_block_state} />
          <SchemaBlock title={copy.invocation} value={m.schemas.invocation} />
          <SchemaBlock title={copy.battleReport} value={m.schemas.battle_report} />
        </div>
      )}

      {runStudio}
    </div>
  );
}

export function DetailField({
  label,
  value,
  testId,
}: {
  label: string;
  value: string | number | undefined;
  /** Stable hook for row-scoped assertions (survives markup changes). */
  testId?: string;
}) {
  if (value === undefined || value === null || value === '') {
    return null;
  }

  return (
    <div className="flex flex-col gap-1" data-testid={testId}>
      <p className="text-label-12 uppercase tracking-wide text-gray-700">{label}</p>
      <p className="text-copy-14 text-gray-1000">{value}</p>
    </div>
  );
}

export function SchemaBlock({
  title,
  value,
}: {
  title: string;
  value: Record<string, unknown> | undefined;
}) {
  return (
    <div className="flex flex-col gap-1">
      <p className="text-label-12 uppercase tracking-wide text-gray-700">{title}</p>
      <pre className="overflow-x-auto rounded-control bg-background-300 p-3 text-copy-13-mono text-gray-900">
        {value ? JSON.stringify(value, null, 2) : '—'}
      </pre>
    </div>
  );
}
