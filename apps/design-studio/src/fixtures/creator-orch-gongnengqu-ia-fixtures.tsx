/**
 * Studio fixtures for V1.132 P3 Creator / Orchestrator 功能区 IA (AC-9).
 * V1.136 P1: creator hub sidebar uses inline create (World|Work tabs + form + submit);
 * content area uses World/Work tab bar + card list (browse-only).
 * Single frame follows the Studio theme toggle — no Light|Dark side-by-side matrix.
 *
 * Prop-driven frames compose `@web-layout/creator-shell-content`,
 * `@web-layout/shell-sidebar-chrome`, `@web-layout/hub-tab-bar`,
 * `@web-layout/hub-card-list-pane`, and `@web-layout/footer-profiles-chrome`
 * without App layout providers or daemon hooks.
 */
import { useState, type ReactNode } from 'react';

import { useTheme } from '@/components/theme-provider';
import { StudioShellLogo } from '@/components/studio-shell-logo';

import {
  CreatorShellContent,
  type CreatorEntityRef,
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
import { ORCHESTRATOR_NAV } from '@/fixtures/shell-nav-data';

const INLINE_CREATE_LABELS = {
  tabs: { world: '世界', work: '作品' } satisfies HubTabBarLabels,
  world: {
    titleLabel: '世界标题',
    titlePlaceholder: '输入世界名称',
    submit: '创建世界',
    disabledTitle: '创建 World 仅在 Nexus 桌面应用中可用。',
  },
  work: {
    titleLabel: '作品标题',
    titlePlaceholder: '输入作品名称',
    goalLabel: '长期目标',
    goalPlaceholder: '这部作品想达成什么？',
    ideaLabel: '初始想法',
    ideaPlaceholder: '从一个场景或概念开始…',
    profileLabel: '作品类型（可选）',
    profileOptions: [
      { value: 'novel', label: '小说' },
      { value: 'essay', label: '随笔' },
    ],
    submit: '创建作品',
  },
} satisfies CreatorShellInlineCreateLabels;

const HUB_BROWSE_LABELS = {
  tabs: { world: '世界', work: '作品' } satisfies HubTabBarLabels,
  cardList: {
    emptyWorlds: '暂无世界，从侧边栏创建',
    emptyWorks: '暂无作品，从侧边栏创建',
    emptyWorldsKey: 'hub.empty.worlds',
    emptyWorksKey: 'hub.empty.works',
  } satisfies HubCardListPaneLabels,
};

const SAMPLE_WORLDS: HubCardListItem[] = [
  { id: 'world-fantasy', label: '奇幻大陆' },
  { id: 'world-scifi', label: '近未来都市' },
];

const SAMPLE_WORKS: HubCardListItem[] = [
  { id: 'work-novel', label: '漫漫长路' },
  { id: 'work-essay', label: '随笔集' },
];

const CONTROLLER_LABELS_BASE = {
  title: '控制面板',
  description: '控制面板 — 即将推出',
  back: '返回',
} as const;

function controllerLabels(entity: CreatorEntityRef) {
  const kind = entity.kind === 'world' ? '世界' : '作品';
  return {
    ...CONTROLLER_LABELS_BASE,
    selectedSummary: `已选${kind}：${entity.label}`,
  };
}

function ThemeCaption() {
  const { resolvedTheme } = useTheme();
  const label = resolvedTheme === 'dark' ? 'Dark' : 'Light';

  return (
    <p
      className="mb-3 text-label-14 font-medium text-gray-700 dark:text-brand-cyan"
      data-testid="gongnengqu-ia-theme-caption"
    >
      Theme: {label} — follows Studio toggle
    </p>
  );
}

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

function HubBrowseContent({
  activeTab,
  onTabChange,
  worlds,
  works,
  onSelectCard,
  testId,
}: {
  activeTab: HubTab;
  onTabChange: (tab: HubTab) => void;
  worlds: HubCardListItem[];
  works: HubCardListItem[];
  onSelectCard?: (id: string) => void;
  testId: string;
}) {
  return (
    <div className="flex min-h-0 flex-1 flex-col bg-background-200" data-testid={testId}>
      <HubTabBar
        activeTab={activeTab}
        onTabChange={onTabChange}
        labels={HUB_BROWSE_LABELS.tabs}
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
          worlds={worlds}
          works={works}
          labels={HUB_BROWSE_LABELS.cardList}
          onSelectCard={onSelectCard}
          data-testid={`${testId}-card-list-pane`}
        />
      </div>
    </div>
  );
}

function GongnengquIaShellFrame({
  activeTab: initialTab = 'creator',
  testId,
  interactive = false,
}: {
  activeTab?: ShellSidebarTab;
  testId: string;
  interactive?: boolean;
}) {
  const [activeTab, setActiveTab] = useState<ShellSidebarTab>(initialTab);
  const [hubTab, setHubTab] = useState<HubTab>('world');
  const [selectedEntity, setSelectedEntity] = useState<CreatorEntityRef | null>(null);

  const footer = <FixtureWorkspaceFooter testId={`${testId}-workspace-footer`} />;

  const createPanel = (
    <CreatorShellContent
      mode="create-inline"
      canCreateWorld={false}
      labels={INLINE_CREATE_LABELS}
      onWorldSubmit={() => {}}
      onWorkSubmit={() => {}}
      data-testid="sidebar-create-panel"
    />
  );

  function handleSelectCard(id: string) {
    if (!interactive) return;
    if (hubTab === 'world') {
      const world = SAMPLE_WORLDS.find((item) => item.id === id);
      if (world) {
        setSelectedEntity({ kind: 'world', id: world.id, label: world.label });
      }
    } else {
      const work = SAMPLE_WORKS.find((item) => item.id === id);
      if (work) {
        setSelectedEntity({ kind: 'work', id: work.id, label: work.label });
      }
    }
  }

  const mainContent =
    activeTab === 'creator' ? (
      selectedEntity ? (
        <CreatorShellContent
          mode="controller"
          selectedEntity={selectedEntity}
          labels={controllerLabels(selectedEntity)}
          onBack={() => setSelectedEntity(null)}
          data-testid={`${testId}-controller-content`}
        />
      ) : (
        <HubBrowseContent
          activeTab={hubTab}
          onTabChange={setHubTab}
          worlds={SAMPLE_WORLDS}
          works={SAMPLE_WORKS}
          onSelectCard={handleSelectCard}
          testId={`${testId}-hub-browse`}
        />
      )
    ) : (
      <div
        data-testid={`${testId}-orchestrator-content`}
        className="flex h-full flex-col items-center justify-center p-8"
      >
        <p className="text-label-14 text-gray-900">编排模式</p>
        <p className="mt-2 max-w-sm text-center text-copy-13 text-gray-700">
          Memory / Runtime / Strategies 导航在左侧；工作区 footer 在两种模式下均可见。
        </p>
      </div>
    );

  return (
    <div
      className="flex min-h-[480px] overflow-hidden rounded-card border border-gray-alpha-300 bg-background-100"
      data-testid={testId}
    >
      <div className="flex w-sidebar-nav-width shrink-0 flex-col">
        <ShellSidebarChrome
          activeTab={activeTab}
          activeRoute={activeTab === 'orchestrator' ? '#memory' : '/works'}
          navGroups={activeTab === 'orchestrator' ? ORCHESTRATOR_NAV : []}
          onTabChange={setActiveTab}
          logo={<StudioShellLogo />}
          panelContent={activeTab === 'creator' ? createPanel : undefined}
          footer={footer}
          creatorTabLabel="创作"
          orchestratorTabLabel="编排"
          primaryNavigationAriaLabel="主导航"
          data-testid={`${testId}-sidebar`}
        />
      </div>

      <div className="flex min-w-0 flex-1 flex-col bg-background-200">{mainContent}</div>
    </div>
  );
}

export function CreatorOrchGongnengquIaFixtures() {
  return (
    <div data-testid="creator-orch-gongnengqu-ia-fixtures">
      <FixtureFrame
        title="创作 hub — sidebar inline create + browse content"
        description="V1.136 P1: 创作 sidebar panelContent = World|Work 标签 + 标题输入 + 提交 (sidebar-create-panel). 内容区 = World/Work 标签 + 卡片列表 — 无双栏左侧创建表单。主题随 Studio 切换。"
        testId="gongnengqu-ia-fixture-creator-hub"
      >
        <ThemeCaption />
        <GongnengquIaShellFrame activeTab="creator" testId="gongnengqu-ia-creator-hub" />
      </FixtureFrame>

      <FixtureFrame
        title="编排 — Orchestrator nav + 工作区 footer"
        description="编排模式保留 Memory / Runtime / Strategies 左侧导航；工作区 footer 与 创作|编排 模式切换始终可见。主题随 Studio 切换。"
        testId="gongnengqu-ia-fixture-orchestrator"
      >
        <ThemeCaption />
        <GongnengquIaShellFrame activeTab="orchestrator" testId="gongnengqu-ia-orchestrator" />
      </FixtureFrame>

      <FixtureFrame
        title="Interactive — card select → entity mode; 工作区 across modes"
        description="选择内容区卡片进入实体模式（Controller stub + 返回）；切换 创作/编排 时工作区 footer 保持可见。"
        testId="gongnengqu-ia-fixture-interactive"
      >
        <ThemeCaption />
        <GongnengquIaShellFrame
          activeTab="creator"
          interactive
          testId="gongnengqu-ia-interactive"
        />
      </FixtureFrame>
    </div>
  );
}
