/**
 * Studio fixtures for V1.135 P0 / V1.136 P1 Creator Hub sidebar-create IA.
 *
 * Proves PAC-5 acceptance matrix:
 * - Create in shell sidebar menu slot (`ShellSidebarChrome.panelContent`)
 * - V1.136: inline create zone (World|Work tabs + form + submit), not dashed cards
 * - Content = World / Work tabs + card list / empty only — no content-left create
 * - World / Work × empty / populated × light / dark (8 variants)
 *
 * Composes `@web-layout/shell-sidebar-chrome`, `@web-layout/creator-shell-content`,
 * `@web-layout/hub-tab-bar`, and `@web-layout/hub-card-list-pane` without App providers.
 */
import { useState, type ReactNode } from 'react';

import { StudioShellLogo } from '@/components/studio-shell-logo';

import {
  CreatorShellContent,
  type CreatorShellInlineCreateLabels,
} from '@web-layout/creator-shell-content';
import {
  HubCardListPane,
  type HubCardListItem,
  type HubCardListPaneLabels,
} from '@web-layout/hub-card-list-pane';
import { HubTabBar, type HubTab, type HubTabBarLabels } from '@web-layout/hub-tab-bar';
import {
  ShellSidebarChrome,
  type ShellSidebarTab,
} from '@web-layout/shell-sidebar-chrome';

import { FixtureWorkspaceFooter } from '@/fixtures/hub-sidebar-fixture-chrome';

const INLINE_CREATE_LABELS_EN = {
  tabs: { world: 'Worlds', work: 'Works' },
  tabsAriaLabel: 'Sidebar create — World or Work',
  world: {
    titleLabel: 'Title',
    titlePlaceholder: 'The World\'s name',
    submit: 'Create',
    disabledTitle: 'Open in the desktop app to create a World',
  },
  work: {
    titleLabel: 'Title',
    titlePlaceholder: 'The Work\'s name',
    goalLabel: 'Long-term goal',
    goalPlaceholder: 'Where this Work is heading',
    ideaLabel: 'Initial idea',
    ideaPlaceholder: 'The seed the runtime will build on',
    profileLabel: 'Work profile',
    profileOptions: [
      { value: 'novel', label: 'Novel' },
      { value: 'essay', label: 'Essay' },
    ],
    submit: 'Create',
  },
} satisfies CreatorShellInlineCreateLabels;

const INLINE_CREATE_LABELS_ZH = {
  tabs: { world: '世界', work: '作品' },
  tabsAriaLabel: '侧边栏创建 — 世界或作品',
  world: {
    titleLabel: '标题',
    titlePlaceholder: '世界名称',
    submit: '创建',
    disabledTitle: '创建 World 仅在 Nexus 桌面应用中可用。',
  },
  work: {
    titleLabel: '标题',
    titlePlaceholder: '作品名称',
    goalLabel: '长期目标',
    goalPlaceholder: '这部作品的发展方向',
    ideaLabel: '初始想法',
    ideaPlaceholder: '运行时将要构建的种子',
    profileLabel: '作品类型',
    profileOptions: [
      { value: 'novel', label: '小说' },
      { value: 'essay', label: '散文' },
    ],
    submit: '创建',
  },
} satisfies CreatorShellInlineCreateLabels;

type HubBrowseLabels = {
  tabs: HubTabBarLabels;
  cardList: HubCardListPaneLabels;
};

const HUB_LABELS_EN: HubBrowseLabels = {
  tabs: { world: 'Worlds', work: 'Works' },
  cardList: {
    emptyWorlds: 'No Worlds yet — create one from the sidebar',
    emptyWorks: 'No Works yet — create one from the sidebar',
    emptyWorldsKey: 'hub.empty.worlds',
    emptyWorksKey: 'hub.empty.works',
  },
};

const HUB_LABELS_ZH: HubBrowseLabels = {
  tabs: { world: '世界', work: '作品' },
  cardList: {
    emptyWorlds: '暂无世界，从侧边栏创建',
    emptyWorks: '暂无作品，从侧边栏创建',
    emptyWorldsKey: 'hub.empty.worlds',
    emptyWorksKey: 'hub.empty.works',
  },
};

const SAMPLE_WORLDS: HubCardListItem[] = [
  { id: 'world-fantasy', label: '奇幻大陆' },
  { id: 'world-scifi', label: '近未来都市' },
];

