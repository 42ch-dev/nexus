/**
 * Capability registry browser (Control Room — READ) — web-ui.md §6.1 #4 /
 * plan v221-caps-sessions-rail T1.
 *
 * Lists every `nexus.*` capability the runtime exposes with its I/O schemas.
 * Surfaces the V1.34 agent tool bridge so authors can see what presets can
 * invoke. Admission-gate details are enforced at invocation time and are not
 * included in the list response; the per-card note surfaces that limitation.
 *
 * v221-caps-sessions-rail T1 — the markup moved to
 * `components/developer/presentational/capabilities-page-chrome.tsx` (pure
 * view, no transport / router / i18n) so the Design Studio fixture can render
 * it with static copy. This file keeps only app wiring: the query, the router
 * read and the copy assembly.
 */
import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useSearchParams } from 'react-router';
import type { TFunction } from 'i18next';

import { useCapabilities } from '@/api/queries';
import {
  CapabilitiesPageChrome,
  type CapabilitiesPageCopy,
} from '@/components/developer/presentational/capabilities-page-chrome';

/** `/capabilities` page entry — the registry's `load` target. */
export function CapabilitiesPage() {
  const { t } = useTranslation('capabilities');
  const caps = useCapabilities();
  const [searchParams] = useSearchParams();
  // V1.171 P1 (PL-13) — the preset profile deep-links required capabilities
  // with `?filter=<name>`; seed the local filter so the linked schema is
  // visible on arrival. The param is a read-only seed: typing does not write
  // it back and a later param change does not re-seed. No param → full list.
  const [filter, setFilter] = useState(() => searchParams.get('filter') ?? '');

  return (
    <CapabilitiesPageChrome
      copy={pageCopy(t)}
      list={{
        items: caps.data,
        isLoading: caps.isLoading,
        isError: caps.isError,
        isRefreshing: caps.isFetching,
        onRefresh: () => caps.refetch(),
      }}
      filter={filter}
      onFilterChange={setFilter}
    />
  );
}

/* ── Caller-owned copy (i18n lives in the app, not in the chrome) ─────────── */

function pageCopy(t: TFunction): CapabilitiesPageCopy {
  return {
    title: t('title'),
    description: t('description'),
    filterLabel: t('filterLabel'),
    filterPlaceholder: t('filterPlaceholder'),
    refresh: t('refresh'),
    refreshAria: t('refreshAria'),
    errorDescription: t('errorDescription'),
    loading: t('loading'),
    emptyTitle: t('emptyTitle'),
    emptyDescription: t('emptyDescription'),
    filteredMissTitle: (filter) => t('filteredMissTitle', { filter }),
    filteredMissDescription: t('filteredMissDescription'),
    clearFilter: t('clearFilter'),
    inputSchema: t('inputSchema'),
    outputSchema: t('outputSchema'),
    userBadge: t('userBadge'),
    localOnlyCopy: t('localOnlyCopy'),
    admissionGatesInfo: t('admissionGatesInfo'),
  };
}
