import { useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useNavigate } from 'react-router-dom';
import { Globe, Plus, RefreshCw } from 'lucide-react';

import { Button } from '@/components/ui/button';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { EmptyCreateCard } from '@/components/ui/empty-create-card';
import { ErrorState, LoadingState } from '@/components/ui/states';
import { useNarrativeWorlds } from '@/api/queries';
import { useNexusClient } from '@/lib/client-context';
import { formatRelative } from '@/lib/format';
import { hasCreateWorldClient } from '@/lib/nexus/create-world';

import { CreateWorkDialog } from './dialogs/create-work-dialog';

/**
 * Worlds picker (V1.115 T3 — R-V1111P1-WORLDS-PICKER; V1.122 P1 T3 retarget).
 *
 * Entry point for the World KB / Timeline canvas when no `worldId` is in the
 * URL. Reuses the existing `useNarrativeWorlds` query
 * (`GET /v1/daemon/narrative/worlds`) — the same source the SOUL selector
 * consumes — so there is exactly one world-list fetch path in the app.
 *
 * V1.122 P1 T3: picking a world now navigates to
 * `/worlds/<id>/timeline` (the World-entry hero surface — architect lock +
 * compass AC-V1122-5). World KB is reachable from the Timeline header as a
 * peer surface. The first world is **never** auto-selected — the author
 * chooses.
 */
export function WorldsPage() {
  const { t } = useTranslation('worlds');
  const navigate = useNavigate();
  const client = useNexusClient();
  const canCreateWorld = useMemo(() => hasCreateWorldClient(client), [client]);
  const [createWorkOpen, setCreateWorkOpen] = useState(false);
  const worlds = useNarrativeWorlds();

  function handleCreateWorldClick() {
    // CreateWorldDialog wires here when the wire contract ships.
  }

  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div>
          {/* V1.121 v0.4: page-level entity title is content voice (serif
              display-24) per DESIGN.md §Design Concept — Worlds is the
              creative-entity index. Sibling chrome (description, refresh
              button) stays interface voice (sans). */}
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
                // V1.123 P3 Task 3 — Timeline activity surface. Per plan
                // Global Constraints + architect §8, the durable slice is
                // the last-edited timestamp from `world.updated_at` (the
                // World wire type marks it optional, so the fallback handles
                // worlds without it). Per-World graph fetches for era/event
                // counts are an N+1 cost the list endpoint performance
                // cannot absorb; that composite is deferred to a future
                // endpoint (`DF-V1122-DEEPER-WB` stays deferred).
                const activityText = world.updated_at
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
                        {/* V1.121 v0.4: world titles in the list are
                            creative-entity titles → content voice (serif
                            display-20). The id line below stays sans mono
                            (interface voice). */}
                        <span className="block truncate font-display text-display-20 tracking-tight text-gray-1000">
                          {label}
                        </span>
                        {world.title && (
                          <span className="block truncate text-copy-13-mono text-gray-700">
                            {world.world_id}
                          </span>
                        )}
                        {/* V1.123 P3 T3 — Timeline activity surface. The
                            testid is namespaced by world_id so tests can
                            target a specific world row without ambiguity. */}
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
