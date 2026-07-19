/**
 * Global Timeline view — V1.123 P3 Task 1 (cross-World Timeline overview).
 *
 * Renders recent Timeline activity across all Worlds as a single list. Per
 * `iterations/v1.123/specs/three-layer-product-spec.md` (Timeline as the
 * central instrument), the global view is the primary-nav entry that
 * complements the per-World Timeline hero surface (`/worlds/:worldId/timeline`
 * from V1.122 P1 T3) and the Work Timeline peer surface
 * (`/works/:workId/timeline` from V1.123 P2 T5).
 *
 * Composition model (architect §5 + §8 LOCKED — client-side composition; no
 * new composite daemon endpoint):
 *   - Worlds list: existing `useNarrativeWorlds()` hook
 *     (`GET /v1/daemon/narrative/worlds`).
 *   - Per-World graph: existing `useWorldKbGraph(worldId)` hook
 *     (`GET /v1/daemon/worlds/{world_id}/kb/graph` — V1.73).
 *   - The two are composed CLIENT-SIDE; the daemon exposes no
 *     `GET .../timeline` composite endpoint in V1.123 (architect §5 LOCK).
 *
 * N+1 mitigation (plan Global Constraints): the per-World graph fan-out is
 * capped to N=5 most-recent Worlds by `updated_at`. Worlds past the cap are
 * omitted from the activity list (the plan allows N=5–10; the floor keeps
 * the page cheap on workspaces with many Worlds). The cap is a `simplify:`
 * ceiling — a future composite endpoint (`DF-V1122-DEEPER-WB` stays
 * deferred) would lift it without UI churn.
 *
 * V1.124 P2: presentational list chrome lives in
 * `presentational/global-timeline-list-chrome.tsx` (Studio:
 * `@web-global-timeline/global-timeline-list-chrome`). This module owns hooks,
 * contracts types, router `Link`, and i18n only.
 */
