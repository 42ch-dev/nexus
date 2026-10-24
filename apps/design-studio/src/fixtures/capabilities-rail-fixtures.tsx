import { useState, type ReactNode } from 'react';

import {
  CapabilitiesPageChrome,
  type CapabilitiesListView,
  type CapabilitiesPageCopy,
  type CapabilityView,
} from '@web-developer/capabilities-page-chrome';

/**
 * Studio fixture for the `/capabilities` surface (plan v221-caps-sessions-rail
 * T1). Chrome rendered from static view models with literal copy — no daemon,
 * no router, no QueryClient. Four states: registry empty, filtered hit,
 * filtered miss (with Clear filter), and the read error.
 */

/* ------------------------------------------------------------------ */
/*  Caller-owned copy (mirrors locales/en/capabilities.json)            */
/* ------------------------------------------------------------------ */

const COPY: CapabilitiesPageCopy = {
  title: 'Capabilities',
  description: 'The nexus.* capabilities the runtime can invoke.',
  filterLabel: 'Filter capabilities',
  filterPlaceholder: 'Filter by name',
  refresh: 'Refresh',
  refreshAria: 'Refresh capabilities',
  errorDescription: 'Could not load capabilities.',
  loading: 'Loading capabilities…',
  emptyTitle: 'No capabilities',
  emptyDescription: 'Capabilities will appear here once the runtime registers them.',
  filteredMissTitle: (filter) => `No capability matches "${filter}"`,
  filteredMissDescription: 'Clear the filter to see all capabilities.',
  clearFilter: 'Clear filter',
  inputSchema: 'Input schema',
  outputSchema: 'Output schema',
  userBadge: 'Local',
  localOnlyCopy: 'Local module — no distribution or signing',
  admissionGatesInfo:
    'Admission gates are enforced by the daemon when this capability is invoked. Gate details are not exposed in the registry list response.',
};

/* ------------------------------------------------------------------ */
/*  Fake data                                                          */
/* ------------------------------------------------------------------ */

const CAPABILITIES: readonly CapabilityView[] = [
  {
    name: 'nexus.world.describe',
    inputSchema: '{"type":"object","properties":{"worldId":{"type":"string"}}}',
    outputSchema: '{"type":"string"}',
    origin: 'builtin',
  },
  {
    name: 'nexus.world.edit',
    inputSchema: '{"type":"object","properties":{"ops":{"type":"array"}}}',
    outputSchema: '{"type":"object"}',
    origin: 'builtin',
  },
  {
    name: 'sync.pull',
    inputSchema: '{"type":"object","properties":{"remote":{"type":"string"}}}',
    outputSchema: '{"type":"string"}',
    origin: 'user',
  },
];

function list(items: readonly CapabilityView[] | undefined, extra?: Partial<CapabilitiesListView>): CapabilitiesListView {
  return {
    items,
    isLoading: false,
    isError: false,
    isRefreshing: false,
    onRefresh: noop,
    ...extra,
  };
}

function noop() {
  /* Studio fixture only — actions have nowhere to go. */
}

/* ------------------------------------------------------------------ */
/*  Frames                                                             */
/* ------------------------------------------------------------------ */

function Frame({
  title,
  description,
  testId,
  children,
}: {
  title: string;
  description: string;
  testId: string;
  children: ReactNode;
}) {
  return (
    <div
      className="mb-8 rounded-card border border-gray-alpha-300 bg-background-100 p-4"
      data-testid={testId}
    >
      <h4 className="text-heading-16 font-heading text-gray-1000 mb-1">{title}</h4>
      <p className="text-copy-13 text-gray-700 mb-4">{description}</p>
      {children}
    </div>
  );
}

/** One interactive frame — the filter input is live so Clear filter works. */
function StatefulFrame({
  title,
  description,
  testId,
  list: listView,
  initialFilter,
}: {
  title: string;
  description: string;
  testId: string;
  list: CapabilitiesListView;
  initialFilter: string;
}) {
  const [filter, setFilter] = useState(initialFilter);

  return (
    <Frame title={title} description={description} testId={testId}>
      <CapabilitiesPageChrome
        copy={COPY}
        list={listView}
        filter={filter}
        onFilterChange={setFilter}
      />
    </Frame>
  );
}

export function CapabilitiesRailFixtures() {
  return (
    <div data-testid="capabilities-rail-fixtures">
      <StatefulFrame
        title="Registry empty"
        description="No capability is registered at all — the registry copy, not the filter miss, even though a filter is present."
        testId="capabilities-rail-fixture-empty"
        list={list([])}
        initialFilter="nexus"
      />

      <StatefulFrame
        title="Filtered hit"
        description="A deep-linked filter narrows the list to the matching schemas and leaves the rest out."
        testId="capabilities-rail-fixture-hit"
        list={list(CAPABILITIES)}
        initialFilter="world"
      />

      <StatefulFrame
        title="Filtered miss"
        description="The registry has capabilities but the filter matches none — its own title quotes the query and Clear filter restores the full list in place."
        testId="capabilities-rail-fixture-miss"
        list={list(CAPABILITIES)}
        initialFilter="no.such.cap"
      />

      <StatefulFrame
        title="Read error"
        description="The capabilities read failed — an alert with retry, never an empty registry."
        testId="capabilities-rail-fixture-error"
        list={list(undefined, { isError: true })}
        initialFilter=""
      />
    </div>
  );
}