const SAMPLE_WORKS: HubCardListItem[] = [
  { id: 'work-novel', label: '漫漫长路' },
  { id: 'work-essay', label: '随笔集' },
];

type VariantState = {
  activeTab: HubTab;
  worlds: HubCardListItem[];
  works: HubCardListItem[];
  label: string;
};

const VARIANT_STATES: VariantState[] = [
  {
    activeTab: 'world',
    worlds: [],
    works: [],
    label: 'World · empty',
  },
  {
    activeTab: 'world',
    worlds: SAMPLE_WORLDS,
    works: SAMPLE_WORKS,
    label: 'World · populated',
  },
  {
    activeTab: 'work',
    worlds: SAMPLE_WORLDS,
    works: [],
    label: 'Work · empty',
  },
  {
    activeTab: 'work',
    worlds: SAMPLE_WORLDS,
    works: SAMPLE_WORKS,
    label: 'Work · populated',
  },
];

function FixtureFrame({
  title,
  description,
  testId,
  children,
}: {
  title: string;
  description: string;
  testId: string;
  children: ReactNode;
}) {
  return (
    <div
      className="mb-8 rounded-card border border-gray-alpha-200 bg-background-100 p-4"
      data-testid={testId}
    >
      <h4 className="mb-1 text-heading-16 font-heading text-gray-1000">{title}</h4>
      <p className="mb-4 text-copy-13 text-gray-700">{description}</p>
      {children}
    </div>
  );
}

function ThemePair({
  testId,
  light,
  dark,
}: {
  testId: string;
  light: ReactNode;
  dark: ReactNode;
}) {
  return (
    <div
      data-testid={testId}
      className="grid grid-cols-1 gap-4 sm:grid-cols-2"
    >
      <div
        data-testid={`${testId}-light`}
        className="rounded-card border border-gray-alpha-300 bg-background-100 p-2"
      >
        <p className="mb-2 px-2 pt-2 text-label-14 font-medium text-gray-1000">Light</p>
        {light}
      </div>
      <div
        data-testid={`${testId}-dark`}
        className="dark rounded-card border border-gray-alpha-300 bg-background-100 p-2"
      >
        <p className="mb-2 px-2 pt-2 text-label-14 font-medium text-brand-cyan">Dark</p>
        {dark}
      </div>
    </div>
  );
}

function HubSidebarBrowseFrame({
  activeTab: initialTab,
  worlds,
  works,
  labels,
  createLabels,
  testId,
  interactive = false,
}: {
  activeTab: HubTab;
  worlds: HubCardListItem[];
  works: HubCardListItem[];
  labels: HubBrowseLabels;
  createLabels: CreatorShellInlineCreateLabels;
  testId: string;
  interactive?: boolean;
}) {
  const [shellTab, setShellTab] = useState<ShellSidebarTab>('creator');
  const [activeTab, setActiveTab] = useState<HubTab>(initialTab);
  const [worldItems, setWorldItems] = useState(worlds);
  const [workItems, setWorkItems] = useState(works);

  const panelContent = (
    <CreatorShellContent
      mode="create-inline"
      canCreateWorld={false}
      labels={createLabels}
      onWorldSubmit={() => {}}
      onWorkSubmit={() => {}}
      data-testid="sidebar-create-panel"
    />
  );

  return (
    <div
      className="flex min-h-[480px] overflow-hidden rounded-card border border-gray-alpha-300 bg-background-100"
      data-testid={testId}
    >
      <div className="flex w-sidebar-nav-width shrink-0 flex-col">
        <ShellSidebarChrome
          activeTab={shellTab}
          activeRoute="/works"
          navGroups={[]}
          onTabChange={setShellTab}
          logo={<StudioShellLogo />}
          panelContent={shellTab === 'creator' ? panelContent : undefined}
          footer={<FixtureWorkspaceFooter testId={`${testId}-workspace-footer`} />}
          creatorTabLabel="创作"
          orchestratorTabLabel="编排"
          primaryNavigationAriaLabel="主导航"
          data-testid={`${testId}-sidebar`}
        />
      </div>

      <div
        className="flex min-w-0 flex-1 flex-col bg-background-200"
        data-testid={`${testId}-hub-browse`}
      >
        <HubTabBar
          activeTab={activeTab}
          onTabChange={setActiveTab}
          labels={labels.tabs}
          ariaLabel="Creator hub entity kind"
          data-testid={`${testId}-tab-bar`}
        />
        <div
          id="hub-tabpanel"
          role="tabpanel"
          aria-labelledby={`hub-tab-${activeTab}`}
          className="min-h-0 flex-1"
          data-testid={`${testId}-tabpanel`}
        >
          <HubCardListPane
            activeTab={activeTab}
            worlds={worldItems}
            works={workItems}
            labels={labels.cardList}
            onSelectCard={
              interactive
                ? (id) => {
                    if (activeTab === 'world') {
                      setWorldItems((items) =>
                        items.map((item) =>
                          item.id === id ? { ...item, label: `${item.label} ✓` } : item,
                        ),
                      );
                    } else {
                      setWorkItems((items) =>
                        items.map((item) =>
                          item.id === id ? { ...item, label: `${item.label} ✓` } : item,
                        ),
                      );
                    }
                  }
                : undefined
            }
            data-testid={`${testId}-card-list-pane`}
          />
        </div>
      </div>
    </div>
  );
}