import { useMemo, type ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import { Link } from 'react-router-dom';
import { useQueries } from '@tanstack/react-query';

import { useNarrativeWorlds } from '@/api/queries';
import { useNexusClient } from '@/lib/client-context';
import { queryKeys } from '@/lib/nexus/query-keys';
import { formatRelative } from '@/lib/format';
import type { WorldKbGraphResponse } from '@42ch/nexus-contracts';

import {
  GlobalTimelineListChrome,
  type GlobalTimelineListRow,
} from '@/components/global-timeline/presentational/global-timeline-list-chrome';

/**
 * Cap on the per-World graph fan-out. The plan Global Constraints allow
 * N=5–10; the floor (5) keeps the page cheap on workspaces with many
 * Worlds. Raising the cap is a one-line change once a composite endpoint
 * lands (`DF-V1122-DEEPER-WB`).
 *
 * `simplify:` static cap — measured against the typical small-Workspace
 * workload (1–10 Worlds). A paginated composite endpoint would replace the
 * fan-out entirely; until then, this cap is the honest performance ceiling.
 */
const GLOBAL_TIMELINE_WORLD_CAP = 5;

/**
 * Derive the Timeline layer kind from a World's KB graph. Mirrors the
 * V1.123 P1 Batch A T3 default-layer logic (`timeline-canvas.tsx`):
 *   - `'brief'`     when the graph has any `block_type=era` entity.
 *   - `'narrative'` otherwise (V1.122 default).
 *
 * `null` indicates the graph is unavailable (loading / error); callers fall
 * back to `'narrative'` so the activity row stays legible.
 */
function deriveLayer(graph: WorldKbGraphResponse | undefined): {
  layer: 'brief' | 'narrative';
  eraCount: number;
  eventCount: number;
} {
  if (!graph) {
    return { layer: 'narrative', eraCount: 0, eventCount: 0 };
  }
  const entities = graph.entities ?? [];
  const eraCount = entities.filter((e) => e.block_type === 'era').length;
  const eventCount = entities.filter((e) => e.block_type === 'event').length;
  return {
    layer: eraCount > 0 ? 'brief' : 'narrative',
    eraCount,
    eventCount,
  };
}

function renderAppRow(
  row: GlobalTimelineListRow,
  className: string,
  content: ReactNode,
) {
  return (
    <Link
      to={`/worlds/${encodeURIComponent(row.id)}/timeline`}
      data-testid="global-timeline-row"
      data-world-id={row.id}
      data-layer={row.layer ?? 'narrative'}
      className={className}
    >
      {content}
    </Link>
  );
}

export function GlobalTimelineView() {
  const { t } = useTranslation('canvas');
  const worlds = useNarrativeWorlds();
  const client = useNexusClient();

  // Top-N most-recent Worlds by `updated_at` (descending). Worlds without
  // `updated_at` sink to the bottom in their original list order so the
  // sort is stable across refetches.
  const visibleWorlds = useMemo(() => {
    const all = worlds.data ?? [];
    const withIdx = all.map((w, idx) => ({ w, idx }));
    withIdx.sort((a, b) => {
      const at = a.w.updated_at ?? '';
      const bt = b.w.updated_at ?? '';
      if (at === bt) return a.idx - b.idx;
      return at < bt ? 1 : at > bt ? -1 : 0;
    });
    return withIdx.slice(0, GLOBAL_TIMELINE_WORLD_CAP).map(({ w }) => w);
  }, [worlds.data]);

  // Per-World graph fan-out (parallel). The query keyspace is shared with
  // the per-World Timeline surface so cache reuse is automatic (opening a
  // World's Timeline does not re-fetch the graph the global view already
  // pulled).
  const graphQueries = useQueries({
    queries: visibleWorlds.map((w) => ({
      queryKey: queryKeys.worldKb.graph(w.world_id),
      queryFn: () => client.getWorldKbGraph(w.world_id),
      staleTime: 5_000,
    })),
  });

  const chromeShared = {
    title: t('globalTimeline.title'),
    description: t('globalTimeline.description'),
    listAriaLabel: t('globalTimeline.listAriaLabel'),
    emptyTitle: t('globalTimeline.empty.title'),
    emptyDescription: t('globalTimeline.empty.description'),
    loadingLabel: t('globalTimeline.loading'),
    errorDescription: t('globalTimeline.loadError'),
  };

  if (worlds.isLoading) {
    return (
      <GlobalTimelineListChrome
        {...chromeShared}
        state="loading"
        rows={[]}
      />
    );
  }
  if (worlds.isError) {
    return (
      <GlobalTimelineListChrome
        {...chromeShared}
        state="error"
        rows={[]}
        onRetry={() => worlds.refetch()}
      />
    );
  }
  if (!worlds.data || worlds.data.length === 0) {
    return (
      <GlobalTimelineListChrome
        {...chromeShared}
        state="empty"
        rows={[]}
      />
    );
  }

  const rows: GlobalTimelineListRow[] = visibleWorlds.map((world, i) => {
    const graphQuery = graphQueries[i];
    const graph = graphQuery?.data as WorldKbGraphResponse | undefined;
    const { layer, eraCount, eventCount } = deriveLayer(graph);
    const graphLoading = graphQuery?.isLoading ?? false;
    const graphError = graphQuery?.isError ?? false;
    const label = world.title || world.world_id;
    const activityText = graphLoading
      ? t('globalTimeline.activityLoading')
      : graphError
        ? t('globalTimeline.activityError')
        : t('globalTimeline.activitySummary', {
            layer: t(`globalTimeline.layer.${layer}`),
            era: eraCount,
            event: eventCount,
          });

    return {
      id: world.world_id,
      label,
      activityText,
      layer,
      lastEditedText: world.updated_at
        ? t('globalTimeline.lastEdited', {
            when: formatRelative(world.updated_at),
          })
        : undefined,
    };
  });

  return (
    <GlobalTimelineListChrome
      {...chromeShared}
      state="ready"
      rows={rows}
      renderRow={renderAppRow}
    />
  );
}
