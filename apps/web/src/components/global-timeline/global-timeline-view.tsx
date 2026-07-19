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
 * Each row links to the per-World Timeline route (the hero surface). The
 * `data-layer` attribute records the derived layer (`brief` when the World
 * has any `block_type=era` entity, else `narrative`) so the activity list
 * doubles as a Brief/Narrative affordance hint without requiring the user to
 * enter each World to find out. Per-layer counts (Brief: era count;
 * Narrative: event count) surface when the graph fetch resolves; on
 * per-World fetch failure the row degrades gracefully to "Timeline" fallback
 * copy and the `data-layer` falls back to `narrative`.
 */
import { useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import { Link } from 'react-router-dom';
import { useQueries } from '@tanstack/react-query';
import { CalendarRange } from 'lucide-react';

import { useNarrativeWorlds } from '@/api/queries';
import { useNexusClient } from '@/lib/client-context';
import { queryKeys } from '@/lib/nexus/query-keys';
import { formatRelative } from '@/lib/format';
import {
  EmptyState,
  ErrorState,
  LoadingState,
} from '@/components/ui/states';
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from '@/components/ui/card';
import type { WorldKbGraphResponse } from '@42ch/nexus-contracts';

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

  if (worlds.isLoading) {
    return (
      <div data-testid="global-timeline-loading">
        <LoadingState label={t('globalTimeline.loading')} />
      </div>
    );
  }
  if (worlds.isError) {
    return (
      <div data-testid="global-timeline-error">
        <ErrorState
          description={t('globalTimeline.loadError')}
          onRetry={() => worlds.refetch()}
        />
      </div>
    );
  }
  if (!worlds.data || worlds.data.length === 0) {
    return (
      <Card
        className="shadow-card"
        data-testid="global-timeline-view"
      >
        <CardHeader>
          <CardTitle voice="content">{t('globalTimeline.title')}</CardTitle>
          <CardDescription>{t('globalTimeline.description')}</CardDescription>
        </CardHeader>
        <CardContent>
          <EmptyState
            title={t('globalTimeline.empty.title')}
            description={t('globalTimeline.empty.description')}
          />
        </CardContent>
      </Card>
    );
  }

  return (
    <Card
      className="shadow-card"
      data-testid="global-timeline-view"
    >
      <CardHeader>
        {/* V1.121 v0.4 voice-split: page-level entity title is content voice
            (serif display-24). The global Timeline is the author's central
            instrument — the serif display treatment mirrors the Worlds list
            page. */}
        <CardTitle voice="content">{t('globalTimeline.title')}</CardTitle>
        <CardDescription>{t('globalTimeline.description')}</CardDescription>
      </CardHeader>
      <CardContent>
        <ul
          className="flex flex-col gap-2"
          aria-label={t('globalTimeline.listAriaLabel')}
        >
          {visibleWorlds.map((world, i) => {
            const graphQuery = graphQueries[i];
            const graph = graphQuery?.data as
              | WorldKbGraphResponse
              | undefined;
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

            return (
              <li key={world.world_id}>
                <Link
                  to={`/worlds/${encodeURIComponent(world.world_id)}/timeline`}
                  data-testid="global-timeline-row"
                  data-world-id={world.world_id}
                  data-layer={layer}
                  className="flex w-full items-center gap-3 rounded-card border border-gray-alpha-400 p-3 text-left transition-colors duration-state ease-standard hover:bg-gray-alpha-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-700 focus-visible:ring-offset-2"
                >
                  <CalendarRange
                    className="h-5 w-5 shrink-0 text-blue-700"
                    aria-hidden
                  />
                  <span className="min-w-0 flex-1">
                    {/* V1.121 v0.4: world titles stay content voice (serif
                        display-20) — mirrors the Worlds list page. */}
                    <span className="block truncate font-display text-display-20 tracking-tight text-gray-1000">
                      {label}
                    </span>
                    <span
                      className="block truncate text-copy-13 text-gray-700"
                      data-testid="global-timeline-row-activity"
                    >
                      {activityText}
                    </span>
                    {world.updated_at ? (
                      <span className="block truncate text-copy-13-mono text-gray-700">
                        {t('globalTimeline.lastEdited', {
                          when: formatRelative(world.updated_at),
                        })}
                      </span>
                    ) : null}
                  </span>
                </Link>
              </li>
            );
          })}
        </ul>
      </CardContent>
    </Card>
  );
}
