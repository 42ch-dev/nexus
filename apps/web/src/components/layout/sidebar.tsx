import { useEffect, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useLocation, useNavigate } from 'react-router-dom';
import {
  BrainCircuit,
  CalendarClock,
  ListChecks,
  Sparkles,
} from 'lucide-react';

import { FooterProfiles } from '@/components/layout/footer-profiles';
import { CreatorShellContent } from '@/components/layout/presentational/creator-shell-content';
import {
  ShellSidebarChrome,
  type ShellNavGroup,
  type ShellSidebarTab,
} from '@/components/layout/presentational/shell-sidebar-chrome';
import { CreateWorldDialog } from '@/components/worlds/create-world-dialog';
import { useNexusClient } from '@/lib/client-context';
import { hasCreateWorldClient } from '@/lib/nexus/create-world';
import { CreateWorkDialog } from '@/pages/dialogs/create-work-dialog';

/**
 * Sidebar nav — V1.94 two-tab IA (Creator | Orchestrator).
 *
 * V1.132 P3 (AC-8): 创作 hub left is Create-only (创建 World / 延续 Work);
 * Worlds/Works lists live in the right content region via
 * {@link CreatorEntityListsPanel}. Orchestrator tab keeps Memory / Runtime /
 * Strategies nav groups.
 *
 * Thin wrapper around {@link ShellSidebarChrome}: owns the active creator
 * profile, route-derived tab state, and Create dialog orchestration.
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
  const { t: tWorlds } = useTranslation('worlds');
  const navigate = useNavigate();
  const client = useNexusClient();
  const canCreateWorld = useMemo(() => hasCreateWorldClient(client), [client]);
  const [createWorkOpen, setCreateWorkOpen] = useState(false);
  const [createWorldOpen, setCreateWorldOpen] = useState(false);

  return (
    <>
      <CreatorShellContent
        mode="create"
        canCreateWorld={canCreateWorld}
        labels={{
          createWorldTitle: tWorlds('emptyCreateWorldTitle'),
          createWorldDescription: tWorlds('emptyCreateWorldDescription'),
          createWorkTitle: tWorlds('emptyCreateWorkTitle'),
          createWorkDescription: tWorlds('emptyCreateWorkDescription'),
          createWorldDisabledTitle: tWorlds('create.desktop-only'),
        }}
        onCreateWorld={() => setCreateWorldOpen(true)}
        onCreateWork={() => setCreateWorkOpen(true)}
        data-testid="sidebar-create-panel"
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

export function Sidebar() {
  const { t } = useTranslation('shell');
  const { pathname } = useLocation();
  const [activeTab, setActiveTab] = useState<ShellSidebarTab>(() => tabFromPathname(pathname));

  useEffect(() => {
    setActiveTab(tabFromPathname(pathname));
  }, [pathname]);

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
        onTabChange={setActiveTab}
        footer={activeTab === 'orchestrator' ? <FooterProfiles /> : null}
        creatorTabLabel={t('nav.creator')}
        orchestratorTabLabel={t('nav.orchestrator')}
        primaryNavigationAriaLabel={t('aria.primaryNavigation')}
      />
    </nav>
  );
}

// Re-export the chrome types for convenience.
export type { ShellNavGroup, ShellSidebarTab };
