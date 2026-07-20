import { useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useNavigate } from 'react-router-dom';
import { Globe, Plus, RefreshCw } from 'lucide-react';

import { Button } from '@/components/ui/button';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { EmptyCreateCard } from '@/components/ui/empty-create-card';
import { ErrorState, LoadingState } from '@/components/ui/states';
import { useNarrativeWorlds, useTimelineOverview } from '@/api/queries';
import { useNexusClient } from '@/lib/client-context';
import { formatRelative } from '@/lib/format';
import { hasCreateWorldClient } from '@/lib/nexus/create-world';

import { CreateWorkDialog } from './dialogs/create-work-dialog';

export function WorldsPage() {
  const { t } = useTranslation('worlds');
  const navigate = useNavigate();
  const client = useNexusClient();
  const canCreateWorld = useMemo(() => hasCreateWorldClient(client), [client]);
  const [createWorkOpen, setCreateWorkOpen] = useState(false);
  const worlds = useNarrativeWorlds();
  const overview = useTimelineOverview();

  const overviewMap = useMemo(() => {
    if (!overview.data) return new Map<string, { era_count: number; event_count: number; last_event_at: string | null }>();
    return new Map(
      overview.data.worlds.map((w) => [
        w.world_id,
        { era_count: w.era_count, event_count: w.event_count, last_event_at: w.last_event_at ?? null },
      ]),
    );
  }, [overview.data]);

  function handleCreateWorldClick() {
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
                <EmptyCreateCard
                  icon={Plus}
                  title={t('emptyCreateWorkTitle')}
                  description={t('emptyCreateWorkDescription')}
                  onClick={() => setCreateWorkOpen(true)}
                  data-testid="worlds-empty-create-work"
                />
              )}
            </div>
          ) : (
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
    </div>
  );
}