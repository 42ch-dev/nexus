import { useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useNavigate } from 'react-router-dom';
import { Globe, Plus, RefreshCw } from 'lucide-react';

import { Button } from '@/components/ui/button';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { EmptyCreateCard } from '@/components/ui/empty-create-card';
import { CreateWorldDialog } from '@/components/worlds/create-world-dialog';
import { ErrorState, LoadingState } from '@/components/ui/states';
import { useNarrativeWorlds, useTimelineOverview, flattenOverviewWorlds } from '@/api/queries';
import { useNexusClient } from '@/lib/client-context';
import { formatRelative } from '@/lib/format';
import { hasCreateWorldClient } from '@/lib/nexus/create-world';

import { CreateWorkDialog } from './dialogs/create-work-dialog';

export function WorldsPage() {
  const { t } = useTranslation('worlds');
  const navigate = useNavigate();
  const client = useNexusClient();
  const canCreateWorld = useMemo(() => hasCreateWorldClient(client), [client]);
  const [createWorldOpen, setCreateWorldOpen] = useState(false);
  const [createWorkOpen, setCreateWorkOpen] = useState(false);
  const worlds = useNarrativeWorlds();
  const overview = useTimelineOverview();
  const overviewWorlds = flattenOverviewWorlds(overview.data);

  const overviewMap = useMemo(() => {
    return new Map(
      overviewWorlds.map((w) => [
        w.world_id,
        { era_count: w.era_count, event_count: w.event_count, last_event_at: w.last_event_at ?? null },
      ]),
    );
  }, [overviewWorlds]);

  function handleCreateWorldClick() {
    setCreateWorldOpen(true);
  }

  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div>
          <h1 className="font-display text-display-24 text-gray-1000">{t('title')}</h1>
          <p className="text-copy-14 text-gray-900">{t('description')}</p>
        </div>
        <Button
          type="button"
          variant="tertiary"
          size="small"
          onClick={() => worlds.refetch()}
          disabled={worlds.isFetching}
          aria-label={t('refreshAria')}
        >
          <RefreshCw className={`h-4 w-4 ${worlds.isFetching ? 'animate-spin' : ''}`} aria-hidden />
          {t('refresh')}
        </Button>
      </div>

      <Card className="shadow-card">
        <CardHeader>
          <CardTitle>{t('listTitle')}</CardTitle>
          <CardDescription>{t('description')}</CardDescription>
        </CardHeader>
        <CardContent>
          {worlds.isError ? (
            <ErrorState description={t('errorDescription')} onRetry={() => worlds.refetch()} />
          ) : worlds.isLoading ? (
            <LoadingState label={t('loading')} />
          ) : !worlds.data || worlds.data.length === 0 ? (
            <div className="flex flex-col gap-3">
              {canCreateWorld ? (
                <EmptyCreateCard
                  icon={Globe}
                  title={t('emptyCreateWorldTitle')}
                  description={t('emptyCreateWorldDescription')}
                  onClick={handleCreateWorldClick}
                  data-testid="worlds-empty-create-world"
                />
              ) : (
                <>
                  {/*
                    V1.127 P0 T1 (AC-V1127-1): createWorld is absent on every
                    current bridge, so render the Create World affordance as a
                    disabled card with a desktop-only tooltip instead of
                    silently swapping it out. Inline (rather than EmptyCreateCard)
                    because EmptyCreateCard has no disabled path; classes mirror
                    its layout and drop hover/focus for the disabled state.
                  */}
                  <button
                    type="button"
                    disabled
                    tabIndex={-1}
                    title={t('create.desktop-only')}
                    data-testid="worlds-empty-create-world"
                    className="flex w-full min-h-[7.5rem] flex-col items-center justify-center gap-2 rounded-card border border-dashed border-gray-alpha-400 p-6 text-center opacity-60 motion-reduce:transition-none"
                  >
                    <Globe className="h-8 w-8 shrink-0 text-brand-deep-blue dark:text-blue-700" aria-hidden />
                    <span className="font-display text-display-20 tracking-tight text-gray-1000">
                      {t('emptyCreateWorldTitle')}
                    </span>
                    <span className="max-w-sm text-copy-14 text-gray-700">
                      {t('emptyCreateWorldDescription')}
                    </span>
                  </button>
                  <EmptyCreateCard
                    icon={Plus}
                    title={t('emptyCreateWorkTitle')}
                    description={t('emptyCreateWorkDescription')}
                    onClick={() => setCreateWorkOpen(true)}
                    data-testid="worlds-empty-create-work"
                  />
                </>
              )}
            </div>
          ) : (
            <>
              {/* V1.127 P0 T3 (AC-V1127-3): scoped overview error banner. The
                world list (useNarrativeWorlds) has its own error handling
                above (line 76); this banner is for the overview/activity
                enrichment composite endpoint only, so a 5xx surfaces a Retry
                instead of silently degrading every row to "no recent
                activity". ErrorState defaults title + retry label from the
                common namespace (mirrors the world-list ErrorState usage). */}
              {overview.isError ? (
                <div className="pb-2" data-testid="worlds-overview-error">
                  <ErrorState
                    description={t('overview.failed')}
                    onRetry={() => overview.refetch()}
                  />
                </div>
              ) : null}
              <ul className="flex flex-col gap-2" aria-label={t('listAriaLabel')}>
                {worlds.data.map((world) => {
                  const label = world.title || world.world_id;
                  const wld = overviewMap.get(world.world_id);
                  const activityText = wld
                    ? t('timelineActivityOverview', {
                        era: wld.era_count,
                        event: wld.event_count,
                        when: wld.last_event_at
                          ? formatRelative(wld.last_event_at)
                          : t('timelineActivityFallback'),
                      })
                    : world.updated_at
                      ? t('timelineActivityLastEdited', {
                          when: formatRelative(world.updated_at),
                        })
                      : t('timelineActivityFallback');
                  return (
                    <li key={world.world_id}>
                      <button
                        type="button"
                        onClick={() => navigate(`/worlds/${encodeURIComponent(world.world_id)}/timeline`)}
                        aria-label={t('openTimeline')}
                        className="flex w-full items-center gap-3 rounded-card border border-gray-alpha-400 p-3 text-left transition-colors duration-state ease-standard hover:bg-gray-alpha-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-700 focus-visible:ring-offset-2"
                      >
                        <Globe className="h-5 w-5 shrink-0 text-blue-700" aria-hidden />
                        <span className="min-w-0 flex-1">
                          <span className="block truncate font-display text-display-20 tracking-tight text-gray-1000">
                            {label}
                          </span>
                          {world.title && (
                            <span className="block truncate text-copy-13-mono text-gray-700">
                              {world.world_id}
                            </span>
                          )}
                          <span
                            className="block truncate text-copy-13 text-gray-700"
                            data-testid={`world-timeline-activity-${world.world_id}`}
                          >
                            {activityText}
                          </span>
                        </span>
                      </button>
                    </li>
                  );
                })}
              </ul>
              {/* V1.127 P0 T2: the world list itself is complete
                (useNarrativeWorlds is unpaginated); the overview is auxiliary
                activity enrichment, capped at 20 worlds per page. Load More
                fetches the next overview page so worlds past the cap gain
                activity counts instead of the fallback. */}
              {overview.hasNextPage ? (
                <div className="pt-2">
                  <Button
                    type="button"
                    variant="tertiary"
                    className="w-full"
                    onClick={() => {
                      void overview.fetchNextPage();
                    }}
                    disabled={overview.isFetchingNextPage}
                    data-testid="worlds-overview-load-more"
                  >
                    {overview.isFetchingNextPage ? t('loadingMore') : t('loadMore')}
                  </Button>
                </div>
              ) : null}
            </>
          )}
        </CardContent>
      </Card>

      <CreateWorkDialog
        open={createWorkOpen}
        onOpenChange={setCreateWorkOpen}
        onCreated={(workId) => {
          navigate(`/works/${encodeURIComponent(workId)}/outline`);
        }}
      />
    <CreateWorldDialog open={createWorldOpen} onOpenChange={setCreateWorldOpen} />
    </div>
  );
}