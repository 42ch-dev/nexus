import { useCallback, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useNavigate } from 'react-router-dom';

import { flattenPages, useCreateWork, useCreateWorld, useNarrativeWorlds, useWorks } from '@/api/queries';
import { HubDualPaneChrome } from '@/components/layout/presentational/hub-dual-pane-chrome';
import type { HubCardListItem } from '@/components/layout/presentational/hub-card-list-pane';
import type { HubTab } from '@/components/layout/presentational/hub-tab-bar';
import { useNexusClient } from '@/lib/client-context';
import { hasCreateWorldClient } from '@/lib/nexus/create-world';
import { useToast } from '@/lib/use-toast';

export function resolveInitialHubTab(worldCount: number, workCount: number): HubTab {
  if (worldCount > 0) return 'world';
  if (workCount > 0) return 'work';
  return 'world';
}

/**
 * Wired Creator Hub dual-pane — shared tab SSOT, inline create, canvas navigation (V1.134 P3).
 */
export function CreatorHubDualPane() {
  const { t } = useTranslation(['shell', 'worlds']);
  const navigate = useNavigate();
  const { toast } = useToast();
  const client = useNexusClient();
  const canCreateWorld = useMemo(() => hasCreateWorldClient(client), [client]);
  const createWork = useCreateWork();
  const createWorld = useCreateWorld();

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
  const [forceExpandedCreate, setForceExpandedCreate] = useState(false);

  const activeItems = activeTab === 'world' ? worldItems : workItems;
  const createExpanded = forceExpandedCreate || activeItems.length === 0;
  const isCreateSubmitting = createWork.isPending || createWorld.isPending;

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
        submittingLabel: t('shell:workCreate.creating'),
      },
      cardList: {
        emptyWorlds: t('shell:hub.empty.worlds'),
        emptyWorks: t('shell:hub.empty.works'),
      },
    }),
    [t],
  );

  const handleTabChange = useCallback((tab: HubTab) => {
    setActiveTab(tab);
    setForceExpandedCreate(false);
  }, []);

  const handleCreateSubmit = useCallback(
    async (title: string) => {
      const trimmed = title.trim();
      if (!trimmed) return;

      if (activeTab === 'world') {
        if (!canCreateWorld) return;
        try {
          await createWorld.mutateAsync({ title: trimmed });
          setForceExpandedCreate(false);
        } catch {
          // Error toast handled by mutation onError.
        }
        return;
      }

      try {
        // V1.134 inline minimum: title-only UX; required wire fields seeded from title.
        await createWork.mutateAsync({
          title: trimmed,
          long_term_goal: trimmed,
          initial_idea: trimmed,
        });
        toast({
          variant: 'success',
          title: t('shell:workCreate.toastCreated'),
        });
        setForceExpandedCreate(false);
      } catch {
        // Error toast handled by mutation onError.
      }
    },
    [activeTab, canCreateWorld, createWorld, createWork, toast, t],
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
    <HubDualPaneChrome
      className="h-full min-h-0 rounded-none border-0"
      activeTab={activeTab}
      onTabChange={handleTabChange}
      worlds={worldItems}
      works={workItems}
      labels={labels}
      createExpanded={createExpanded}
      onCreateSubmit={(title) => {
        void handleCreateSubmit(title);
      }}
      onExpandCreate={() => setForceExpandedCreate(true)}
      isCreateSubmitting={isCreateSubmitting}
      canCreateWorld={canCreateWorld}
      createWorldDisabledTitle={t('worlds:create.desktop-only')}
      onSelectCard={handleSelectCard}
      tabBarAriaLabel={t('shell:hub.tabs.ariaLabel')}
      data-testid="creator-hub-dual-pane"
    />
  );
}
