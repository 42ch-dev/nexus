import type { ReactNode } from 'react';

import { cn } from '@/lib/utils';

import {
  HubCardListPane,
  type HubCardListItem,
  type HubCardListPaneLabels,
} from './hub-card-list-pane';
import { HubTabBar, type HubTab, type HubTabBarLabels } from './hub-tab-bar';
import {
  HubWorkspacePane,
  type HubWorkspacePaneLabels,
} from './hub-workspace-pane';

export type HubDualPaneChromeLabels = {
  tabs: HubTabBarLabels;
  workspace: HubWorkspacePaneLabels;
  cardList: HubCardListPaneLabels;
};

export type HubDualPaneChromeProps = {
  activeTab: HubTab;
  onTabChange: (tab: HubTab) => void;
  worlds: HubCardListItem[];
  works: HubCardListItem[];
  labels: HubDualPaneChromeLabels;
  onCreateSubmit?: (title: string) => void;
  onExpandCreate?: () => void;
  onSelectCard?: (id: string) => void;
  tabBarAriaLabel?: string;
  /** When omitted, derived from active tab item count (expanded when zero). */
  createExpanded?: boolean;
  isCreateSubmitting?: boolean;
  createErrorMessage?: string | null;
  canCreateWorld?: boolean;
  createWorldDisabledTitle?: string;
  header?: ReactNode;
  className?: string;
  'data-testid'?: string;
};

/**
 * Creator Hub dual-pane chrome — shared tab bar + workspace + card list (V1.134 P3).
 *
 * Presentational extract consumed by App hub routes and Design Studio
 * fixtures via `@web-layout/hub-dual-pane-chrome`. Host owns tab SSOT,
 * queries, and i18n labels.
 */
export function HubDualPaneChrome({
  activeTab,
  onTabChange,
  worlds,
  works,
  labels,
  onCreateSubmit,
  onExpandCreate,
  onSelectCard,
  tabBarAriaLabel,
  createExpanded,
  isCreateSubmitting,
  createErrorMessage,
  canCreateWorld,
  createWorldDisabledTitle,
  header,
  className,
  'data-testid': testId = 'hub-dual-pane-chrome',
}: HubDualPaneChromeProps) {
  const activeItems = activeTab === 'world' ? worlds : works;
  const expanded = createExpanded ?? activeItems.length === 0;

  return (
    <div
      className={cn(
        'flex min-h-[420px] flex-col overflow-hidden rounded-card border border-gray-alpha-300 bg-background-100',
        className,
      )}
      data-testid={testId}
      data-active-tab={activeTab}
    >
      {header}
      <HubTabBar
        activeTab={activeTab}
        onTabChange={onTabChange}
        labels={labels.tabs}
        ariaLabel={tabBarAriaLabel}
        data-testid={`${testId}-tab-bar`}
      />
      <div
        id="hub-tabpanel"
        role="tabpanel"
        aria-labelledby={`hub-tab-${activeTab}`}
        className="flex min-h-0 flex-1"
        data-testid={`${testId}-tabpanel`}
      >
        <div
          className="w-full max-w-[22rem] shrink-0 border-r border-gray-alpha-400 bg-background-100"
          data-testid={`${testId}-workspace`}
        >
          <HubWorkspacePane
            activeTab={activeTab}
            labels={labels.workspace}
            createExpanded={expanded}
            onSubmit={onCreateSubmit}
            onExpandCreate={onExpandCreate}
            isSubmitting={isCreateSubmitting}
            errorMessage={createErrorMessage}
            canCreateWorld={canCreateWorld}
            createWorldDisabledTitle={createWorldDisabledTitle}
            data-testid={`${testId}-workspace-pane`}
          />
        </div>
        <div
          className="min-w-0 flex-1"
          data-testid={`${testId}-card-list`}
        >
          <HubCardListPane
            activeTab={activeTab}
            worlds={worlds}
            works={works}
            labels={labels.cardList}
            onSelectCard={onSelectCard}
            data-testid={`${testId}-card-list-pane`}
          />
        </div>
      </div>
    </div>
  );
}
