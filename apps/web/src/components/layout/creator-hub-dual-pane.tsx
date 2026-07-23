import { useCallback, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useNavigate } from 'react-router-dom';

import { flattenPages, useNarrativeWorlds, useWorks } from '@/api/queries';
import { HubDualPaneChrome } from '@/components/layout/presentational/hub-dual-pane-chrome';
import type { HubCardListItem } from '@/components/layout/presentational/hub-card-list-pane';
import type { HubTab } from '@/components/layout/presentational/hub-tab-bar';
import { CreateWorldDialog } from '@/components/worlds/create-world-dialog';
import { useNexusClient } from '@/lib/client-context';
import { hasCreateWorldClient } from '@/lib/nexus/create-world';
import { CreateWorkDialog } from '@/pages/dialogs/create-work-dialog';

export function resolveInitialHubTab(worldCount: number, workCount: number): HubTab {
  if (worldCount > 0) return 'world';
  if (workCount > 0) return 'work';
  return 'world';
}

/**
 * Wired Creator Hub dual-pane — shared tab SSOT, queries, and canvas navigation (V1.134 P3).
 */
export function CreatorHubDualPane() {
  const { t } = useTranslation(['shell', 'worlds']);
  const navigate = useNavigate();
  const client = useNexusClient();
  const canCreateWorld = useMemo(() => hasCreateWorldClient(client), [client]);

  const worksQuery = useWorks({ limit: 12 });
  const works = useMemo(() => flattenPages(worksQuery.data), [worksQuery.data]);
  const worldsQuery = useNarrativeWorlds({ limit: 12 });
  const worlds = useMemo(() => worldsQuery.data ?? [], [worldsQuery.data]);

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

  const [activeTab, setActiveTab] = useState<HubTab>(() =>
    resolveInitialHubTab(worldItems.length, workItems.length),
  );
  const [createWorkOpen, setCreateWorkOpen] = useState(false);
  const [createWorldOpen, setCreateWorldOpen] = useState(false);

  const labels = useMemo(
    () => ({
      tabs: {
        world: t('shell:hub.tabs.world'),
        work: t('shell:hub.tabs.work'),
      },
      workspace: {
        createWorldTitle: t('worlds:emptyCreateWorldTitle'),
        createWorldDescription: t('worlds:emptyCreateWorldDescription'),
        createWorkTitle: t('worlds:emptyCreateWorkTitle'),
        createWorkDescription: t('worlds:emptyCreateWorkDescription'),
        createWorldCompact: t('shell:hub.workspace.createWorldCompact'),
        createWorkCompact: t('shell:hub.workspace.createWorkCompact'),
        titleLabel: t('shell:workCreate.titleLabel'),
        titlePlaceholder: t('shell:workCreate.titlePlaceholder'),
        submitLabel: t('shell:workCreate.create'),
      },
      cardList: {
        emptyWorlds: t('shell:hub.empty.worlds'),
        emptyWorks: t('shell:hub.empty.works'),
      },
    }),
    [t],
  );

  const openCreateDialog = useCallback(() => {
    if (activeTab === 'world') {
      if (canCreateWorld) {
        setCreateWorldOpen(true);
      }
      return;
    }
    setCreateWorkOpen(true);
  }, [activeTab, canCreateWorld]);

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
    <>
      <HubDualPaneChrome
        className="h-full min-h-0 rounded-none border-0"
        activeTab={activeTab}
        onTabChange={setActiveTab}
        worlds={worldItems}
        works={workItems}
        labels={labels}
        onCreateSubmit={() => openCreateDialog()}
        onExpandCreate={openCreateDialog}
        onSelectCard={handleSelectCard}
        tabBarAriaLabel={t('shell:hub.tabs.ariaLabel')}
        data-testid="creator-hub-dual-pane"
      />
      <CreateWorldDialog open={createWorldOpen} onOpenChange={setCreateWorldOpen} />
      <CreateWorkDialog
        open={createWorkOpen}
        onOpenChange={setCreateWorkOpen}
        onCreated={(workId) => {
          navigate(`/works/${encodeURIComponent(workId)}/outline`);
        }}
      />
    </>
  );
}
