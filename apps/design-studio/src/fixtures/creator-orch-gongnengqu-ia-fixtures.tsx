/**
 * Studio fixtures for V1.132 P3 Creator / Orchestrator 功能区 IA (AC-9).
 *
 * Proves grill A composition in light + dark:
 * - 创作 hub left = Create-only (创建 World / 延续 Work) — not Menu nav (世界/作品)
 * - Worlds / Works = right-side content lists
 * - 工作区 footer visible under both 创作 and 编排
 *
 * Prop-driven frames compose `@web-layout/creator-shell-content`,
 * `@web-layout/shell-sidebar-chrome`, and `@web-layout/footer-profiles-chrome`
 * without App layout providers or daemon hooks.
 */
import { useRef, useState, type KeyboardEvent, type ReactNode } from 'react';

import { Globe, Layers, type LucideIcon } from 'lucide-react';

import { cn } from '@42ch/nexus-ui';

import { StudioShellLogo } from '@/components/studio-shell-logo';

import {
  CreatorShellContent,
  type CreatorEntityRef,
} from '@web-layout/creator-shell-content';
import {
  FooterProfilesChrome,
  type FooterProfile,
} from '@web-layout/footer-profiles-chrome';
import {
  ShellSidebarChrome,
  type ShellSidebarTab,
} from '@web-layout/shell-sidebar-chrome';

import { ORCHESTRATOR_NAV } from '@/fixtures/shell-nav-data';

const CREATE_LABELS = {
  createWorldTitle: '创建 World',
  createWorldDescription: '在本地运行时中启动一个新世界。',
  createWorkTitle: '延续 Work',
  createWorkDescription: '创建一部作品以开始创作——世界将从你的作品中诞生。',
  createWorldDisabledTitle: '创建 World 仅在 Nexus 桌面应用中可用。',
} as const;

const SAMPLE_WORLDS: { id: string; label: string }[] = [
  { id: 'world-fantasy', label: '奇幻大陆' },
  { id: 'world-scifi', label: '近未来都市' },
];

