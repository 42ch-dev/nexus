import { useTranslation } from 'react-i18next';
import { useNavigate } from 'react-router-dom';
import { Globe, RefreshCw } from 'lucide-react';

import { Button } from '@/components/ui/button';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { EmptyState, ErrorState, LoadingState } from '@/components/ui/states';
import { useNarrativeWorlds } from '@/api/queries';

/**
 * Worlds picker (V1.115 T3 — R-V1111P1-WORLDS-PICKER).
 *
 * Entry point for the World KB canvas when no `worldId` is in the URL. Reuses
 * the existing `useNarrativeWorlds` query (`GET /v1/daemon/narrative/worlds`)
 * — the same source the SOUL selector consumes — so there is exactly one
 * world-list fetch path in the app.
 *
 * Picking a world navigates to `/worlds/<id>/kb` (the existing World KB
 * route). The first world is **never** auto-selected — the author chooses.
 */
export function WorldsPage() {
  const { t } = useTranslation('worlds');
  const navigate = useNavigate();
  const worlds = useNarrativeWorlds();

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
            <EmptyState title={t('emptyTitle')} description={t('emptyDescription')} />
          ) : (
            <ul className="flex flex-col gap-2" aria-label={t('listAriaLabel')}>
              {worlds.data.map((world) => {
                const label = world.title || world.world_id;
                return (
                  <li key={world.world_id}>
                    <button
                      type="button"
                      onClick={() => navigate(`/worlds/${encodeURIComponent(world.world_id)}/kb`)}
                      aria-label={t('openKb')}
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
                      </span>
                    </button>
                  </li>
                );
              })}
            </ul>
          )}
        </CardContent>
      </Card>
    </div>
  );
}
