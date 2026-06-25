import { RefreshCw } from 'lucide-react';
import { useState } from 'react';

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
 * invoke. Note: CapabilityInfo carries name + input/output schemas only;
 * admission-gate data is not exposed in the list response (residual).
 */
export function CapabilitiesPage() {
  const caps = useCapabilities();
  const [filter, setFilter] = useState('');

  const filtered =
    caps.data?.filter((c) =>
      filter.trim() ? c.name.toLowerCase().includes(filter.trim().toLowerCase()) : true,
    ) ?? [];

  return (
    <Card className="shadow-card">
      <CardHeader>
        <div className="flex flex-wrap items-center justify-between gap-2">
          <div>
            <CardTitle>Capabilities</CardTitle>
            <CardDescription>The nexus.* capabilities the runtime can invoke.</CardDescription>
          </div>
          <div className="flex items-center gap-2">
            <label htmlFor="caps-filter" className="sr-only">Filter capabilities</label>
            <Input
              id="caps-filter"
              type="search"
              value={filter}
              onChange={(e) => setFilter(e.target.value)}
              placeholder="Filter by name"
              className="h-9 max-w-[220px]"
            />
            <Button
              type="button"
              variant="tertiary"
              size="small"
              onClick={() => caps.refetch()}
              disabled={caps.isFetching}
              aria-label="Refresh capabilities"
            >
              <RefreshCw className={`h-4 w-4 ${caps.isFetching ? 'animate-spin' : ''}`} aria-hidden />
              Refresh
            </Button>
          </div>
        </div>
      </CardHeader>
      <CardContent>
        {caps.isError ? (
          <ErrorState description="Could not load capabilities." onRetry={() => caps.refetch()} />
        ) : caps.isLoading ? (
          <LoadingState label="Loading capabilities…" />
        ) : filtered.length === 0 ? (
          <EmptyState title="No capabilities" description="Capabilities will appear here once the runtime registers them." />
        ) : (
          <ul className="flex flex-col gap-2">
            {filtered.map((c) => (
              <li key={c.name} className="rounded-card border border-gray-alpha-400 p-4">
                <div className="flex flex-wrap items-center gap-2">
                  <Badge variant="preset">{c.name}</Badge>
                </div>
                <div className="mt-3 grid grid-cols-1 gap-3 md:grid-cols-2">
                  <SchemaBlock title="Input schema" value={c.input_schema} />
                  <SchemaBlock title="Output schema" value={c.output_schema} />
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