function VariantMatrixCell({
  state,
  theme,
  labels,
  createLabels,
}: {
  state: VariantState;
  theme: 'light' | 'dark';
  labels: HubBrowseLabels;
  createLabels: CreatorShellInlineCreateLabels;
}) {
  const slug = state.label.toLowerCase().replace(/[^a-z0-9]+/g, '-');
  const testId = `creator-hub-dual-pane-ia-matrix-${slug}-${theme}`;

  return (
    <div
      className={theme === 'dark' ? 'dark rounded-card border border-gray-alpha-300 bg-background-100 p-2' : 'rounded-card border border-gray-alpha-300 bg-background-100 p-2'}
      data-testid={testId}
    >
      <p className="mb-2 px-2 pt-2 text-label-12 font-medium text-gray-700">
        {state.label} · {theme}
      </p>
      <HubSidebarBrowseFrame
        activeTab={state.activeTab}
        worlds={state.worlds}
        works={state.works}
        labels={labels}
        createLabels={createLabels}
        testId={`${testId}-frame`}
      />
    </div>
  );
}

export function CreatorHubDualPaneIaFixtures() {
  return (
    <div data-testid="creator-hub-dual-pane-ia-fixtures">
      <FixtureFrame
        title="Hub IA — sidebar inline create + content browse (World tab)"
        description="V1.136 P1: sidebar hosts World|Work create tabs + inline form + submit. Content is browse-only: World/Work tab bar + card list or empty — no content-left create form."
        testId="creator-hub-dual-pane-ia-fixture-world-tab"
      >
        <div className="mb-6 flex flex-col gap-6">
          <div>
            <p className="mb-2 text-label-12 font-medium uppercase tracking-wide text-gray-600">
              Empty — en
            </p>
            <ThemePair
              testId="creator-hub-dual-pane-ia-world-empty-themes"
              light={
                <HubSidebarBrowseFrame
                  activeTab="world"
                  worlds={[]}
                  works={[]}
                  labels={HUB_LABELS_EN}
                  createLabels={INLINE_CREATE_LABELS_EN}
                  testId="creator-hub-dual-pane-ia-world-empty-light"
                />
              }
              dark={
                <HubSidebarBrowseFrame
                  activeTab="world"
                  worlds={[]}
                  works={[]}
                  labels={HUB_LABELS_EN}
                  createLabels={INLINE_CREATE_LABELS_EN}
                  testId="creator-hub-dual-pane-ia-world-empty-dark"
                />
              }
            />
          </div>
          <div>
            <p className="mb-2 text-label-12 font-medium uppercase tracking-wide text-gray-600">
              Populated — zh-CN tabs
            </p>
            <ThemePair
              testId="creator-hub-dual-pane-ia-world-populated-themes"
              light={
                <HubSidebarBrowseFrame
                  activeTab="world"
                  worlds={SAMPLE_WORLDS}
                  works={SAMPLE_WORKS}
                  labels={HUB_LABELS_ZH}
                  createLabels={INLINE_CREATE_LABELS_ZH}
                  testId="creator-hub-dual-pane-ia-world-populated-light"
                />
              }
              dark={
                <HubSidebarBrowseFrame
                  activeTab="world"
                  worlds={SAMPLE_WORLDS}
                  works={SAMPLE_WORKS}
                  labels={HUB_LABELS_ZH}
                  createLabels={INLINE_CREATE_LABELS_ZH}
                  testId="creator-hub-dual-pane-ia-world-populated-dark"
                />
              }
            />
          </div>
        </div>
      </FixtureFrame>

      <FixtureFrame
        title="Hub IA — sidebar inline create + content browse (Work tab)"
        description="Work tab active: sidebar still hosts inline create; content shows Work cards only (World cards hidden). Empty copy uses hub.empty.works and points to sidebar create."
        testId="creator-hub-dual-pane-ia-fixture-work-tab"
      >
        <div className="mb-6 flex flex-col gap-6">
          <div>
            <p className="mb-2 text-label-12 font-medium uppercase tracking-wide text-gray-600">
              Empty — zh-CN
            </p>
            <ThemePair
              testId="creator-hub-dual-pane-ia-work-empty-themes"
              light={
                <HubSidebarBrowseFrame
                  activeTab="work"
                  worlds={SAMPLE_WORLDS}
                  works={[]}
                  labels={HUB_LABELS_ZH}
                  createLabels={INLINE_CREATE_LABELS_ZH}
                  testId="creator-hub-dual-pane-ia-work-empty-light"
                />
              }
              dark={
                <HubSidebarBrowseFrame
                  activeTab="work"
                  worlds={SAMPLE_WORLDS}
                  works={[]}
                  labels={HUB_LABELS_ZH}
                  createLabels={INLINE_CREATE_LABELS_ZH}
                  testId="creator-hub-dual-pane-ia-work-empty-dark"
                />
              }
            />
          </div>
          <div>
            <p className="mb-2 text-label-12 font-medium uppercase tracking-wide text-gray-600">
              Populated — en
            </p>
            <ThemePair
              testId="creator-hub-dual-pane-ia-work-populated-themes"
              light={
                <HubSidebarBrowseFrame
                  activeTab="work"
                  worlds={SAMPLE_WORLDS}
                  works={SAMPLE_WORKS}
                  labels={HUB_LABELS_EN}
                  createLabels={INLINE_CREATE_LABELS_EN}
                  testId="creator-hub-dual-pane-ia-work-populated-light"
                />
              }
              dark={
                <HubSidebarBrowseFrame
                  activeTab="work"
                  worlds={SAMPLE_WORLDS}
                  works={SAMPLE_WORKS}
                  labels={HUB_LABELS_EN}
                  createLabels={INLINE_CREATE_LABELS_EN}
                  testId="creator-hub-dual-pane-ia-work-populated-dark"
                />
              }
            />
          </div>
        </div>
      </FixtureFrame>

      <FixtureFrame
        title="8-variant acceptance matrix (2 tabs × 2 content × 2 themes)"
        description="PAC-5 minimum matrix. Each cell: sidebar-create-panel with create-inline mode; browse-only content; no workspace-pane-inline-form."
        testId="creator-hub-dual-pane-ia-fixture-matrix"
      >
        <div
          className="grid grid-cols-1 gap-4 xl:grid-cols-2"
          data-testid="creator-hub-dual-pane-ia-variant-matrix"
        >
          {VARIANT_STATES.flatMap((state) => [
            <VariantMatrixCell
              key={`${state.label}-light`}
              state={state}
              theme="light"
              labels={HUB_LABELS_EN}
              createLabels={INLINE_CREATE_LABELS_EN}
            />,
            <VariantMatrixCell
              key={`${state.label}-dark`}
              state={state}
              theme="dark"
              labels={HUB_LABELS_EN}
              createLabels={INLINE_CREATE_LABELS_EN}
            />,
          ])}
        </div>
      </FixtureFrame>

      <FixtureFrame
        title="Interactive — tab switch + card select"
        description="Content tab switches update browse list only. Sidebar inline create remains in panelContent. Card click marks selection in browse list."
        testId="creator-hub-dual-pane-ia-fixture-interactive"
      >
        <HubSidebarBrowseFrame
          activeTab="world"
          worlds={SAMPLE_WORLDS}
          works={SAMPLE_WORKS}
          labels={HUB_LABELS_ZH}
          createLabels={INLINE_CREATE_LABELS_ZH}
          interactive
          testId="creator-hub-dual-pane-ia-interactive"
        />
      </FixtureFrame>
    </div>
  );
}
