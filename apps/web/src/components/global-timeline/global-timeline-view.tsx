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

import { useTimelineOverview } from '@/api/queries';
import { formatRelative } from '@/lib/format';

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
  if (!overview.data || overview.data.worlds.length === 0) {
    return (
      <GlobalTimelineListChrome
        {...chromeShared}
        state="empty"
        rows={[]}
      />
    );
  }

  const rows: GlobalTimelineListRow[] = overview.data.worlds.map((world) => {
    const label = world.title || world.world_id;
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

  return (
    <GlobalTimelineListChrome
      {...chromeShared}
      state="ready"
      rows={rows}
      renderRow={renderAppRow}
    />
  );
}