/**
 * World app surface (plan v1219-world-module T1) — route owner of
 * `/worlds/:worldId`. Header (name + `世界 · 时间线`) + layer tabs
 * (Brief / Narrative / Moment) riding the `?layer=` URL convention.
 *
 * Brief-layer read model = the era `KnowledgeEntry` projection over the World
 * KB graph (`useWorldKbGraph`; architect-confirmed data contract) — NOT the
 * timeline-era endpoints (those belong to the Narrative layer). The Brief
 * layer's write path (empty state → inline editor → saved card) lives in T2's
 * components; Narrative / Moment are labelled pre-rebuild placeholders until
 * their module rebuilds.
 *
 * States are explicit and single: loading (`正在加载世界…`) and error
 * (`世界加载失败：{reason}` + `重试`) — never silent blanks or toasts.
 */
import { useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { useParams, useSearchParams } from 'react-router';

import { BriefCard, BriefReadyCard } from '@/components/world/brief-card';
import { BriefEditor } from '@/components/world/brief-editor';
import { BriefEmpty } from '@/components/world/brief-empty';
import { LayerTabs, parseWorldLayer } from '@/components/world/layer-tabs';
import { SideChecklist } from '@/components/world/side-checklist';
import { WorldHeader } from '@/components/world/world-header';
import { ErrorState, LoadingState } from '@/components/ui/states';
import { useWorldKbGraph } from '@/lib/canvas/use-world-kb-data';
import { useNexusClient } from '@/lib/client-context';
import { NexusClientError } from '@/lib/nexus';
import { useToast } from '@/lib/use-toast';
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
          <BriefLayer worldId={worldId} eras={eras} />
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

/**
 * Brief layer body (plan v1219-world-module T2 + QC fix wave): the guided
 * empty state → inline editor → saved card flow. The Brief read model is the
 * era KnowledgeEntry projection; an era with a non-empty `world_summary` is
 * the saved Brief (renders as {@link BriefCard}), otherwise the empty state
 * with the `+ 写 Brief` action. Eras without a summary keep the T1 plain list
 * item. The `世界已就绪` follow-up card announces the Work unlock after the
 * just-saved session only (T2 review gating); the mutation owns the sole
 * `unlock` call site (D8/D17), and the side checklist tracks the 3-step
 * onboarding progress. Leaving edit mode is safe without a flash because
 * {@link useSaveBrief} awaits the graph invalidation inside `onSuccess` — by
 * the time the editor's `mutateAsync` resolves, the eras prop already
 * contains the saved entity.
 *
 * Edit target snapshot (QC fix wave): captured at edit entry so a mid-edit
 * era deletion cannot flip the editor from the OCC patch path to a create.
 * While the era still exists in the graph, the observed version/body track
 * the live projection — a concurrent-write 409 refreshes the graph and the
 * mounted editor picks up the fresh revision for the promised retry.
 */
type EditTarget =
  | { kind: 'new' }
  | {
      kind: 'era';
      entityId: string;
      version: number;
      body: Record<string, unknown> | undefined;
      summary: string | null;
    };

function BriefLayer({ worldId, eras }: { worldId: string; eras: WorldKbEntityProjection[] }) {
  const { t } = useTranslation('worlds');
  const { toast } = useToast();
  const [editTarget, setEditTarget] = useState<EditTarget | null>(null);
  // BriefReadyCard mounts only for the just-saved session (T2 review note) —
  // a revisit with an existing Brief, or an edit re-save after the unlock,
  // does not re-announce.
  const [justSaved, setJustSaved] = useState(false);

  const withSummary = eras.filter((era) => eraSummary(era) !== null);
  const withoutSummary = eras.filter((era) => eraSummary(era) === null);
  const brief = withSummary[0] ?? null;
  const liveEra =
    editTarget?.kind === 'era'
      ? (eras.find((era) => era.key_block_id === editTarget.entityId) ?? null)
      : null;
  const editEntity =
    editTarget?.kind === 'era'
      ? {
          entityId: editTarget.entityId,
          version: liveEra ? liveEra.version : editTarget.version,
          body: liveEra ? (liveEra.body as Record<string, unknown> | undefined) : editTarget.body,
        }
      : undefined;
  const editInitialText =
    editTarget?.kind === 'era'
      ? ((liveEra ? eraSummary(liveEra) : editTarget.summary) ?? undefined)
      : undefined;

  return (
    <div className="flex items-start gap-4">
      <div className="flex min-w-0 flex-1 flex-col gap-2">
        {editTarget !== null ? (
          <BriefEditor
            worldId={worldId}
            initialText={editInitialText}
            entity={editEntity}
            onCancel={() => setEditTarget(null)}
            onSaved={() => {
              setEditTarget(null);
            }}
            onFirstUnlock={() => {
              setJustSaved(true);
              toast({ variant: 'success', title: t('worldApp.briefSavedToast') });
            }}
          />
        ) : (
          <>
            {!brief && (
              <BriefEmpty onCreate={() => setEditTarget({ kind: 'new' })} />
            )}
            {withSummary.length > 0 && (
              <ul className="flex flex-col gap-2">
                {withSummary.map((era) => (
                  <li key={era.key_block_id}>
                    <BriefCard
                      text={eraSummary(era)!}
                      title={era.canonical_name}
                      updatedAt={era.updated_at}
                      onEdit={() =>
                        setEditTarget({
                          kind: 'era',
                          entityId: era.key_block_id,
                          version: era.version,
                          body: era.body as Record<string, unknown> | undefined,
                          summary: eraSummary(era),
                        })
                      }
                    />
                  </li>
                ))}
              </ul>
            )}
            {withoutSummary.length > 0 && (
              <ul className="flex flex-col gap-2">
                {withoutSummary.map((era) => (
                  <li key={era.key_block_id} className="rounded-card border border-gray-500 p-4">
                    <h3 className="text-heading-16 font-heading text-gray-1000">
                      {era.canonical_name}
                    </h3>
                  </li>
                ))}
              </ul>
            )}
            {justSaved && <BriefReadyCard />}
          </>
        )}
      </div>
      <aside className="w-64 shrink-0">
        <SideChecklist briefDone={brief !== null} />
      </aside>
    </div>
  );
}