const SAMPLE_WORKS: { id: string; label: string }[] = [
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

function GongnengquIaModeSwitch({
  activeTab,
  onTabChange,
  footer,
  testId,
}: {
  activeTab: ShellSidebarTab;
  onTabChange: (tab: ShellSidebarTab) => void;
  footer: ReactNode;
  testId?: string;
}) {
  return (
    <div
      className="mt-auto border-t border-gray-alpha-400 pt-2"
      data-testid={testId ?? 'gongnengqu-ia-mode-switch'}
    >
      <div
        className="grid grid-cols-2 gap-1 rounded-card bg-gray-alpha-100 p-1"
        role="tablist"
        aria-label="主导航"
        data-testid="shell-mode-switch"
      >
        {(['creator', 'orchestrator'] as const).map((tab) => {
          const label = tab === 'creator' ? '创作' : '编排';
          const active = activeTab === tab;
          return (
            <button
              key={tab}
              type="button"
              id={tab}
              role="tab"
              aria-selected={active}
              onClick={() => onTabChange(tab)}
              className={cn(
                'rounded-control px-2 py-1.5 text-button-14 font-button transition-colors duration-state ease-standard motion-reduce:transition-none',
                active
                  ? 'bg-brand-cyan text-brand-deep-blue shadow-card'
                  : 'text-gray-700 hover:bg-gray-alpha-200 hover:text-gray-1000',
              )}
            >
              {label}
            </button>
          );
        })}
      </div>
      <div className="mt-2 flex flex-col gap-2">{footer}</div>
    </div>
  );
}

function FixtureWorkspaceFooter({ testId }: { testId?: string }) {
  const [activeId, setActiveId] = useState('local-creator');
  const [focusIndex, setFocusIndex] = useState(0);
  const itemRefs = useRef<(HTMLButtonElement | null)[]>([]);
  const addRef = useRef<HTMLButtonElement | null>(null);

  const profiles: FooterProfile[] = [
    {
      id: 'local-creator',
      displayName: '本地创作者',
      active: activeId === 'local-creator',
    },
  ];
  const total = profiles.length + 1;

  function focusAt(index: number) {
    const next = Math.max(0, Math.min(total - 1, index));
    const el = next === profiles.length ? addRef.current : itemRefs.current[next];
    el?.focus();
    setFocusIndex(next);
  }

  function handleKeyDown(event: KeyboardEvent<HTMLDivElement>) {
    switch (event.key) {
      case 'ArrowRight':
        event.preventDefault();
        focusAt(focusIndex + 1);
        break;
      case 'ArrowLeft':
        event.preventDefault();
        focusAt(focusIndex - 1);
        break;
      case 'Home':
        event.preventDefault();
        focusAt(0);
        break;
      case 'End':
        event.preventDefault();
        focusAt(total - 1);
        break;
      default:
        break;
    }
  }

  return (
    <div data-testid={testId ?? 'gongnengqu-ia-workspace-footer'}>
      <FooterProfilesChrome
        sectionLabel="工作区"
        addButtonLabel="添加创作者"
        profiles={profiles}
        focusIndex={focusIndex}
        onSelect={setActiveId}
        onAdd={() => {}}
        onFocus={setFocusIndex}
        onKeyDown={handleKeyDown}
        onItemRef={(index, el) => {
          itemRefs.current[index] = el;
        }}
        onAddRef={(el) => {
          addRef.current = el;
        }}
      />
    </div>
  );
}

function EntityListSection({
  title,
  icon: Icon,
  items,
  selectedId,
  onSelect,
  testId,
}: {
  title: string;
  icon: LucideIcon;
  items: { id: string; label: string }[];
  selectedId?: string | null;
  onSelect?: (id: string) => void;
  testId: string;
}) {
  return (
    <section data-testid={testId} className="flex flex-col gap-2">
      <h3 className="flex items-center gap-2 px-1 text-label-12 font-medium uppercase tracking-wide text-gray-600">
        <Icon className="h-3.5 w-3.5" aria-hidden />
        {title}
      </h3>
      <ul className="flex flex-col gap-1" role="list">
        {items.map((item) => {
          const selected = selectedId === item.id;
          return (
            <li key={item.id}>
              <button
                type="button"
                data-testid={`${testId}-row-${item.id}`}
                aria-pressed={selected}
                onClick={() => onSelect?.(item.id)}
                className={cn(
                  'flex w-full items-center rounded-control px-3 py-2 text-left text-label-14 transition-colors duration-state ease-standard motion-reduce:transition-none',
                  selected
                    ? 'bg-gray-alpha-100 text-gray-1000'
                    : 'text-gray-700 hover:bg-gray-alpha-100 hover:text-gray-1000',
                )}
              >
                {item.label}
              </button>
            </li>
          );
        })}
      </ul>
    </section>
  );
}

function EntityListsPanel({
  selectedEntity,
  onSelectWorld,
  onSelectWork,
  testId = 'gongnengqu-ia-entity-lists',
}: {
  selectedEntity?: CreatorEntityRef | null;
  onSelectWorld?: (id: string) => void;
  onSelectWork?: (id: string) => void;
  testId?: string;
}) {
  const selectedWorldId =
    selectedEntity?.kind === 'world' ? selectedEntity.id : null;
  const selectedWorkId =
    selectedEntity?.kind === 'work' ? selectedEntity.id : null;

  return (
    <div
      data-testid={testId}
      className="flex h-full w-full flex-col gap-6 overflow-auto p-6"
    >
      <EntityListSection
        title="世界"
        icon={Globe}
        items={SAMPLE_WORLDS}
        selectedId={selectedWorldId}
        onSelect={onSelectWorld}
        testId={`${testId}-worlds`}
      />
      <EntityListSection
        title="作品"
        icon={Layers}
        items={SAMPLE_WORKS}
        selectedId={selectedWorkId}
        onSelect={onSelectWork}
        testId={`${testId}-works`}
      />
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
  const [selectedEntity, setSelectedEntity] = useState<CreatorEntityRef | null>(null);

  const footer = <FixtureWorkspaceFooter />;

  function handleSelectWorld(id: string) {
    if (!interactive) return;
    const world = SAMPLE_WORLDS.find((item) => item.id === id);
    if (world) {
      setSelectedEntity({ kind: 'world', id: world.id, label: world.label });
    }
  }

  function handleSelectWork(id: string) {
    if (!interactive) return;
    const work = SAMPLE_WORKS.find((item) => item.id === id);
    if (work) {
      setSelectedEntity({ kind: 'work', id: work.id, label: work.label });
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
        <EntityListsPanel
          selectedEntity={selectedEntity}
          onSelectWorld={handleSelectWorld}
          onSelectWork={handleSelectWork}
          testId={`${testId}-entity-lists`}
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
        {activeTab === 'creator' ? (
          <div
            className="flex h-full min-h-0 flex-col border-r border-gray-alpha-400 bg-background-100 p-3"
            data-testid={`${testId}-creator-sidebar`}
          >
            <div className="flex h-12 items-center px-3">
              <StudioShellLogo />
            </div>
            <div
              className="flex flex-1 flex-col overflow-auto py-2"
              data-testid={`${testId}-create-left`}
            >
              <CreatorShellContent
                mode="create"
                canCreateWorld={false}
                labels={CREATE_LABELS}
                onCreateWork={() => {}}
                data-testid={`${testId}-create-content`}
              />
            </div>
            <GongnengquIaModeSwitch
              activeTab={activeTab}
              onTabChange={setActiveTab}
              footer={footer}
              testId={`${testId}-mode-switch`}
            />
          </div>
        ) : (
          <ShellSidebarChrome
            activeTab={activeTab}
            activeRoute="#memory"
            navGroups={ORCHESTRATOR_NAV}
            onTabChange={setActiveTab}
            logo={<StudioShellLogo />}
            footer={footer}
            creatorTabLabel="创作"
            orchestratorTabLabel="编排"
            primaryNavigationAriaLabel="主导航"
            data-testid={`${testId}-orchestrator-sidebar`}
          />
        )}
      </div>

      <div className="flex min-w-0 flex-1 flex-col bg-background-200">{mainContent}</div>
    </div>
  );
}

export function CreatorOrchGongnengquIaFixtures() {
  return (
    <div data-testid="creator-orch-gongnengqu-ia-fixtures">
      <FixtureFrame
        title="创作 hub — Create left + lists right (light / dark)"
        description="Grill A: 创作左侧仅 创建 World / 延续 Work；世界与作品列表在右侧内容区。无左侧 Menu 导航（世界/作品）。"
        testId="gongnengqu-ia-fixture-creator-hub"
      >
        <ThemePair
          testId="gongnengqu-ia-creator-hub-themes"
          light={
            <GongnengquIaShellFrame
              activeTab="creator"
              testId="gongnengqu-ia-creator-hub-light"
            />
          }
          dark={
            <GongnengquIaShellFrame
              activeTab="creator"
              testId="gongnengqu-ia-creator-hub-dark"
            />
          }
        />
      </FixtureFrame>

      <FixtureFrame
        title="编排 — Orchestrator nav + 工作区 footer (light / dark)"
        description="编排模式保留 Memory / Runtime / Strategies 左侧导航；工作区 footer 与 创作|编排 模式切换始终可见。"
        testId="gongnengqu-ia-fixture-orchestrator"
      >
        <ThemePair
          testId="gongnengqu-ia-orchestrator-themes"
          light={
            <GongnengquIaShellFrame
              activeTab="orchestrator"
              testId="gongnengqu-ia-orchestrator-light"
            />
          }
          dark={
            <GongnengquIaShellFrame
              activeTab="orchestrator"
              testId="gongnengqu-ia-orchestrator-dark"
            />
          }
        />
      </FixtureFrame>

      <FixtureFrame
        title="Interactive — list row → entity mode; 工作区 across modes"
        description="选择右侧列表行进入实体模式（Controller stub + 返回）；切换 创作/编排 时工作区 footer 保持可见。"
        testId="gongnengqu-ia-fixture-interactive"
      >
        <GongnengquIaShellFrame
          activeTab="creator"
          interactive
          testId="gongnengqu-ia-interactive"
        />
      </FixtureFrame>
    </div>
  );
}
