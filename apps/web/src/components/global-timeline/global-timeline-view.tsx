/**
 * Global Timeline view — V1.126 P2 (composite endpoint migration).
 *
 * Renders recent Timeline activity across all Worlds as a single list.
 * Uses one `useTimelineOverview` call instead of the old N=5–10 parallel
 * `kb/graph` fan-out. Per-World drill-down still calls `kb/graph` as before.
 */
import { type ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import { Link } from 'react-router-dom';

import { useTimelineOverview, flattenOverviewWorlds } from '@/api/queries';
import { formatRelative } from '@/lib/format';
import { Button } from '@/components/ui/button';

import {
  GlobalTimelineListChrome,
  type GlobalTimelineListRow,
} from '@/components/global-timeline/presentational/global-timeline-list-chrome';

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
  const overview = useTimelineOverview();
  const worlds = flattenOverviewWorlds(overview.data);

  const chromeShared = {
    title: t('globalTimeline.title'),
    description: t('globalTimeline.description'),
    listAriaLabel: t('globalTimeline.listAriaLabel'),
    emptyTitle: t('globalTimeline.empty.title'),
    emptyDescription: t('globalTimeline.empty.description'),
    loadingLabel: t('globalTimeline.loading'),
    errorDescription: t('globalTimeline.loadError'),
  };

  if (overview.isLoading) {
    return (
      <GlobalTimelineListChrome
        {...chromeShared}
        state="loading"
        rows={[]}
      />
    );
  }
  if (overview.isError) {
    return (
      <GlobalTimelineListChrome
        {...chromeShared}
        state="error"
        rows={[]}
        onRetry={() => overview.refetch()}
      />
    );
  }
  if (worlds.length === 0) {
    return (
      <GlobalTimelineListChrome
        {...chromeShared}
        state="empty"
        rows={[]}
      />
    );
  }

  const rows: GlobalTimelineListRow[] = worlds.map((world) => {
    const label = world.title?.trim() ? world.title : world.world_id;
    const layer = world.era_count > 0 ? 'brief' : 'narrative';
    const activityText = t('globalTimeline.activitySummary', {
      layer: t(`globalTimeline.layer.${layer}`),
      era: world.era_count,
      event: world.event_count,
    });

    return {
      id: world.world_id,
      label,
      activityText,
      layer,
      lastEditedText: world.last_event_at
        ? t('globalTimeline.lastEdited', {
            when: formatRelative(world.last_event_at),
          })
        : undefined,
    };
  });

  // V1.127 P0 T2: "Load more" pages past the daemon's 20-World page cap.
  // `hasNextPage` is true when the last overview page returned a non-null
  // next cursor (see useTimelineOverview.getNextPageParam).
  const footer = overview.hasNextPage ? (
    <div className="pt-2">
      <Button
        type="button"
        variant="tertiary"
        className="w-full"
        onClick={() => {
          void overview.fetchNextPage();
        }}
        disabled={overview.isFetchingNextPage}
        data-testid="global-timeline-load-more"
      >
        {overview.isFetchingNextPage
          ? t('globalTimeline.loadingMore')
          : t('globalTimeline.loadMore')}
      </Button>
    </div>
  ) : null;

  return (
    <GlobalTimelineListChrome
      {...chromeShared}
      state="ready"
      rows={rows}
      renderRow={renderAppRow}
      footer={footer}
    />
  );
}