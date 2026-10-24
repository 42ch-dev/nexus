/**
 * Presentational body of the `/capabilities` surface (plan
 * v221-caps-sessions-rail T1).
 *
 * The product page (`components/developer/capabilities-page.tsx`) owns the
 * query, the router read and the i18n lookups; this module owns the markup so
 * the Design Studio fixture can render the same tree from static view models
 * with literal copy.
 *
 * Boundary: no transport, no router, no i18n, no QueryClient. Copy arrives in
 * `copy` props, data as view models, behavior as callbacks.
 */
import { useId } from 'react';

import { Info, RefreshCw } from 'lucide-react';

import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Input } from '@/components/ui/input';
import { EmptyState, ErrorState, LoadingState } from '@/components/ui/states';

/** A capability row, reduced to what the chrome renders. */
export interface CapabilityView {
  name: string;
  inputSchema: string;
  outputSchema: string;
  origin?: 'builtin' | 'user';
}

export interface CapabilitiesPageCopy {
  title: string;
  description: string;
  filterLabel: string;
  filterPlaceholder: string;
  refresh: string;
  refreshAria: string;
  errorDescription: string;
  loading: string;
  /** Registry-empty branch (no capability items at all). */
  emptyTitle: string;
  emptyDescription: string;
  /** Filtered-miss branch (registry non-empty, filter matched nothing). */
  filteredMissTitle: (filter: string) => string;
  filteredMissDescription: string;
  clearFilter: string;
  inputSchema: string;
  outputSchema: string;
  userBadge: string;
  localOnlyCopy: string;
  admissionGatesInfo: string;
}

/** The list query, reduced to what the chrome renders. */
export interface CapabilitiesListView {
  items?: readonly CapabilityView[];
  isLoading: boolean;
  isError: boolean;
  isRefreshing: boolean;
  onRefresh: () => void;
}

export interface CapabilitiesPageChromeProps {
  copy: CapabilitiesPageCopy;
  list: CapabilitiesListView;
  /** Controlled filter text (the caller owns where the seed comes from). */
  filter: string;
  onFilterChange: (filter: string) => void;
}

/**
 * Capability registry browser — header, filter, card list and the four
 * content states. Matching is case-insensitive substring on `name`
 * (unchanged); registry-empty takes precedence over filtered-miss.
 */
export function CapabilitiesPageChrome({
  copy,
  list,
  filter,
  onFilterChange,
}: CapabilitiesPageChromeProps) {
  const items = list.items ?? [];
  const query = filter.trim().toLowerCase();
  const matched = query
    ? items.filter((c) => c.name.toLowerCase().includes(query))
    : items;

  const filterId = useId();

  return (
    <Card className="shadow-card">
      <CardHeader>
        <div className="flex flex-wrap items-center justify-between gap-2">
          <div>
            <CardTitle>{copy.title}</CardTitle>
            <CardDescription>{copy.description}</CardDescription>
          </div>
          <div className="flex items-center gap-2">
            <label htmlFor={filterId} className="sr-only">{copy.filterLabel}</label>
            <Input
              id={filterId}
              type="search"
              value={filter}
              onChange={(e) => onFilterChange(e.target.value)}
              placeholder={copy.filterPlaceholder}
              className="h-9 max-w-[220px]"
            />
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
        </div>
      </CardHeader>
      <CardContent>
        {list.isError ? (
          <ErrorState description={copy.errorDescription} onRetry={list.onRefresh} />
        ) : list.isLoading ? (
          <LoadingState label={copy.loading} />
        ) : items.length === 0 ? (
          <EmptyState title={copy.emptyTitle} description={copy.emptyDescription} />
        ) : matched.length === 0 ? (
          <EmptyState
            title={copy.filteredMissTitle(filter.trim())}
            description={copy.filteredMissDescription}
            action={
              <Button
                type="button"
                variant="tertiary"
                size="small"
                onClick={() => onFilterChange('')}
              >
                {copy.clearFilter}
              </Button>
            }
          />
        ) : (
          <ul className="flex flex-col gap-2">
            {matched.map((c) => (
              <li key={c.name} className="rounded-card border border-gray-alpha-400 p-4">
                <div className="flex flex-wrap items-center gap-2">
                  <Badge variant="preset">{c.name}</Badge>
                  {c.origin === 'user' && <Badge variant="neutral">{copy.userBadge}</Badge>}
                </div>
                <div className="mt-3 grid grid-cols-1 gap-3 md:grid-cols-2">
                  <SchemaBlock title={copy.inputSchema} value={c.inputSchema} />
                  <SchemaBlock title={copy.outputSchema} value={c.outputSchema} />
                </div>
                {c.origin === 'user' && (
                  <div className="mt-3 flex items-start gap-2 rounded-control bg-background-300 p-3 text-copy-13 text-gray-800">
                    <Info className="mt-0.5 h-4 w-4 shrink-0 text-gray-700" aria-hidden />
                    <p>{copy.localOnlyCopy}</p>
                  </div>
                )}
                <div className="mt-3 flex items-start gap-2 rounded-control bg-background-300 p-3 text-copy-13 text-gray-800">
                  <Info className="mt-0.5 h-4 w-4 shrink-0 text-gray-700" aria-hidden />
                  <p>{copy.admissionGatesInfo}</p>
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
