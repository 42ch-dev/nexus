import { Info, RefreshCw } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { useState } from 'react';
import { useSearchParams } from 'react-router';

import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Input } from '@/components/ui/input';
import { EmptyState, ErrorState, LoadingState } from '@/components/ui/states';
import { useCapabilities } from '@/api/queries';

/**
 * Capability registry browser (Control Room — READ) — web-ui.md §6.1 #4.
 *
 * Lists every `nexus.*` capability the runtime exposes with its I/O schemas.
 * Surfaces the V1.34 agent tool bridge so authors can see what presets can
 * invoke. Admission-gate details are enforced at invocation time and are not
 * included in the list response; the UI surfaces that limitation explicitly.
 */
export function CapabilitiesPage() {
  const { t } = useTranslation('capabilities');
  const caps = useCapabilities();
  const [searchParams] = useSearchParams();
  // V1.171 P1 (PL-13) — the preset profile deep-links required capabilities
  // with `?filter=<name>`; seed the local filter so the linked schema is
  // visible on arrival. No param → empty filter (existing behavior).
  const [filter, setFilter] = useState(() => searchParams.get('filter') ?? '');

  const filtered =
    caps.data?.filter((c) =>
      filter.trim() ? c.name.toLowerCase().includes(filter.trim().toLowerCase()) : true,
    ) ?? [];

  return (
    <Card className="shadow-card">
      <CardHeader>
        <div className="flex flex-wrap items-center justify-between gap-2">
          <div>
            <CardTitle>{t('title')}</CardTitle>
            <CardDescription>{t('description')}</CardDescription>
          </div>
          <div className="flex items-center gap-2">
            <label htmlFor="caps-filter" className="sr-only">{t('filterLabel')}</label>
            <Input
              id="caps-filter"
              type="search"
              value={filter}
              onChange={(e) => setFilter(e.target.value)}
              placeholder={t('filterPlaceholder')}
              className="h-9 max-w-[220px]"
            />
            <Button
              type="button"
              variant="tertiary"
              size="small"
              onClick={() => caps.refetch()}
              disabled={caps.isFetching}
              aria-label={t('refreshAria')}
            >
              <RefreshCw className={`h-4 w-4 ${caps.isFetching ? 'animate-spin' : ''}`} aria-hidden />
              {t('refresh')}
            </Button>
          </div>
        </div>
      </CardHeader>
      <CardContent>
        {caps.isError ? (
          <ErrorState description={t('errorDescription')} onRetry={() => caps.refetch()} />
        ) : caps.isLoading ? (
          <LoadingState label={t('loading')} />
        ) : filtered.length === 0 ? (
          <EmptyState title={t('emptyTitle')} description={t('emptyDescription')} />
        ) : (
          <ul className="flex flex-col gap-2">
            {filtered.map((c) => (
              <li key={c.name} className="rounded-card border border-gray-alpha-400 p-4">
                <div className="flex flex-wrap items-center gap-2">
                  <Badge variant="preset">{c.name}</Badge>
                  {c.origin === 'user' && <Badge variant="neutral">{t('userBadge')}</Badge>}
                </div>
                <div className="mt-3 grid grid-cols-1 gap-3 md:grid-cols-2">
                  <SchemaBlock title={t('inputSchema')} value={c.input_schema} />
                  <SchemaBlock title={t('outputSchema')} value={c.output_schema} />
                </div>
                {c.origin === 'user' && (
                  <div className="mt-3 flex items-start gap-2 rounded-control bg-background-300 p-3 text-copy-13 text-gray-800">
                    <Info className="mt-0.5 h-4 w-4 shrink-0 text-gray-700" aria-hidden />
                    <p>{t('localOnlyCopy')}</p>
                  </div>
                )}
                <div className="mt-3 flex items-start gap-2 rounded-control bg-background-300 p-3 text-copy-13 text-gray-800">
                  <Info className="mt-0.5 h-4 w-4 shrink-0 text-gray-700" aria-hidden />
                  <p>{t('admissionGatesInfo')}</p>
                </div>
              </li>
            ))}
          </ul>
        )}
      </CardContent>
    </Card>
  );
}

function SchemaBlock({ title, value }: { title: string; value: string }) {
  return (
    <div className="flex flex-col gap-1">
      <p className="text-label-12 uppercase tracking-wide text-gray-700">{title}</p>
      <pre className="overflow-x-auto rounded-control bg-background-300 p-3 text-copy-13-mono text-gray-900">
        {value || '—'}
      </pre>
    </div>
  );
}
