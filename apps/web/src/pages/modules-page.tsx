/**
 * Compute Modules body (Control Room — READ) — V1.114 P2 T4 / V1.131 P2.
 *
 * List/detail/query/error live here once. Settings modal mounts the body as
 * the `modules` section; `/modules` is a compatibility redirect only.
 */
import { Cpu, RefreshCw } from 'lucide-react';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Navigate } from 'react-router-dom';

import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { EmptyState, ErrorState, LoadingState, UnavailableState } from '@/components/ui/states';
import { useComputeModule, useComputeModules } from '@/api/queries';
import { isOrchestrationEngineUnavailable } from '@/lib/nexus/errors';
import { cn } from '@/lib/utils';

/** Compatibility adapter — product entry is Settings modal `modules` section. */
export function ModulesPage() {
  return <Navigate to="/settings/modules" replace />;
}

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
                        'flex w-full flex-col gap-2 rounded-card border p-4 text-left transition-colors duration-state ease-standard focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-700 focus-visible:ring-offset-2 focus-visible:ring-offset-background-100',
                        selectedId === m.module_id
                          ? 'border-blue-700 bg-gray-alpha-100'
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
    </div>
  );
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
