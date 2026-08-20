import { useEffect, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useLocation, useNavigate } from 'react-router';

import { useCreateWork, useCreateWorld } from '@/api/queries';
import { FooterProfiles } from '@/components/layout/footer-profiles';
import { useEntrance } from '@/lib/entrance-context';
import { ENTRANCE_BY_ID } from '@/components/layout/entrance-registry';
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

/**
 * Footer "Switch entrance" affordance (V1.170 P1 — AR-15 / EL §2).
 *
 * Persistent shell-chrome control — NOT a third top-level tab (the
 * Creator | Orchestrator tabs stay untouched). Opens the identity page, which
 * is the two-option control; only its Continue persists (AR-20). Shows the
 * current layout name so the affordance doubles as a status readout.
 */
function EntranceSwitchControl() {
  const { t } = useTranslation('shell');
  const { entrance } = useEntrance();
  const navigate = useNavigate();
  return (
    <button
      type="button"
      onClick={() => navigate('/entrance')}
      data-testid="entrance-switch-control"
      className="flex w-full items-center justify-between gap-2 rounded-control px-2 py-1.5 text-button-14 font-button text-gray-700 transition-colors duration-state ease-standard motion-reduce:transition-none hover:bg-gray-alpha-100 hover:text-gray-1000 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-700"
    >
      <span>{t('entrance.switchLabel')}</span>
      <span className="text-label-12 font-medium text-gray-900">
        {t(`entrance.layout.${ENTRANCE_BY_ID[entrance].id}`)}
      </span>
    </button>
  );
}

export function Sidebar() {
  const { t } = useTranslation('shell');
  const { pathname } = useLocation();
  const { entrance } = useEntrance();
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

  // V1.170 P1 (AR-15) — the nav slot renders the entrance-filtered tree from
  // the registry (labels are `shell` i18n keys, resolved here). Create omits
  // the EL §3 hide-table surfaces; Develop shows the full Control Room + hub.
  // The Creator|Orchestrator tabs stay untouched — entrance is a separate
  // axis (`tabFromPathname` unchanged), so the groups render in the
  // orchestrator nav slot exactly like the V1.94 orchestrator groups did.
  const navGroups: ShellNavGroup[] = useMemo(
    () =>
      ENTRANCE_BY_ID[entrance].navGroups.map((group) => ({
        ...group,
        label: t(group.label),
        items: group.items.map((item) => ({ ...item, label: t(item.label) })),
      })),
    [entrance, t],
  );

  const creatorPanel = activeTab === 'creator' ? <CreatorCreatePanel /> : undefined;

  return (
    <nav aria-label={t('aria.primary')} className="min-h-0 flex-1">
      <ShellSidebarChrome
        activeTab={activeTab}
        activeRoute={pathname}
        navGroups={activeTab === 'orchestrator' ? navGroups : []}
        panelContent={creatorPanel}
        onTabChange={handleTabChange}
        footer={
          <>
            <EntranceSwitchControl />
            <FooterProfiles />
          </>
        }
        creatorTabLabel={t('nav.creator')}
        orchestratorTabLabel={t('nav.orchestrator')}
        primaryNavigationAriaLabel={t('aria.primaryNavigation')}
      />
    </nav>
  );
}

// Re-export the chrome types for convenience.
export type { ShellNavGroup, ShellSidebarTab };
