import { useCallback, useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import { useNavigate } from 'react-router-dom';

import { flattenPages, useNarrativeWorlds, useWorks } from '@/api/queries';
import {
  HubCardListPane,
  type HubCardListItem,
} from '@/components/layout/presentational/hub-card-list-pane';
import { HubTabBar } from '@/components/layout/presentational/hub-tab-bar';
import { useHubTabState } from '@/components/layout/use-hub-tab-state';
import { cn } from '@/lib/utils';

export { resolveInitialHubTab } from '@/components/layout/use-hub-tab-state';

/**
 * Wired Creator Hub browse surface — shared tab SSOT, card list, canvas navigation (V1.135 P0).
 *
 * Create lives in {@link Sidebar} `panelContent` ({@link CreatorCreatePanel}), not here.
 */
export function CreatorHubDualPane() {
  const { t } = useTranslation(['shell', 'common']);
  const navigate = useNavigate();

  const worksQuery = useWorks({ limit: 12 });
  const works = useMemo(() => flattenPages(worksQuery.data), [worksQuery.data]);
  const worldsQuery = useNarrativeWorlds({ limit: 12 });
  const worlds = useMemo(() => worldsQuery.data ?? [], [worldsQuery.data]);
  const isListsLoading = worksQuery.isPending || worldsQuery.isPending;

  const worldItems: HubCardListItem[] = useMemo(
    () =>
      worlds.map((world) => ({
        id: world.world_id,
        label: world.title || world.world_id,
      })),
    [worlds],
  );

  const workItems: HubCardListItem[] = useMemo(
    () =>
      works.map((work) => ({
        id: work.work_id,
        label: work.title,
      })),
    [works],
  );

  const { activeTab, onTabChange } = useHubTabState(
    worldItems.length,
    workItems.length,
    isListsLoading,
  );

  const labels = useMemo(
    () => ({
      tabs: {
        world: t('shell:hub.tabs.world'),
        work: t('shell:hub.tabs.work'),
      },
      cardList: {
        emptyWorlds: t('shell:hub.empty.worlds'),
        emptyWorks: t('shell:hub.empty.works'),
      },
    }),
    [t],
  );

  const handleSelectCard = useCallback(
    (id: string) => {
      if (activeTab === 'world') {
        navigate(`/worlds/${encodeURIComponent(id)}/timeline`);
        return;
      }
      navigate(`/works/${encodeURIComponent(id)}/outline`);
    },
    [activeTab, navigate],
  );

  return (
    <div
      className={cn(
        'flex h-full min-h-0 flex-col overflow-hidden rounded-none border-0 bg-background-100',
      )}
      data-testid="creator-hub-dual-pane"
      data-active-tab={activeTab}
    >
      <HubTabBar
        activeTab={activeTab}
        onTabChange={onTabChange}
        labels={labels.tabs}
        ariaLabel={t('shell:hub.tabs.ariaLabel')}
        data-testid="creator-hub-dual-pane-tab-bar"
      />
      <div
        id="hub-tabpanel"
        role="tabpanel"
        aria-labelledby={`hub-tab-${activeTab}`}
        className="min-h-0 flex-1"
        data-testid="creator-hub-dual-pane-tabpanel"
      >
        <HubCardListPane
          activeTab={activeTab}
          worlds={worldItems}
          works={workItems}
          labels={labels.cardList}
          onSelectCard={handleSelectCard}
          isListLoading={isListsLoading}
          loadingLabel={t('common:status.loading')}
          data-testid="creator-hub-dual-pane-card-list-pane"
        />
      </div>
    </div>
  );
}
