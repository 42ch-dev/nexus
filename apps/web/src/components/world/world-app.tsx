/**
 * World app surface (plan v1219-world-module T1) — route owner of
 * `/worlds/:worldId`. Header (name + `世界 · 时间线`) + layer tabs
 * (Brief / Narrative / Moment) riding the `?layer=` URL convention.
 *
 * Brief-layer read model = the era `KnowledgeEntry` projection over the World
 * KB graph (`useWorldKbGraph`; architect-confirmed data contract) — NOT the
 * timeline-era endpoints (those belong to the Narrative layer). Narrative /
 * Moment are labelled pre-rebuild placeholders until their module rebuilds;
 * the Brief editor + write path lands in T2 (this file only reads).
 *
 * States are explicit and single: loading (`正在加载世界…`) and error
 * (`世界加载失败：{reason}` + `重试`) — never silent blanks or toasts.
 */
import { useQuery } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { useParams, useSearchParams } from 'react-router';

import { LayerTabs, parseWorldLayer } from '@/components/world/layer-tabs';
import { WorldHeader } from '@/components/world/world-header';
import { ErrorState, LoadingState } from '@/components/ui/states';
import { useWorldKbGraph } from '@/lib/canvas/use-world-kb-data';
import { useNexusClient } from '@/lib/client-context';
import { NexusClientError } from '@/lib/nexus';
import { NotFoundPage } from '@/pages/not-found-page';
import type { WorldKbEntityProjection } from '@42ch/nexus-contracts';

export function WorldApp() {
  const { worldId } = useParams<{ worldId: string }>();
  if (!worldId) return <NotFoundPage />;
  return <WorldAppSurface worldId={worldId} />;
}

function errorReason(error: unknown): string {
  if (error instanceof NexusClientError) return error.message;
  if (error instanceof Error) return error.message;
  return String(error);
}

function eraSummary(entity: WorldKbEntityProjection): string | null {
  const attrs = entity.body?.attributes;
  const raw = attrs && typeof attrs === 'object' ? (attrs as Record<string, unknown>).world_summary : null;
  return typeof raw === 'string' && raw.trim() !== '' ? raw : null;
}

function WorldAppSurface({ worldId }: { worldId: string }) {
  const { t } = useTranslation('worlds');
  const client = useNexusClient();
  const [searchParams] = useSearchParams();
  const layer = parseWorldLayer(searchParams.get('layer'));

  // World read (name for the header; 404 not_found surfaces as the error state).
  const world = useQuery({
    queryKey: ['worlds', 'detail', worldId],
    queryFn: () => client.getWorld(worldId),
  });
  // Brief-layer read model: era KnowledgeEntry projection over the KB graph.
  const graph = useWorldKbGraph(worldId);

  if (world.isPending || graph.isPending) {
    return <LoadingState label={t('worldApp.loading')} />;
  }
  const error = world.error ?? graph.error;
  if (error) {
    return (
      <ErrorState
        title={t('worldApp.loadError', { reason: errorReason(error) })}
        retryLabel={t('worldApp.retry')}
        onRetry={() => {
          void world.refetch();
          void graph.refetch();
        }}
      />
    );
  }

  const eras = (graph.data?.entities ?? []).filter((e) => e.block_type === 'era');

  return (
    <div className="flex flex-col gap-4" data-testid="world-app">
      <WorldHeader name={world.data!.world.title} />
      <LayerTabs active={layer} />
      {layer === 'brief' ? (
        <section aria-label={t('worldApp.briefAria')} data-testid="world-layer-brief">
          {eras.length === 0 ? (
            <div className="rounded-card border border-gray-500 p-6" data-testid="world-brief-empty">
              <h2 className="font-display text-display-24 text-gray-1000">
                {t('worldApp.briefEmpty.title')}
              </h2>
              <p className="text-copy-14 text-gray-900">{t('worldApp.briefEmpty.body')}</p>
              <p className="text-copy-14 text-gray-700">{t('worldApp.briefEmpty.example')}</p>
            </div>
          ) : (
            <ul className="flex flex-col gap-2">
              {eras.map((era) => {
                const summary = eraSummary(era);
                return (
                  <li
                    key={era.key_block_id}
                    className="rounded-card border border-gray-500 p-4"
                  >
                    <h3 className="text-heading-16 font-heading text-gray-1000">
                      {era.canonical_name}
                    </h3>
                    {summary && <p className="text-copy-14 text-gray-900">{summary}</p>}
                  </li>
                );
              })}
            </ul>
          )}
        </section>
      ) : (
        <section
          aria-label={t(`worldApp.layer.${layer}`)}
          data-testid={`world-layer-${layer}`}
          className="rounded-card border border-gray-500 p-6"
        >
          <h2 className="text-heading-16 font-heading text-gray-1000">
            {t(layer === 'narrative' ? 'worldApp.narrativePlaceholder' : 'worldApp.momentPlaceholder')}
          </h2>
          <p className="text-copy-14 text-gray-700">{t('worldApp.layerPlaceholderSub')}</p>
        </section>
      )}
    </div>
  );
}
