import { useEffect, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useLocation, useNavigate } from 'react-router';
import {
  BrainCircuit,
  CalendarClock,
  ListChecks,
  Sparkles,
} from 'lucide-react';

import { useCreateWork, useCreateWorld } from '@/api/queries';
import { FooterProfiles } from '@/components/layout/footer-profiles';
import {
  CreatorShellContent,
  type CreatorShellInlineWorkSubmit,
} from '@/components/layout/presentational/creator-shell-content';
import {
  ShellSidebarChrome,
  type ShellNavGroup,
  type ShellSidebarTab,
} from '@/components/layout/presentational/shell-sidebar-chrome';
import { useNexusClient } from '@/lib/client-context';
import { hasCreateWorldClient } from '@/lib/nexus/create-world';
import { useToast } from '@/lib/use-toast';
import { isWorkProfile, WORK_PROFILES } from '@/lib/work-profiles';

/**
 * Sidebar nav — V1.94 two-tab IA (Creator | Orchestrator).
 *
 * V1.135 P0: hub create lives in {@link CreatorCreatePanel} as
 * `panelContent` on every creator-tab surface, including `/works` and `/worlds`.
 * Hub content is browse-only (tabs + card list).
 *
 * Thin wrapper around {@link ShellSidebarChrome}: owns the active creator
 * profile, route-derived tab state, and inline create mutations (V1.136 P1).
 */
const ORCHESTRATOR_ROUTE_PREFIXES = [
  '/memory',
  '/strategies',
  '/sessions',
  '/schedule',
  '/modules',
] as const;

function tabFromPathname(pathname: string): ShellSidebarTab {
  return ORCHESTRATOR_ROUTE_PREFIXES.some(
    (prefix) => pathname === prefix || pathname.startsWith(`${prefix}/`),
  )
    ? 'orchestrator'
    : 'creator';
}

function CreatorCreatePanel() {
  const { t: tShell } = useTranslation('shell');
  const { t: tWorlds } = useTranslation('worlds');
  const { t: tCommon } = useTranslation('common');
  const navigate = useNavigate();
  const { toast } = useToast();
  const client = useNexusClient();
  const canCreateWorld = useMemo(() => hasCreateWorldClient(client), [client]);
  const createWorld = useCreateWorld();
  const createWork = useCreateWork();

  const inlineLabels = useMemo(
    () => ({
      tabs: {
        world: tShell('hub.tabs.world'),
        work: tShell('hub.tabs.work'),
      },
      tabsAriaLabel: tShell('hub.create.tabs.ariaLabel'),
      world: {
        titleLabel: tShell('worldCreate.titleLabel'),
        titlePlaceholder: tShell('worldCreate.titlePlaceholder'),
        submit: createWorld.isPending ? tShell('worldCreate.creating') : tShell('worldCreate.create'),
        disabledTitle: tWorlds('create.desktop-only'),
      },
      work: {
        titleLabel: tShell('workCreate.titleLabel'),
        titlePlaceholder: tShell('workCreate.titlePlaceholder'),
        goalLabel: tShell('workCreate.goalLabel'),
        goalPlaceholder: tShell('workCreate.goalPlaceholder'),
        ideaLabel: tShell('workCreate.ideaLabel'),
        ideaPlaceholder: tShell('workCreate.ideaPlaceholder'),
        profileLabel: tShell('workCreate.profileLabel'),
        profileOptions: WORK_PROFILES.map((profile) => ({
          value: profile.value,
          label: tCommon(`status.${profile.value}`),
        })),
        submit: createWork.isPending ? tShell('workCreate.creating') : tShell('workCreate.create'),
      },
    }),
    [createWork.isPending, createWorld.isPending, tCommon, tShell, tWorlds],
  );

  async function handleWorldSubmit(title: string) {
    const res = await createWorld.mutateAsync({ title });
    navigate(`/worlds/${encodeURIComponent(res.world_id)}/timeline`);
  }

  async function handleWorkSubmit(payload: CreatorShellInlineWorkSubmit) {
    const res = await createWork.mutateAsync({
      title: payload.title,
      long_term_goal: payload.longTermGoal,
      initial_idea: payload.initialIdea,
      ...(payload.workProfile && isWorkProfile(payload.workProfile)
        ? { work_profile: payload.workProfile }
        : {}),
    });
    toast({ variant: 'success', title: tShell('workCreate.toastCreated'), description: res.work_id });
    navigate(`/works/${encodeURIComponent(res.work_id)}/outline`);
  }

  return (
    <CreatorShellContent
      mode="create-inline"
      canCreateWorld={canCreateWorld}
      labels={inlineLabels}
      worldIsPending={createWorld.isPending}
      workIsPending={createWork.isPending}
      onWorldSubmit={(title) => handleWorldSubmit(title)}
      onWorkSubmit={(payload) => handleWorkSubmit(payload)}
      data-testid="sidebar-create-panel"
    />
  );
}

const CREATOR_HUB_PATH = '/works';
const ORCHESTRATOR_HUB_PATH = '/strategies';

export function Sidebar() {
  const { t } = useTranslation('shell');
  const { pathname } = useLocation();
  const navigate = useNavigate();
  const [activeTab, setActiveTab] = useState<ShellSidebarTab>(() => tabFromPathname(pathname));

  useEffect(() => {
    setActiveTab(tabFromPathname(pathname));
  }, [pathname]);

  function handleTabChange(tab: ShellSidebarTab) {
    const routeTab = tabFromPathname(pathname);
    if (tab !== routeTab) {
      navigate(tab === 'creator' ? CREATOR_HUB_PATH : ORCHESTRATOR_HUB_PATH);
    }
    setActiveTab(tab);
  }

  const orchestratorGroups: ShellNavGroup[] = useMemo(
    () => [
      {
        id: 'memory',
        label: t('nav.memory'),
        items: [{ to: '/memory', label: t('nav.memory'), icon: BrainCircuit }],
      },
      {
        id: 'strategies',
        label: t('nav.strategies'),
        items: [{ to: '/strategies', label: t('nav.strategies'), icon: Sparkles }],
      },
      {
        id: 'runtime',
        label: t('nav.runtime'),
        items: [
          { to: '/sessions', label: t('nav.sessions'), icon: ListChecks },
          { to: '/schedule', label: t('nav.schedule'), icon: CalendarClock },
        ],
      },
    ],
    [t],
  );

  const creatorPanel = activeTab === 'creator' ? <CreatorCreatePanel /> : undefined;

  return (
    <nav aria-label={t('aria.primary')} className="min-h-0 flex-1">
      <ShellSidebarChrome
        activeTab={activeTab}
        activeRoute={pathname}
        navGroups={activeTab === 'orchestrator' ? orchestratorGroups : []}
        panelContent={creatorPanel}
        onTabChange={handleTabChange}
        footer={<FooterProfiles />}
        creatorTabLabel={t('nav.creator')}
        orchestratorTabLabel={t('nav.orchestrator')}
        primaryNavigationAriaLabel={t('aria.primaryNavigation')}
      />
    </nav>
  );
}

// Re-export the chrome types for convenience.
export type { ShellNavGroup, ShellSidebarTab };
